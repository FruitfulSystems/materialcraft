//! Function graph evaluation: a [`FnGraph`] is compiled once into a flat instruction list
//! (dependency order from the output node), then run per pixel with a small register file. All
//! values are 4-wide; scalars are broadcast to every lane, so component-wise ops need no type
//! checks. The pixel processor samples its inputs at any position; the value processor runs once.

use std::collections::BTreeMap;

use materialcraft_core::Value;
use materialcraft_core::function::FnGraph;
use materialcraft_image::{Image, Kind};

use crate::catalog::{CookResult, Ctx};
use crate::noise;

/// Function node operations: (id, label, input count, library category).
pub const FN_OPS: &[(&str, &str, usize, &str)] = &[
    ("const_float", "Float", 0, "Constant"),
    ("const_float2", "Float2", 0, "Constant"),
    ("const_float3", "Float3", 0, "Constant"),
    ("const_float4", "Float4", 0, "Constant"),
    ("const_int", "Integer", 0, "Constant"),
    ("const_bool", "Boolean", 0, "Constant"),
    ("get_pos", "$pos", 0, "Variables"),
    ("get_size", "$size", 0, "Variables"),
    ("get_var", "Get variable", 0, "Variables"),
    ("sample_gray", "Sample Gray", 1, "Samplers"),
    ("sample_color", "Sample Color", 1, "Samplers"),
    ("vec2", "Vector Float2", 2, "Vector"),
    ("vec3", "Vector Float3", 3, "Vector"),
    ("vec4", "Vector Float4", 4, "Vector"),
    ("swizzle", "Swizzle", 1, "Vector"),
    ("to_int", "To Integer", 1, "Cast"),
    ("to_float", "To Float", 1, "Cast"),
    ("add", "Add", 2, "Operator"),
    ("sub", "Subtraction", 2, "Operator"),
    ("mul", "Multiplication", 2, "Operator"),
    ("div", "Division", 2, "Operator"),
    ("mod", "Modulo", 2, "Operator"),
    ("neg", "Negation", 1, "Operator"),
    ("dot", "Dot Product", 2, "Operator"),
    ("and", "And", 2, "Logical"),
    ("or", "Or", 2, "Logical"),
    ("not", "Not", 1, "Logical"),
    ("gt", "Greater", 2, "Comparison"),
    ("ge", "Greater or equal", 2, "Comparison"),
    ("lt", "Lower", 2, "Comparison"),
    ("le", "Lower or equal", 2, "Comparison"),
    ("eq", "Equal", 2, "Comparison"),
    ("ne", "Not equal", 2, "Comparison"),
    ("abs", "Absolute", 1, "Function"),
    ("floor", "Floor", 1, "Function"),
    ("ceil", "Ceil", 1, "Function"),
    ("frac", "Fractional part", 1, "Function"),
    ("sqrt", "Square root", 1, "Function"),
    ("exp", "Exponential", 1, "Function"),
    ("log", "Logarithm", 1, "Function"),
    ("log2", "Logarithm base 2", 1, "Function"),
    ("pow2", "2^x", 1, "Function"),
    ("sin", "Sine", 1, "Function"),
    ("cos", "Cosine", 1, "Function"),
    ("tan", "Tangent", 1, "Function"),
    ("atan2", "Arc tangent 2", 2, "Function"),
    ("cartesian", "Cartesian", 2, "Function"),
    ("pow", "Power", 2, "Function"),
    ("min", "Minimum", 2, "Function"),
    ("max", "Maximum", 2, "Function"),
    ("clamp", "Clamp", 3, "Function"),
    ("lerp", "Linear interpolation", 3, "Function"),
    ("length", "Length", 1, "Function"),
    ("normalize", "Normalize", 1, "Function"),
    ("rand", "Random", 1, "Function"),
    ("noise", "Perlin noise", 1, "Function"),
    ("if_else", "If...Else", 3, "Control"),
];

#[derive(Clone, Debug)]
enum Op {
    Const([f32; 4]),
    Pos,
    Size,
    Sample { input: usize, gray: bool },
    Unary(fn(f32) -> f32),
    Binary(fn(f32, f32) -> f32),
    Ternary(fn(f32, f32, f32) -> f32),
    Dot,
    Length,
    Normalize,
    Vec(usize),
    Swizzle([usize; 4]),
    Rand,
    Noise,
    IfElse,
    Cartesian,
}

#[derive(Clone, Debug)]
struct Instr {
    op: Op,
    args: Vec<usize>,
    /// Lanes that carry meaning (1 = scalar).
    dims: usize,
}

