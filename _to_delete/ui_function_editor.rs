//! The function graph editor: a window with a small node canvas for a pixel or value
//! processor's function. Right-click (or Space) adds operations by category, dragging from an
//! output to an input wires them, the starred node is the result. Each change commits the whole
//! function through `graph.node.param`.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::NodeId;
use materialcraft_engine::core::Value;
use materialcraft_engine::core::function::{FnGraph, FnNode};
use materialcraft_engine::nodes::function::FN_OPS;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const W: f32 = 130.0;
const HEADER: f32 = 18.0;
const ROW: f32 = 18.0;

/// The open editor: which node's parameter, and the canvas camera.
pub struct FnEditor {
    pub node: NodeId,
    pub param: String,
    pan: Vec2,
    zoom: f32,
    drag: Option<FnDrag>,
    menu_at: Pos2,
    /// The camera has been fitted to the nodes.
    framed: bool,
}

enum FnDrag {
    Node(u32, Vec2),
    Wire(u32),
    Pan,
}

impl FnEditor {
    pub fn new(node: NodeId, param: &str) -> FnEditor {
        FnEditor { node, param: param.into(), pan: vec2(-40.0, -40.0), zoom: 1.0, drag: None, menu_at: Pos2::ZERO, framed: false }
    }
}

fn arity(op: &str) -> usize {
    FN_OPS.iter().find(|o| o.0 == op).map_or(0, |o| o.2)
}

fn label(op: &str) -> &'static str {
    FN_OPS.iter().find(|o| o.0 == op).map_or("?", |o| o.1)
}

fn category_color(op: &str) -> Color32 {
    match FN_OPS.iter().find(|o| o.0 == op).map_or("", |o| o.3) {
        "Constant" => Color32::from_rgb(0x5c, 0x62, 0x6e),
        "Variables" => Color32::from_rgb(0x6d, 0x52, 0xb5),
        "Samplers" => Color32::from_rgb(0xa8, 0x3c, 0x34),
        "Vector" | "Cast" => Color32::from_rgb(0x3a, 0x66, 0xb0),
        "Operator" => Color32::from_rgb(0x3b, 0x7f, 0x4c),
        "Logical" | "Comparison" | "Control" => Color32::from_rgb(0xb3, 0x5a, 0x2c),
        _ => Color32::from_rgb(0x1e, 0x8a, 0xa0),
    }
}

/// The value a new node of `op` starts with.
fn default_value(op: &str) -> Option<Value> {
    Some(match op {
        "const_float" => Value::Float(0.5),
        "const_float2" => Value::Float2([0.5, 0.5]),
        "const_float3" => Value::Float3([0.5, 0.5, 0.5]),
        "const_float4" => Value::Float4([0.5, 0.5, 0.5, 1.0]),
        "const_int" => Value::Int(1),
        "const_bool" => Value::Bool(true),
        "get_var" => Value::Text(String::new()),
        "sample_gray" | "sample_color" => Value::Int(0),
        "swizzle" => Value::Text("x".into()),
        _ => return None,
    })
}

fn node_rect(n: &FnNode) -> Rect {
    let rows = arity(&n.op).max(1) as f32 + if n.value.is_some() { 1.0 } else { 0.0 };
    Rect::from_min_size(pos2(n.pos[0], n.pos[1]), vec2(W, HEADER + rows * ROW + 6.0))
}

fn in_port(r: Rect, i: usize) -> Pos2 {
    pos2(r.left(), r.top() + HEADER + ROW * (i as f32 + 0.5))
}

fn out_port(r: Rect) -> Pos2 {
    pos2(r.right(), r.top() + HEADER + ROW * 0.5)
}

/// The function currently stored on the node: a function parameter, or (`fn:<param>`) the
/// function driving a dynamic parameter.
fn current(app: &MaterialCraftApp, e: &FnEditor) -> Option<FnGraph> {
    let n = app.session.graph().ok()?.node(e.node).ok()?;
    if let Some(p) = e.param.strip_prefix("fn:") {
        return n.functions.get(p).cloned();
    }
    match n.params.get(&e.param) {
        Some(Value::Function(f)) => Some((**f).clone()),
        _ => materialcraft_engine::nodes::find(&n.def)?.param(&e.param).and_then(|p| match &p.default {
            Value::Function(f) => Some((**f).clone()),
            _ => None,
        }),
    }
}

