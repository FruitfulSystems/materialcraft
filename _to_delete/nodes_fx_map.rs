//! Atomic › FX-Map: a pattern instancer (regular grid, random scatter or random quadtree
//! subdivision), plus the shared splatting engine of the scatter nodes. Instances wrap across the
//! edges, so the output tiles.

use std::f32::consts::{SQRT_2, TAU};

use materialcraft_image::{Image, Kind};

use crate::atomic::ATOMIC;
use crate::catalog::{CookResult, Ctx, NodeDef, node};
use crate::noise::{hash, rand1};
use crate::patterns::{PATTERNS, SHAPES, shape};

/// One placed pattern: center (uv), half extents (uv), rotation (turns), luminance and the index
/// of the pattern it shows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inst {
    pub c: [f32; 2],
    pub r: [f32; 2],
    pub rot: f32,
    pub lum: f32,
    pub pat: usize,
}

pub(crate) const BLENDS: &[&str] = &["Max", "Add", "Alpha over"];

/// Renders `items` over black. Item `i` covers the rotated box ±`r` around its center, where
/// `pat(i, x, y)` is its pattern (0..1) at local coordinates -1..1. Items are binned on a grid about
/// their size so a pixel only tests its neighbours; later items draw on top (Alpha over).
pub(crate) fn splat(ctx: &Ctx, items: &[Inst], blend: i32, pat: impl Fn(usize, f32, f32) -> f32 + Sync) -> Image {
    let valid = |it: &Inst| it.r[0] > 1e-6 && it.r[1] > 1e-6 && it.c.iter().chain(&it.r).all(|v| v.is_finite());
    let n = items.iter().filter(|it| valid(it)).count().max(1);
    let mean_r = items.iter().filter(|it| valid(it)).map(|it| it.r[0].max(it.r[1])).sum::<f32>() / n as f32;
    let g = ((0.5 / mean_r.max(1e-4)) as usize).clamp(1, 256);
    let gf = g as f32;
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); g * g];
    for (i, it) in items.iter().enumerate().filter(|(_, it)| valid(it)) {
        let br = it.r[0].max(it.r[1]) * SQRT_2;
        // At most one full turn of bins per axis, so no item lands twice in a bin.
        let span = |c: f32| {
            let a = ((c - br) * gf).floor() as i64;
            (a, (((c + br) * gf).floor() as i64).min(a + g as i64 - 1))
        };
        let ((x0, x1), (y0, y1)) = (span(it.c[0]), span(it.c[1]));
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if let Some(b) = bins.get_mut(gy.rem_euclid(g as i64) as usize * g + gx.rem_euclid(g as i64) as usize) {
                    b.push(i as u32);
                }
            }
        }
    }
    let rots: Vec<(f32, f32)> = items.iter().map(|it| (-it.rot * TAU).sin_cos()).collect();
    let tile = ctx.tile;
    Image::generate_uv(ctx.w, ctx.h, Kind::Gray, |u, v| {
        let mut acc = 0.0f32;
        let bin = bins.get(((v * gf) as usize).min(g - 1) * g + ((u * gf) as usize).min(g - 1));
        for &i in bin.into_iter().flatten() {
            let (Some(it), Some(&(s, c))) = (items.get(i as usize), rots.get(i as usize)) else { continue };
            let (mut dx, mut dy) = (u - it.c[0], v - it.c[1]);
            if tile {
                dx -= dx.round();
                dy -= dy.round();
            }
            let (x, y) = ((dx * c - dy * s) / it.r[0], (dx * s + dy * c) / it.r[1]);
            if x.abs() > 1.0 || y.abs() > 1.0 {
                continue;
            }
            let p = pat(i as usize, x, y);
            if p <= 0.0 {
                continue;
            }
            acc = match blend {
                1 => acc + p * it.lum,
                2 => acc + (it.lum - acc) * p.min(1.0),
                _ => acc.max(p * it.lum),
            };
        }
        [acc, 0.0, 0.0, 0.0]
    })
}

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("fx_map", "FX-Map", ATOMIC)
            .desc(
                "Instances up to three patterns on a grid, at random positions or in a random quadtree, with per-instance scale, rotation, \
                 position and luminance jitter. Scale 1 fills an instance's cell. Unconnected: soft discs. Tiles.",
            )
            .atomic()
            .generator()
            .gray_in("pattern0", "Pattern 0")
            .gray_in("pattern1", "Pattern 1")
            .gray_in("pattern2", "Pattern 2")
            .choice("mode", "Mode", &["Grid", "Random", "Quadtree"], 2)
            .int("count", "Count (random)", 64, 1, 20000)
            .int("x_amount", "X amount", 4, 1, 256)
            .int("y_amount", "Y amount", 4, 1, 256)
            .int("subdivisions", "Subdivisions (quadtree)", 3, 0, 8)
            .float("split_chance", "Subdivide chance", 0.5, 0.0, 1.0)
            .float("scale", "Scale", 0.9, 0.0, 2.0)
            .float("scale_random", "Scale random", 0.2, 0.0, 1.0)
            .angle("rotation", "Rotation", 0.0)
            .float("rotation_random", "Rotation random", 0.0, 0.0, 1.0)
            .float("position_random", "Position random", 0.0, 0.0, 1.0)
            .float("luminance", "Luminance", 1.0, 0.0, 1.0)
            .float("luminance_random", "Luminance random", 0.4, 0.0, 1.0)
            .float("pattern_random", "Pattern random", 1.0, 0.0, 1.0)
            .choice("blending", "Blending", BLENDS, 0)
            .int("seed_offset", "Seed offset", 0, 0, 10000)
            .gray_out()
            .cook(fx_map),
        node("shape_splatter", "Shape Splatter", PATTERNS)
            .desc("Scatters the pattern input (or a shape) with random scale, rotation and luminance; the mask input sets where instances land.")
            .generator()
            .gray_in("pattern", "Pattern")
            .gray_in("mask", "Mask")
            .choice("shape", "Shape (no input)", SHAPES, 3)
            .int("count", "Count", 300, 1, 20000)
            .float("size", "Size", 0.05, 0.0, 1.0)
            .float("size_random", "Size random", 0.5, 0.0, 1.0)
            .float("rotation_random", "Rotation random", 1.0, 0.0, 1.0)
            .float("luminance_random", "Luminance random", 0.4, 0.0, 1.0)
            .choice("blending", "Blending", BLENDS, 0)
            .gray_out()
            .cook(shape_splatter),
    ]
}