/// A compiled function graph.
#[derive(Clone, Debug)]
pub struct Program {
    code: Vec<Instr>,
    seed: u32,
}

fn b(v: bool) -> f32 {
    if v { 1.0 } else { 0.0 }
}

/// Return zero for nonfinite arithmetic results.
fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn dims_of(v: &Value) -> usize {
    match v {
        Value::Float2(_) | Value::Int2(_) => 2,
        Value::Float3(_) => 3,
        Value::Float4(_) | Value::Color(_) => 4,
        _ => 1,
    }
}

fn broadcast(v: &Value) -> [f32; 4] {
    match dims_of(v) {
        1 => [v.as_f32().unwrap_or(0.0); 4],
        _ => v.as_vec4().unwrap_or([0.0; 4]),
    }
}

/// Compile `g` for evaluation. `vars` resolves `get_var` names (exposed graph inputs).
pub fn compile(g: &FnGraph, vars: &BTreeMap<String, Value>, seed: u32) -> Result<Program, String> {
    let out = g.output.ok_or("the function has no output node")?;
    let mut code: Vec<Instr> = Vec::new();
    let mut reg: BTreeMap<u32, usize> = BTreeMap::new();
    // Iterative post-order from the output; a node seen twice on the current path is a cycle.
    let mut stack: Vec<(u32, bool)> = vec![(out, false)];
    let mut on_path = std::collections::BTreeSet::new();
    while let Some((id, done)) = stack.pop() {
        if reg.contains_key(&id) {
            continue;
        }
        let n = g.node(id).ok_or_else(|| format!("missing function node {id}"))?;
        if !done {
            if !on_path.insert(id) {
                return Err("the function graph has a cycle".into());
            }
            stack.push((id, true));
            for i in n.inputs.iter().flatten() {
                if !reg.contains_key(i) {
                    stack.push((*i, false));
                }
            }
            continue;
        }
        on_path.remove(&id);
        if code.len() > 4096 {
            return Err("the function graph is too large".into());
        }
        let args: Vec<usize> = n.inputs.iter().map(|i| i.and_then(|i| reg.get(&i).copied())).map(|r| r.unwrap_or(usize::MAX)).collect();
        let arity = FN_OPS.iter().find(|o| o.0 == n.op).map(|o| o.2).ok_or_else(|| format!("unknown function op `{}`", n.op))?;
        if args.len() < arity || args.iter().take(arity).any(|a| *a == usize::MAX) {
            return Err(format!("`{}` needs {arity} connected input(s)", n.op));
        }
        let dim = |k: usize| args.get(k).and_then(|r| code.get(*r)).map_or(1, |i: &Instr| i.dims);
        let maxd = (0..arity).map(dim).max().unwrap_or(1);
        let (op, dims) = match n.op.as_str() {
            o if o.starts_with("const_") => {
                let v = n.value.clone().unwrap_or(Value::Float(0.0));
                (Op::Const(broadcast(&v)), dims_of(&v))
            }
            "get_pos" => (Op::Pos, 2),
            "get_size" => (Op::Size, 2),
            "get_var" => {
                let name = match &n.value {
                    Some(Value::Text(s)) => s.as_str(),
                    _ => "",
                };
                let v = vars.get(name).cloned().unwrap_or(Value::Float(0.0));
                (Op::Const(broadcast(&v)), dims_of(&v))
            }
            "sample_gray" | "sample_color" => {
                let input = n.value.as_ref().and_then(Value::as_i32).unwrap_or(0).clamp(0, 3) as usize;
                let gray = n.op == "sample_gray";
                (Op::Sample { input, gray }, if gray { 1 } else { 4 })
            }
            "vec2" => (Op::Vec(2), 2),
            "vec3" => (Op::Vec(3), 3),
            "vec4" => (Op::Vec(4), 4),
            "swizzle" => {
                let s = match &n.value {
                    Some(Value::Text(s)) => s.clone(),
                    _ => "x".into(),
                };
                let mut idx = [0usize; 4];
                let mut len = 0;
                for (k, ch) in s.chars().take(4).enumerate() {
                    idx[k] = match ch {
                        'y' | 'g' => 1,
                        'z' | 'b' => 2,
                        'w' | 'a' => 3,
                        _ => 0,
                    };
                    len = k + 1;
                }
                if len == 1 {
                    idx = [idx[0]; 4];
                }
                (Op::Swizzle(idx), len.max(1))
            }
            "to_int" | "floor" => (Op::Unary(f32::floor), maxd),
            "to_float" => (Op::Unary(|a| a), maxd),
            "add" => (Op::Binary(|a, b| a + b), maxd),
            "sub" => (Op::Binary(|a, b| a - b), maxd),
            "mul" => (Op::Binary(|a, b| a * b), maxd),
            "div" => (Op::Binary(|a, b| if b == 0.0 { 0.0 } else { a / b }), maxd),
            "mod" => (Op::Binary(|a, b| if b == 0.0 { 0.0 } else { a.rem_euclid(b) }), maxd),
            "neg" => (Op::Unary(|a| -a), maxd),
            "dot" => (Op::Dot, 1),
            "and" => (Op::Binary(|a, x| b(a != 0.0 && x != 0.0)), maxd),
            "or" => (Op::Binary(|a, x| b(a != 0.0 || x != 0.0)), maxd),
            "not" => (Op::Unary(|a| b(a == 0.0)), maxd),
            "gt" => (Op::Binary(|a, x| b(a > x)), maxd),
            "ge" => (Op::Binary(|a, x| b(a >= x)), maxd),
            "lt" => (Op::Binary(|a, x| b(a < x)), maxd),
            "le" => (Op::Binary(|a, x| b(a <= x)), maxd),
            "eq" => (Op::Binary(|a, x| b(a == x)), maxd),
            "ne" => (Op::Binary(|a, x| b(a != x)), maxd),
            "abs" => (Op::Unary(f32::abs), maxd),
            "ceil" => (Op::Unary(f32::ceil), maxd),
            "frac" => (Op::Unary(|a| a - a.floor()), maxd),
            "sqrt" => (Op::Unary(|a| a.max(0.0).sqrt()), maxd),
            "exp" => (Op::Unary(f32::exp), maxd),
            "log" => (Op::Unary(|a| if a > 0.0 { a.ln() } else { 0.0 }), maxd),
            "log2" => (Op::Unary(|a| if a > 0.0 { finite_or_zero(a.log2()) } else { 0.0 }), maxd),
            "pow2" => (Op::Unary(|a| finite_or_zero(a.exp2())), maxd),
            "sin" => (Op::Unary(f32::sin), maxd),
            "cos" => (Op::Unary(f32::cos), maxd),
            "tan" => (Op::Unary(f32::tan), maxd),
            "atan2" => (Op::Binary(f32::atan2), maxd),
            "cartesian" => (Op::Cartesian, 2),
            "pow" => (Op::Binary(|a, x| if a < 0.0 && x.fract() != 0.0 { 0.0 } else { a.powf(x) }), maxd),
            "min" => (Op::Binary(f32::min), maxd),
            "max" => (Op::Binary(f32::max), maxd),
            "clamp" => (Op::Ternary(|a, lo, hi| a.max(lo).min(hi.max(lo))), maxd),
            "lerp" => (Op::Ternary(|a, x, t| a + (x - a) * t), maxd.max(dim(0)).max(dim(1))),
            "length" => (Op::Length, 1),
            "normalize" => (Op::Normalize, dim(0)),
            "rand" => (Op::Rand, dim(0)),
            "noise" => (Op::Noise, 1),
            "if_else" => (Op::IfElse, dim(1).max(dim(2))),
            other => return Err(format!("unknown function op `{other}`")),
        };
        reg.insert(id, code.len());
        code.push(Instr { op, args: args.into_iter().take(arity).collect(), dims });
    }
    Ok(Program { code, seed })
}