fn commit(app: &mut MaterialCraftApp, node: NodeId, param: &str, g: &FnGraph) {
    if let Some(p) = param.strip_prefix("fn:") {
        app.run("graph.node.function", json!({"id": node.0, "param": p, "function": g}));
        return;
    }
    let v = serde_json::to_value(Value::Function(Box::new(g.clone()))).unwrap_or(serde_json::Value::Null);
    app.run("graph.node.param", json!({"id": node.0, "param": param, "value": v}));
}

/// Add a function node of `op` to the open function editor (the library's Function nodes).
/// Returns false when no function graph is open.
pub fn add_op(app: &mut MaterialCraftApp, op: &str) -> bool {
    let Some(e) = app.fn_editor.as_ref() else { return false };
    let (node, param) = (e.node, e.param.clone());
    let Some(mut g) = current(app, e) else { return false };
    if !FN_OPS.iter().any(|o| o.0 == op) || g.nodes.len() >= 10_000 {
        return false;
    }
    let id = g.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
    // Below the lowest node, so new nodes never stack on one another.
    let bottom = g.nodes.iter().map(|n| n.pos[1]).fold(-ROW * 4.0, f32::max) + ROW * 4.0;
    let left = g.nodes.iter().map(|n| n.pos[0]).fold(0.0, f32::min);
    g.nodes.push(FnNode { id, op: op.into(), inputs: vec![None; arity(op)], value: default_value(op), pos: [left, bottom] });
    if g.output.is_none() {
        g.output = Some(id);
    }
    commit(app, node, &param, &g);
    true
}

/// Draw the editor window when one is open.
pub fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some(mut e) = app.fn_editor.take() else { return };
    let Some(mut g) = current(app, &e) else { return };
    let title = app.session.graph().ok().and_then(|gr| gr.node(e.node).ok()).map(crate::graph_view::node_label).unwrap_or_default();
    let mut open = true;
    let mut changed = false;
    let what = e.param.strip_prefix("fn:").map_or_else(|| tr("Function graph").to_string(), |p| format!("{} {p}", tr("Function of")));
    egui::Window::new(format!("{title} - {what}"))
        .id(egui::Id::unique(("fn-editor", e.node.0, e.param.as_str())))
        .open(&mut open)
        .default_size(vec2(760.0, 460.0))
        .resizable(true)
        .show(ctx, |ui| {
            let tk = Tokens::get(ui.ctx());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr("Right-click or Space to add operations; drag from an output dot to an input; the starred node is the result."))
                        .size(11.0)
                        .color(tk.text_dim),
                );
            });
            changed = canvas(&mut e, &mut g, ui);
        });
    if changed {
        commit(app, e.node, &e.param, &g);
    }
    if open {
        app.fn_editor = Some(e);
    }
}