/// A connected pattern at local `(x, y)` (-1..1), or a soft disc when there is none.
pub(crate) fn sample_pattern(pats: &[Image], k: usize, x: f32, y: f32) -> f32 {
    match pats.get(k) {
        Some(img) => img.sample_gray((x + 1.0) * 0.5, (y + 1.0) * 0.5, false),
        None => shape(3, x, y),
    }
}

/// Cells of a random quadtree over an `nx × ny` root grid: (center, size, key). A cell splits
/// with probability `chance` until `depth`.
fn quadtree(nx: i32, ny: i32, depth: i32, chance: f32, seed: u32) -> Vec<([f32; 2], [f32; 2], u32)> {
    const MAX_CELLS: usize = 1 << 18;
    let mut out = Vec::new();
    let mut stack: Vec<([f32; 2], [f32; 2], i32, u32)> = Vec::new();
    for j in (0..ny).rev() {
        for i in (0..nx).rev() {
            let size = [1.0 / nx as f32, 1.0 / ny as f32];
            stack.push(([i as f32 * size[0], j as f32 * size[1]], size, 0, hash((j * nx + i) as u32 ^ 0x5157_0000)));
        }
    }
    while let Some((p, s, lvl, key)) = stack.pop() {
        if lvl < depth && rand1(key, seed ^ 0x51ed) < chance && out.len() + stack.len() + 4 <= MAX_CELLS {
            let h = [s[0] * 0.5, s[1] * 0.5];
            for q in (0..4u32).rev() {
                let o = [p[0] + (q & 1) as f32 * h[0], p[1] + (q >> 1) as f32 * h[1]];
                stack.push((o, h, lvl + 1, hash(key.wrapping_mul(4).wrapping_add(q + 1))));
            }
        } else {
            out.push(([p[0] + s[0] * 0.5, p[1] + s[1] * 0.5], s, key));
        }
    }
    out
}

/// The instances an FX-Map places (separate from rendering, so it can be tested).
pub(crate) fn fx_instances(ctx: &Ctx, n_patterns: usize) -> Vec<Inst> {
    let seed = ctx.seed.wrapping_add(ctx.i("seed_offset") as u32);
    let (nx, ny) = (ctx.i("x_amount").clamp(1, 256), ctx.i("y_amount").clamp(1, 256));
    let cells: Vec<([f32; 2], [f32; 2], u32)> = match ctx.i("mode") {
        0 => (0..ny)
            .flat_map(|j| {
                (0..nx).map(move |i| ([(i as f32 + 0.5) / nx as f32, (j as f32 + 0.5) / ny as f32], [1.0 / nx as f32, 1.0 / ny as f32], (j * nx + i) as u32))
            })
            .collect(),
        1 => {
            let count = ctx.i("count").clamp(1, 20000) as u32;
            let side = 1.0 / (count as f32).sqrt();
            (0..count).map(|i| ([rand1(i * 2, seed ^ 0xa11), rand1(i * 2 + 1, seed ^ 0xa11)], [side, side], i)).collect()
        }
        _ => quadtree(nx, ny, ctx.i("subdivisions").clamp(0, 8), ctx.f("split_chance"), seed),
    };
    let (sc, sr, rot, rr, pr) = (ctx.f("scale"), ctx.f("scale_random"), ctx.f("rotation"), ctx.f("rotation_random"), ctx.f("position_random"));
    let (lum, lr, patr) = (ctx.f("luminance"), ctx.f("luminance_random"), ctx.f("pattern_random"));
    let n = n_patterns.max(1);
    cells
        .into_iter()
        .map(|(c, size, key)| {
            let r = |k: u32| rand1(key.wrapping_mul(8).wrapping_add(k), seed);
            let s = sc * (1.0 - sr * r(2));
            Inst {
                c: [c[0] + (r(0) - 0.5) * pr * size[0], c[1] + (r(1) - 0.5) * pr * size[1]],
                r: [size[0] * 0.5 * s, size[1] * 0.5 * s],
                rot: rot + (r(3) - 0.5) * rr,
                lum: lum * (1.0 - lr * r(4)),
                pat: if r(5) < patr { ((r(6) * n as f32) as usize).min(n - 1) } else { 0 },
            }
        })
        .collect()
}