impl Program {
    /// Lanes of the result.
    pub fn dims(&self) -> usize {
        self.code.last().map_or(1, |i| i.dims)
    }

    /// Run at normalized position `pos` of an image of `size`, sampling `inputs`.
    pub fn eval(&self, pos: [f32; 2], size: [f32; 2], inputs: &[Option<Image>], tile: bool, regs: &mut Vec<[f32; 4]>) -> [f32; 4] {
        regs.clear();
        for ins in &self.code {
            let a = |k: usize| ins.args.get(k).and_then(|r| regs.get(*r)).copied().unwrap_or([0.0; 4]);
            let v = match &ins.op {
                Op::Const(c) => *c,
                Op::Pos => [pos[0], pos[1], 0.0, 0.0],
                Op::Size => [size[0], size[1], 0.0, 0.0],
                Op::Sample { input, gray } => {
                    let p = a(0);
                    match inputs.get(*input).and_then(Option::as_ref) {
                        Some(img) if *gray => [img.sample_gray(p[0], p[1], tile); 4],
                        Some(img) => img.sample(p[0], p[1], tile),
                        None => [0.0, 0.0, 0.0, 1.0],
                    }
                }
                Op::Unary(f) => a(0).map(f),
                Op::Binary(f) => {
                    let (x, y) = (a(0), a(1));
                    [f(x[0], y[0]), f(x[1], y[1]), f(x[2], y[2]), f(x[3], y[3])]
                }
                Op::Ternary(f) => {
                    let (x, y, z) = (a(0), a(1), a(2));
                    [f(x[0], y[0], z[0]), f(x[1], y[1], z[1]), f(x[2], y[2], z[2]), f(x[3], y[3], z[3])]
                }
                Op::Dot => {
                    let (x, y) = (a(0), a(1));
                    let n = ins.args.iter().filter_map(|r| self.code.get(*r)).map(|i| i.dims).max().unwrap_or(1);
                    [(0..n).map(|k| x[k] * y[k]).sum(); 4]
                }
                Op::Length => {
                    let x = a(0);
                    let n = ins.args.first().and_then(|r| self.code.get(*r)).map_or(1, |i| i.dims);
                    [(0..n).map(|k| x[k] * x[k]).sum::<f32>().sqrt(); 4]
                }
                Op::Normalize => {
                    let x = a(0);
                    let l = (0..ins.dims).map(|k| x[k] * x[k]).sum::<f32>().sqrt();
                    if l > 0.0 { x.map(|c| c / l) } else { x }
                }
                Op::Vec(n) => {
                    let mut o = [0.0; 4];
                    for k in 0..*n {
                        o[k] = a(k)[0];
                    }
                    o
                }
                Op::Swizzle(idx) => {
                    let x = a(0);
                    idx.map(|i| x[i])
                }
                Op::Rand => a(0).map(|c| noise::rand1(c.to_bits(), self.seed)),
                Op::Noise => {
                    let p = a(0);
                    [noise::perlin(p[0], p[1], 0, self.seed) * 0.5 + 0.5; 4]
                }
                Op::Cartesian => {
                    // Inputs are radius and angle (radians); the result is a Float2.
                    let (radius, angle) = (a(0)[0], a(1)[0]);
                    if radius.is_finite() && angle.is_finite() {
                        let (sin, cos) = angle.sin_cos();
                        [finite_or_zero(radius * cos), finite_or_zero(radius * sin), 0.0, 0.0]
                    } else {
                        [0.0; 4]
                    }
                }
                Op::IfElse => {
                    if a(0)[0] != 0.0 {
                        a(1)
                    } else {
                        a(2)
                    }
                }
            };
            regs.push(v);
        }
        regs.last().copied().unwrap_or([0.0; 4])
    }
}