fn canvas(e: &mut FnEditor, g: &mut FnGraph, ui: &mut Ui) -> bool {
    let tk = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(ui.available_size().max(vec2(200.0, 200.0)), Sense::click_and_drag());
    crate::widgets::name(&resp, egui::Role::Canvas, "Function graph");
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::from_rgb(0x26, 0x28, 0x2c));
    if !e.framed {
        e.framed = true;
        let b = g.nodes.iter().map(node_rect).fold(Rect::NOTHING, |a, r| a.union(r));
        if b.is_finite() && b.width() > 0.0 {
            let b = b.expand(30.0);
            e.zoom = (rect.width() / b.width()).min(rect.height() / b.height()).clamp(0.3, 1.2);
            e.pan = b.center().to_vec2() - rect.size() * 0.5 / e.zoom;
        }
    }
    let to_s = |q: Pos2, e: &FnEditor| rect.min + (q.to_vec2() - e.pan) * e.zoom;
    let to_g = |s: Pos2, e: &FnEditor| ((s - rect.min) / e.zoom + e.pan).to_pos2();
    let mut changed = false;
    let pointer = ui.input(|i| i.pointer.hover_pos());

    // Zoom and pan.
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0
            && let Some(ptr) = pointer
        {
            let before = to_g(ptr, e);
            e.zoom = (e.zoom * (scroll * 0.0025).exp()).clamp(0.3, 2.5);
            e.pan = before.to_vec2() - (ptr - rect.min) / e.zoom;
        }
    }

    // Wires.
    for n in &g.nodes {
        let r = node_rect(n);
        for (i, src) in n.inputs.iter().enumerate() {
            if let Some(s) = src.and_then(|s| g.node(s)) {
                let a = to_s(out_port(node_rect(s)), e);
                let b = to_s(in_port(r, i), e);
                let dx = ((b.x - a.x).abs() * 0.5).max(24.0);
                p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b],
                    false,
                    Color32::TRANSPARENT,
                    Stroke::new(2.0, Color32::from_gray(0xb4)),
                ));
            }
        }
    }

    // Nodes.
    let font = FontId::proportional((11.0 * e.zoom).clamp(7.0, 16.0));
    let mut remove: Option<u32> = None;
    let mut set_output: Option<u32> = None;
    let mut ports: Vec<(u32, Option<usize>, Pos2)> = Vec::new();
    let ids: Vec<u32> = g.nodes.iter().map(|n| n.id).collect();
    for id in ids {
        let Some(idx) = g.nodes.iter().position(|n| n.id == id) else { continue };
        let mut r = node_rect(&g.nodes[idx]);
        if let Some(FnDrag::Node(d, delta)) = &e.drag
            && *d == id
        {
            r = r.translate(*delta);
        }
        let sr = Rect::from_min_max(to_s(r.min, e), to_s(r.max, e));
        let n = &g.nodes[idx];
        let is_out = g.output == Some(id);
        p.rect_filled(sr, 5.0, Color32::from_rgb(0x1e, 0x20, 0x24));
        let header = Rect::from_min_size(sr.min, vec2(sr.width(), HEADER * e.zoom));
        p.rect_filled(header, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, category_color(&n.op));
        let star = if is_out { format!("{} ", icon::STAR) } else { String::new() };
        p.text(header.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, format!("{star}{}", label(&n.op)), font.clone(), Color32::WHITE);
        p.rect_stroke(sr, 5.0, Stroke::new(if is_out { 2.0 } else { 1.0 }, if is_out { tk.accent } else { Color32::from_gray(0x10) }), StrokeKind::Outside);
        for i in 0..arity(&n.op) {
            let pos = to_s(in_port(r, i), e);
            p.circle(pos, 4.5, Color32::from_gray(0xc8), Stroke::new(1.0, Color32::from_gray(0x18)));
            ports.push((id, Some(i), pos));
        }
        let op = to_s(out_port(r), e);
        p.circle(op, 4.5, Color32::from_rgb(0xe8, 0xa9, 0x3c), Stroke::new(1.0, Color32::from_gray(0x18)));
        ports.push((id, None, op));
        // Inline value editor.
        if n.value.is_some() {
            let row = Rect::from_min_size(
                to_s(pos2(r.left() + 6.0, r.top() + HEADER + arity(&n.op).max(1) as f32 * ROW + 2.0), e),
                vec2((W - 12.0) * e.zoom, (ROW - 2.0) * e.zoom),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row));
            if let Some(v) = g.nodes[idx].value.as_mut() {
                changed |= value_editor(&mut child, v);
            }
        }
        let body = ui.interact(sr, ui.scope_id().with(("fn-node", id)), Sense::click_and_drag());
        crate::widgets::name(&body, egui::Role::Button, label(&g.nodes[idx].op));
        if body.drag_started() && e.drag.is_none() {
            e.drag = Some(FnDrag::Node(id, Vec2::ZERO));
        }
        body.context_menu(|ui| {
            if ui.button(format!("{}  {}", icon::STAR, tr("Use as result"))).clicked() {
                set_output = Some(id);
                ui.close();
            }
            if ui.button(format!("{}  {}", icon::X, tr("Delete"))).clicked() {
                remove = Some(id);
                ui.close();
            }
        });
    }
    if let Some(id) = set_output {
        g.output = Some(id);
        changed = true;
    }
    if let Some(id) = remove {
        g.nodes.retain(|n| n.id != id);
        for n in &mut g.nodes {
            for i in &mut n.inputs {
                if *i == Some(id) {
                    *i = None;
                }
            }
        }
        if g.output == Some(id) {
            g.output = None;
        }
        changed = true;
    }

    // Gestures: wires from output dots, panning.
    let near = |ptr: Pos2| ports.iter().filter(|(_, _, q)| q.distance(ptr) < 9.0).min_by(|a, b| a.2.distance(ptr).total_cmp(&b.2.distance(ptr))).cloned();
    if resp.drag_started_by(egui::PointerButton::Middle) || (resp.drag_started() && ui.input(|i| i.modifiers.alt)) {
        e.drag = Some(FnDrag::Pan);
    } else if ui.input(|i| i.pointer.primary_pressed())
        && e.drag.is_none()
        && let Some(ptr) = pointer
        && rect.contains(ptr)
        && let Some((id, None, _)) = near(ptr)
    {
        e.drag = Some(FnDrag::Wire(id));
    }
    let delta = ui.input(|i| i.pointer.delta());
    match &mut e.drag {
        Some(FnDrag::Node(_, d)) => *d += delta / e.zoom,
        Some(FnDrag::Pan) => e.pan -= delta / e.zoom,
        Some(FnDrag::Wire(id)) => {
            if let (Some(ptr), Some(src)) = (pointer, ports.iter().find(|(n, s, _)| n == id && s.is_none())) {
                p.line_segment([src.2, ptr], Stroke::new(2.0, tk.accent));
            }
        }
        None => {}
    }
    if ui.input(|i| i.pointer.any_released()) {
        match e.drag.take() {
            Some(FnDrag::Node(id, d)) => {
                if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id)
                    && d.length() > 0.5
                {
                    n.pos = [n.pos[0] + d.x, n.pos[1] + d.y];
                    changed = true;
                }
            }
            Some(FnDrag::Wire(src)) => {
                if let Some(ptr) = pointer
                    && let Some((dst, Some(slot), _)) = near(ptr)
                    && dst != src
                    && !reaches(g, dst, src)
                    && let Some(n) = g.nodes.iter_mut().find(|n| n.id == dst)
                {
                    if n.inputs.len() <= slot {
                        n.inputs.resize(slot + 1, None);
                    }
                    n.inputs[slot] = Some(src);
                    changed = true;
                }
            }
            _ => {}
        }
    }

    // Adding operations.
    if resp.secondary_clicked()
        && let Some(ptr) = pointer
    {
        e.menu_at = to_g(ptr, e);
    }
    if resp.hovered()
        && ui.input(|i| i.key_pressed(egui::Key::Space))
        && let Some(ptr) = pointer
    {
        e.menu_at = to_g(ptr, e);
        egui::Popup::open_id(ui.ctx(), resp.id.with("menu"));
    }
    let at = e.menu_at;
    let mut add: Option<&'static str> = None;
    resp.context_menu(|ui| {
        let mut cats: Vec<&str> = FN_OPS.iter().map(|o| o.3).collect();
        cats.dedup();
        for c in cats {
            ui.menu_button(tr(c), |ui| {
                for o in FN_OPS.iter().filter(|o| o.3 == c) {
                    if ui.button(tr(o.1)).clicked() {
                        add = Some(o.0);
                        ui.close();
                    }
                }
            });
        }
    });
    if let Some(op) = add {
        let id = g.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1);
        g.nodes.push(FnNode { id, op: op.into(), inputs: vec![None; arity(op)], value: default_value(op), pos: [at.x, at.y] });
        if g.output.is_none() {
            g.output = Some(id);
        }
        changed = true;
    }
    changed
}