fn fx_map(ctx: &Ctx) -> CookResult {
    let pats: Vec<Image> = (0..3).filter_map(|i| ctx.input(i).map(Image::to_gray)).collect();
    let items = fx_instances(ctx, pats.len());
    Ok(vec![splat(ctx, &items, ctx.i("blending"), |i, x, y| sample_pattern(&pats, items.get(i).map_or(0, |it| it.pat), x, y))])
}

fn shape_splatter(ctx: &Ctx) -> CookResult {
    let pats: Vec<Image> = ctx.input(0).map(Image::to_gray).into_iter().collect();
    let mask = ctx.input(1);
    let (k, s) = (ctx.i("shape"), ctx.seed);
    let (size, sr, rr, lr) = (ctx.f("size"), ctx.f("size_random"), ctx.f("rotation_random"), ctx.f("luminance_random"));
    let items: Vec<Inst> = (0..ctx.i("count").clamp(1, 20000) as u32)
        .filter_map(|i| {
            let r = |k: u32| rand1(i * 8 + k, s);
            let c = [r(0), r(1)];
            // Instances land where the mask is bright: kept with probability = mask value.
            if mask.is_some_and(|m| r(2) >= m.sample_gray(c[0], c[1], ctx.tile)) {
                return None;
            }
            let rad = size * (1.0 - sr * r(3));
            Some(Inst { c, r: [rad, rad], rot: (r(4) - 0.5) * rr, lum: 1.0 - lr * r(5), pat: 0 })
        })
        .collect();
    Ok(vec![splat(ctx, &items, ctx.i("blending"), |_, x, y| if pats.is_empty() { shape(k, x, y) } else { sample_pattern(&pats, 0, x, y) })])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use materialcraft_core::Value;

    use super::*;
    use crate::catalog::{find, resolve_params};

    fn with(params: &[(&str, Value)], f: impl FnOnce(&Ctx)) {
        let def = find("fx_map").unwrap();
        let mut p = resolve_params(def, &BTreeMap::new());
        for (k, v) in params {
            p.insert(k.to_string(), v.clone());
        }
        let no_res = |_: &str| None;
        let vars = BTreeMap::new();
        let inputs = [None, None, None];
        f(&Ctx { w: 64, h: 64, seed: 3, tile: true, inputs: &inputs, params: &p, resource: &no_res, vars: &vars });
    }

    #[test]
    fn fx_map_instance_counts() {
        with(&[("mode", Value::Enum(0)), ("x_amount", Value::Int(5)), ("y_amount", Value::Int(3))], |c| assert_eq!(fx_instances(c, 1).len(), 15));
        with(&[("mode", Value::Enum(1)), ("count", Value::Int(77))], |c| assert_eq!(fx_instances(c, 1).len(), 77));
        let quad = |chance: f32, depth: i32| {
            let mut n = 0;
            with(
                &[
                    ("mode", Value::Enum(2)),
                    ("x_amount", Value::Int(2)),
                    ("y_amount", Value::Int(1)),
                    ("subdivisions", Value::Int(depth)),
                    ("split_chance", Value::Float(chance)),
                ],
                |c| n = fx_instances(c, 1).len(),
            );
            n
        };
        assert_eq!(quad(0.0, 3), 2);
        assert_eq!(quad(1.0, 3), 2 * 64);
        let some = quad(0.5, 4);
        assert!(some > 2 && some < 2 * 256, "{some}");
    }

    #[test]
    fn fx_map_is_deterministic_per_seed() {
        let cook = |off: i32| {
            let mut img = None;
            with(&[("seed_offset", Value::Int(off))], |c| img = Some(fx_map(c).unwrap().remove(0)));
            img.unwrap()
        };
        assert_eq!(cook(0), cook(0));
        assert_ne!(cook(0), cook(1));
        assert!(cook(0).mean()[0] > 0.05);
    }
}