/// The default pixel processor function: the first input, unchanged.
pub fn default_pixel_function() -> FnGraph {
    let mut g = FnGraph::default();
    let pos = g.add("get_pos", &[], None);
    let s = g.add("sample_color", &[pos], Some(Value::Int(0)));
    g.output = Some(s);
    g
}

pub(crate) fn pixel_processor(ctx: &Ctx) -> CookResult {
    let f = ctx.function("function").ok_or("no function")?;
    let prog = compile(f, ctx.vars, ctx.seed)?;
    let gray = ctx.i("output") == 1;
    let dims = prog.dims();
    let size = [ctx.w as f32, ctx.h as f32];
    let kind = if gray { Kind::Gray } else { Kind::Color };
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, kind, |u, v| {
        // shortcut: one register file per pixel; a thread-local would save the allocation.
        let mut regs = Vec::with_capacity(prog.code.len());
        let r = prog.eval([u, v], size, ctx.inputs, ctx.tile, &mut regs);
        match (gray, dims) {
            (true, _) => [r[0], 0.0, 0.0, 0.0],
            (false, 1) => [r[0], r[0], r[0], 1.0],
            (false, 2) => [r[0], r[1], 0.0, 1.0],
            (false, 3) => [r[0], r[1], r[2], 1.0],
            _ => r,
        }
    })])
}