/// Whether `a` feeds `b` (directly or through other nodes, or is `b`): then `b` may not feed `a`.
fn reaches(g: &FnGraph, a: u32, b: u32) -> bool {
    let mut stack = vec![b];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == a {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(node) = g.node(n) {
            stack.extend(node.inputs.iter().flatten());
        }
    }
    false
}

/// A compact editor for a function node's value. Returns true when it changed.
fn value_editor(ui: &mut Ui, v: &mut Value) -> bool {
    let drag = |ui: &mut Ui, f: &mut f32| ui.add(egui::DragValue::new(f).speed(0.01).max_decimals(3)).changed();
    ui.horizontal(|ui| match v {
        Value::Float(f) => drag(ui, f),
        Value::Float2(a) => {
            let x = drag(ui, &mut a[0]);
            drag(ui, &mut a[1]) || x
        }
        Value::Float3(a) => {
            let mut c = false;
            for f in a.iter_mut() {
                c |= drag(ui, f);
            }
            c
        }
        Value::Float4(a) | Value::Color(a) => {
            let mut c = false;
            for f in a.iter_mut() {
                c |= ui.add(egui::DragValue::new(f).speed(0.01).max_decimals(2)).changed();
            }
            c
        }
        Value::Int(i) => ui.add(egui::DragValue::new(i).speed(0.1)).changed(),
        Value::Bool(b) => ui.checkbox(b, "").changed(),
        Value::Text(t) => {
            let r = ui.add(egui::TextEdit::singleline(t).desired_width(ui.available_width()));
            crate::widgets::name(&r, egui::Role::TextInput, "Value");
            r.changed()
        }
        _ => false,
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_are_detected() {
        let mut g = FnGraph::default();
        let a = g.add("get_pos", &[], None);
        let b = g.add("swizzle", &[a], Some(Value::Text("x".into())));
        // a feeds b: wiring b into a would loop.
        assert!(reaches(&g, a, b));
        assert!(!reaches(&g, b, a));
        assert_eq!(arity("add"), 2);
        assert!(default_value("const_float").is_some());
    }
}