pub(crate) fn value_processor(ctx: &Ctx) -> CookResult {
    let f = ctx.function("function").ok_or("no function")?;
    let prog = compile(f, ctx.vars, ctx.seed)?;
    let r = prog.eval([0.5, 0.5], [ctx.w as f32, ctx.h as f32], ctx.inputs, ctx.tile, &mut Vec::new());
    Ok(vec![if prog.dims() == 1 { Image::filled(ctx.w, ctx.h, Kind::Gray, [r[0]; 4]) } else { Image::filled(ctx.w, ctx.h, Kind::Color, r) }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate_math(op: &str, values: &[Value]) -> (usize, [f32; 4]) {
        let mut g = FnGraph::default();
        let inputs: Vec<_> = values
            .iter()
            .map(|value| {
                let op = match value {
                    Value::Float2(_) => "const_float2",
                    Value::Float3(_) => "const_float3",
                    Value::Float4(_) => "const_float4",
                    _ => "const_float",
                };
                g.add(op, &[], Some(value.clone()))
            })
            .collect();
        g.output = Some(g.add(op, &inputs, None));
        let p = compile(&g, &BTreeMap::new(), 0).unwrap();
        (p.dims(), p.eval([0.0; 2], [1.0; 2], &[], true, &mut Vec::new()))
    }

    #[test]
    fn base_two_functions_preserve_lanes_and_scalar_broadcast() {
        let (dims, result) = evaluate_math("pow2", &[Value::Float4([-2.0, 0.0, 1.0, 3.0])]);
        assert_eq!((dims, result), (4, [0.25, 1.0, 2.0, 8.0]));
        let (dims, result) = evaluate_math("log2", &[Value::Float4([0.25, 1.0, 2.0, 8.0])]);
        assert_eq!((dims, result), (4, [-2.0, 0.0, 1.0, 3.0]));
        assert_eq!(evaluate_math("pow2", &[Value::Float(3.0)]), (1, [8.0; 4]));
        assert_eq!(evaluate_math("log2", &[Value::Float(8.0)]), (1, [3.0; 4]));
        for op in ["pow2", "log2"] {
            assert_eq!(evaluate_math(op, &[Value::Float2([1.0, 2.0])]).0, 2);
            assert_eq!(evaluate_math(op, &[Value::Float3([1.0, 2.0, 3.0])]).0, 3);
        }
    }

    #[test]
    fn cartesian_uses_radius_then_angle_in_radians() {
        for (radius, angle, expected) in [
            (2.0, 0.0, [2.0, 0.0]),
            (2.0, std::f32::consts::FRAC_PI_2, [0.0, 2.0]),
            (2.0, -std::f32::consts::FRAC_PI_2, [0.0, -2.0]),
            (2.0, std::f32::consts::PI, [-2.0, 0.0]),
            (0.0, 1.0, [0.0, 0.0]),
        ] {
            let (dims, result) = evaluate_math("cartesian", &[Value::Float(radius), Value::Float(angle)]);
            assert_eq!(dims, 2);
            assert!((result[0] - expected[0]).abs() < 1e-6);
            assert!((result[1] - expected[1]).abs() < 1e-6);
            assert_eq!(&result[2..], &[0.0, 0.0]);
        }
    }

    #[test]
    fn new_math_functions_handle_invalid_domains_and_missing_inputs() {
        assert_eq!(evaluate_math("log2", &[Value::Float4([0.0, -1.0, f32::NAN, f32::INFINITY])]).1, [0.0; 4]);
        assert_eq!(evaluate_math("pow2", &[Value::Float4([128.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY])]).1, [0.0; 4]);
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(evaluate_math("cartesian", &[Value::Float(invalid), Value::Float(0.0)]).1, [0.0; 4]);
            assert_eq!(evaluate_math("cartesian", &[Value::Float(1.0), Value::Float(invalid)]).1, [0.0; 4]);
        }
        for op in ["pow2", "log2", "cartesian"] {
            let mut g = FnGraph::default();
            g.output = Some(g.add(op, &[], None));
            assert!(compile(&g, &BTreeMap::new(), 0).is_err());
        }
    }

    #[test]
    fn compiles_and_runs_arithmetic() {
        let mut g = FnGraph::default();
        let pos = g.add("get_pos", &[], None);
        let x = g.add("swizzle", &[pos], Some(Value::Text("x".into())));
        let two = g.add("const_float", &[], Some(Value::Float(2.0)));
        let m = g.add("mul", &[x, two], None);
        g.output = Some(m);
        let p = compile(&g, &BTreeMap::new(), 0).unwrap();
        assert_eq!(p.dims(), 1);
        let r = p.eval([0.25, 0.5], [8.0, 8.0], &[], true, &mut Vec::new());
        assert_eq!(r[0], 0.5);
    }

    #[test]
    fn rejects_bad_graphs() {
        let mut g = FnGraph::default();
        let a = g.add("add", &[], None);
        g.output = Some(a);
        assert!(compile(&g, &BTreeMap::new(), 0).is_err());
        g.nodes[0].inputs = vec![Some(a), Some(a)];
        assert!(compile(&g, &BTreeMap::new(), 0).is_err());
        assert!(compile(&FnGraph::default(), &BTreeMap::new(), 0).is_err());
    }
}
