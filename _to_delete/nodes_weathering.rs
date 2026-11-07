//! Wear and weathering: cracks, paint strokes, scratches along a direction field, rust, moss and
//! lichen, leaks (drips running down from a mask) and chipped edges. The generators tile.

use std::f32::consts::{PI, TAU};

use materialcraft_image::{Image, Kind};
use rayon::prelude::*;

use crate::catalog::{CookResult, Ctx, NodeDef, PortKind, node};
use crate::fx_map::{BLENDS, Inst, splat};
use crate::generators::NOISES;
use crate::noise::{cellular, fbm, rand1, value, value_xy};
use crate::noises_more::smoothstep;
use crate::ops;
use crate::patterns::PATTERNS;
use crate::svg::{Capsules, Seg};

const WEATHER: &str = "Material filters/Weathering";

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("cracks", "Cracks", NOISES)
            .desc("A connected network of thin cracks along warped cell borders, with finer branching cracks. White = crack.")
            .generator()
            .float("scale", "Scale", 5.0, 1.0, 32.0)
            .float("width", "Width", 1.5, 0.0, 16.0)
            .float("warp", "Warp", 0.5, 0.0, 1.0)
            .float("branches", "Branches", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(cracks),
        node("paint_strokes", "Paint Strokes", PATTERNS)
            .desc("Overlapping brush strokes with bristle streaks and ragged ends.")
            .generator()
            .int("count", "Count", 150, 1, 5000)
            .float("length", "Length", 0.35, 0.0, 1.0)
            .float("width", "Width", 0.07, 0.0, 0.5)
            .angle("angle", "Angle", 0.0)
            .float("angle_random", "Angle random", 0.08, 0.0, 1.0)
            .int("bristles", "Bristles", 20, 1, 256)
            .float("luminance_random", "Luminance random", 0.5, 0.0, 1.0)
            .choice("blending", "Blending", BLENDS, 2)
            .gray_out()
            .cook(paint_strokes),
        node("scratches_generator", "Scratches Generator", PATTERNS)
            .desc("Thin tapered scratches that bend along a direction field (gray = angle in turns) and land where the mask is bright.")
            .generator()
            .gray_in("direction", "Direction field")
            .gray_in("mask", "Mask")
            .int("count", "Count", 200, 0, 5000)
            .float("length", "Length", 0.25, 0.0, 1.0)
            .float("width", "Width", 1.0, 0.1, 16.0)
            .angle("angle", "Angle", 0.0)
            .float("angle_random", "Angle random", 0.15, 0.0, 1.0)
            .float("curvature", "Curvature", 0.1, 0.0, 1.0)
            .float("field_intensity", "Field intensity", 0.25, 0.0, 1.0)
            .float("luminance_random", "Luminance random", 0.6, 0.0, 1.0)
            .gray_out()
            .cook(scratches_generator),
        node("rust", "Rust", WEATHER)
            .desc("A mask of rust blotches and pitting; with a mask input it grows out from the mask (edges, cavities).")
            .generator()
            .gray_in("mask", "Mask (optional)")
            .float("amount", "Amount", 0.45, 0.0, 1.0)
            .float("scale", "Scale", 6.0, 1.0, 64.0)
            .float("spread", "Spread", 12.0, 0.0, 128.0)
            .float("pitting", "Pitting", 0.5, 0.0, 1.0)
            .float("contrast", "Contrast", 0.7, 0.0, 1.0)
            .gray_out()
            .cook(rust),
        node("moss", "Moss / Lichen", WEATHER)
            .desc("Fuzzy moss clumps (gathering in low areas) or round lichen rosettes (on high areas) of an optional height map.")
            .generator()
            .gray_in("height", "Height (optional)")
            .choice("style", "Style", &["Moss", "Lichen"], 0)
            .float("amount", "Amount", 0.5, 0.0, 1.0)
            .float("scale", "Scale", 6.0, 1.0, 64.0)
            .float("height_bias", "Height bias", 0.5, 0.0, 1.0)
            .float("contrast", "Contrast", 0.6, 0.0, 1.0)
            .gray_out()
            .cook(moss),
        node("leaks", "Leaks", WEATHER)
            .desc("Streaky drips running down from a mask (unconnected: random stains), fading with length and meandering.")
            .gray_in("mask", "Mask")
            .float("length", "Length", 0.35, 0.0, 1.0)
            .float("length_random", "Length random", 0.7, 0.0, 1.0)
            .float("streaks", "Streaks", 32.0, 1.0, 256.0)
            .float("wobble", "Wobble", 0.3, 0.0, 1.0)
            .gray_out()
            .cook(leaks),
        node("edge_damage", "Edge Damage", WEATHER)
            .desc("Chips the edges of a height mask (white = material) in chunky, ragged bites; also outputs the damage mask.")
            .gray_in("height", "Height")
            .float("width", "Width", 8.0, 0.0, 64.0)
            .float("amount", "Amount", 0.5, 0.0, 1.0)
            .float("scale", "Chip scale", 12.0, 1.0, 64.0)
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .float("depth", "Depth", 1.0, 0.0, 1.0)
            .gray_out()
            .output("mask", "Damage mask", PortKind::Gray)
            .cook(edge_damage),
    ]
}

/// `v` pixels at 256 px, at the output size.
fn px(ctx: &Ctx, v: f32) -> f32 {
    v * ctx.w as f32 / 256.0
}

fn whole(ctx: &Ctx, id: &str) -> f32 {
    ctx.f(id).max(1.0).round()
}

/// A soft threshold: 1 where `v` is above `1 - amount`; `contrast` 1 is hard, 0 soft.
fn thresh(v: f32, amount: f32, contrast: f32) -> f32 {
    let soft = (1.0 - contrast.clamp(0.0, 1.0)) * 0.25 + 0.005;
    smoothstep(1.0 - amount - soft, 1.0 - amount + soft, v)
}

/// A gray image from `f(pixel index, u, v)`.
fn gen_px(ctx: &Ctx, f: impl Fn(usize, f32, f32) -> f32 + Sync) -> Image {
    let (w, h) = (ctx.w.max(1), ctx.h.max(1));
    Image::generate(w, h, Kind::Gray, |x, y| [f(y as usize * w as usize + x as usize, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32), 0.0, 0.0, 0.0])
}

fn cracks(ctx: &Ctx) -> CookResult {
    let (s, wpx, warp, br, seed) = (whole(ctx, "scale"), px(ctx, ctx.f("width")), ctx.f("warp"), ctx.f("branches"), ctx.seed);
    let cell_px = ctx.w as f32 / s;
    Ok(vec![ctx.gen_gray(|u, v| {
        // Broad bends plus fine jaggedness, both in cell units.
        let k = warp / s;
        let wu = u + (fbm(u, v, 4.0, 4, 0.5, seed ^ 11) - 0.5) * 2.0 * k + (fbm(u, v, s * 4.0, 3, 0.5, seed ^ 14) - 0.5) * 0.3 * k;
        let wv = v + (fbm(u, v, 4.0, 4, 0.5, seed ^ 12) - 0.5) * 2.0 * k + (fbm(u, v, s * 4.0, 3, 0.5, seed ^ 15) - 0.5) * 0.3 * k;
        let line = |m: f32, sd: u32, half: f32| {
            let c = cellular(wu * s * m, wv * s * m, (s * m) as i32, 1.0, 0, sd);
            1.0 - smoothstep(half, half + 1.0, (c.f2 - c.f1) * 0.5 * cell_px / m)
        };
        let main = line(1.0, seed, wpx * 0.5);
        if br <= 0.0 {
            return main;
        }
        let keep = smoothstep(1.0 - br, 1.0 - br + 0.2, fbm(u, v, s, 3, 0.5, seed ^ 13));
        main.max(line(3.0, seed ^ 7, wpx * 0.3) * keep * 0.8)
    })])
}

fn paint_strokes(ctx: &Ctx) -> CookResult {
    let (len, wid, ang, ar, lr, s) = (ctx.f("length"), ctx.f("width"), ctx.f("angle"), ctx.f("angle_random"), ctx.f("luminance_random"), ctx.seed);
    let bristles = ctx.i("bristles").clamp(1, 256) as f32;
    let items: Vec<Inst> = (0..ctx.i("count").clamp(1, 5000) as u32)
        .map(|i| {
            let r = |k: u32| rand1(i * 8 + k, s);
            Inst {
                c: [r(0), r(1)],
                r: [len * 0.5 * (0.7 + 0.3 * r(2)), wid * 0.5 * (0.8 + 0.4 * r(3))],
                rot: ang + (r(4) - 0.5) * ar,
                lum: 1.0 - lr * r(5),
                pat: 0,
            }
        })
        .collect();
    Ok(vec![splat(ctx, &items, ctx.i("blending"), |i, x, y| {
        let sd = s ^ (i as u32).wrapping_mul(0x9e37_79b9);
        // Bristle density across the stroke; the ends are ragged per bristle.
        let b = value(y * bristles + 0.5, 0.5, 0, sd);
        let ragged = value(y * bristles * 0.5 + 3.1, x * 2.0, 0, sd ^ 1);
        let end = 1.0 - smoothstep(0.55 + 0.4 * ragged, 1.0, x.abs());
        let edge = 1.0 - smoothstep(0.7, 1.0, y.abs());
        end * edge * (0.45 + 0.55 * b)
    })])
}

fn scratches_generator(ctx: &Ctx) -> CookResult {
    const STEPS: u32 = 12;
    let (len, ang, ar, curv, fi, lr, s) =
        (ctx.f("length"), ctx.f("angle"), ctx.f("angle_random"), ctx.f("curvature"), ctx.f("field_intensity"), ctx.f("luminance_random"), ctx.seed);
    let (field, mask) = (ctx.input(0), ctx.input(1));
    let (w, h) = (ctx.w as f32, ctx.h as f32);
    let mut segs = Vec::new();
    for i in 0..ctx.i("count").clamp(0, 5000) as u32 {
        let r = |k: u32| rand1(i * 8 + k, s);
        let mut p = [r(0), r(1)];
        if mask.is_some_and(|m| r(2) >= m.sample_gray(p[0], p[1], ctx.tile)) {
            continue;
        }
        let (base, bend, lum) = (ang + (r(3) - 0.5) * ar, (r(4) - 0.5) * curv, 1.0 - lr * r(6));
        let step = len * (0.4 + 0.6 * r(5)) / STEPS as f32;
        for k in 0..STEPS {
            let t = k as f32 / STEPS as f32;
            let a = (base + bend * t + field.map_or(0.0, |f| f.sample_gray(p[0], p[1], ctx.tile) * fi)) * TAU;
            let q = [p[0] + a.cos() * step, p[1] - a.sin() * step];
            let taper = ((t + 0.5 / STEPS as f32) * PI).sin().sqrt();
            segs.push(Seg { a: [p[0] * w, p[1] * h], b: [q[0] * w, q[1] * h], wt: lum * taper });
            p = q;
        }
    }
    let caps = Capsules::new(segs, px(ctx, ctx.f("width")) * 0.5, ctx.w, ctx.h, ctx.tile);
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Gray, |x, y| [caps.coverage(x as f32 + 0.5, y as f32 + 0.5), 0.0, 0.0, 0.0])])
}

fn rust(ctx: &Ctx) -> CookResult {
    let (amt, s, pit, con, seed) = (ctx.f("amount"), whole(ctx, "scale"), ctx.f("pitting"), ctx.f("contrast"), ctx.seed);
    let grow = ctx.sized(0).map(|m| ops::gaussian(&m.to_gray(), px(ctx, ctx.f("spread")), ctx.tile));
    Ok(vec![gen_px(ctx, |i, u, v| {
        let blotch = fbm(u, v, s, 6, 0.55, seed);
        let base = match &grow {
            Some(g) => 0.5 * blotch + 0.6 * g.data.get(i).copied().unwrap_or(0.0),
            None => blotch,
        };
        let c = cellular(u * s * 8.0, v * s * 8.0, (s * 8.0) as i32, 1.0, 0, seed ^ 3);
        let pits = (1.0 - smoothstep(0.05, 0.4, c.f1)) * (c.id < pit) as i32 as f32;
        // Pits speckle the surroundings of the blotches only.
        thresh(base + pits * 0.2 * smoothstep(0.3, 0.5, base), amt, con) * (0.7 + 0.3 * fbm(u, v, s * 16.0, 3, 0.6, seed ^ 5))
    })])
}

fn moss(ctx: &Ctx) -> CookResult {
    let (lichen, amt, s, bias, con, seed) = (ctx.i("style") == 1, ctx.f("amount"), whole(ctx, "scale"), ctx.f("height_bias"), ctx.f("contrast"), ctx.seed);
    let hgt = ctx.sized(0).map(|h| h.to_gray());
    Ok(vec![gen_px(ctx, |i, u, v| {
        let hb = hgt.as_ref().map_or(0.0, |h| (h.data.get(i).copied().unwrap_or(0.5) - 0.5) * bias);
        let fine = |m: f32, sd: u32| value(u * s * m, v * s * m, (s * m) as i32, seed ^ sd);
        if !lichen {
            // Fuzzy clumps, thicker in low areas.
            let clump = fbm(u, v, s, 5, 0.6, seed) - hb + (fine(24.0, 9) - 0.5) * 0.25;
            return thresh(clump, amt, con) * (0.55 + 0.45 * fine(48.0, 10));
        }
        // Round rosettes with brighter rims, on high areas.
        let c = cellular(u * s, v * s, s as i32, 0.9, 0, seed);
        if c.id >= amt * 1.4 + hb {
            return 0.0;
        }
        let rad = (0.25 + 0.35 * (c.id * 7.31).fract()) * (0.8 + 0.4 * fbm(u, v, s * 8.0, 3, 0.5, seed ^ 4));
        let soft = 0.01 + (1.0 - con) * 0.06;
        (1.0 - smoothstep(rad - soft, rad, c.f1)) * (0.55 + 0.45 * smoothstep(rad * 0.4, rad, c.f1)) * (0.8 + 0.2 * fine(32.0, 6))
    })])
}

fn leaks(ctx: &Ctx) -> CookResult {
    let (w, h, seed) = (ctx.w as usize, ctx.h as usize, ctx.seed);
    let src = match ctx.sized(0) {
        Some(m) => m.to_gray(),
        None => ctx.gen_gray(|u, v| smoothstep(0.62, 0.7, fbm(u, v, 4.0, 4, 0.5, seed ^ 21))),
    };
    let (len, lr, streaks, wob) = (ctx.f("length").max(0.0) * h as f32, ctx.f("length_random"), whole(ctx, "streaks"), ctx.f("wobble"));
    // Each column carries its value down, fading over a length that varies across columns.
    let cols: Vec<Vec<f32>> = (0..w)
        .into_par_iter()
        .map(|x| {
            let u = (x as f32 + 0.5) / w as f32;
            let r = 0.6 * value(u * streaks, 0.5, streaks as i32, seed ^ 23) + 0.4 * value(u * streaks * 4.0, 0.5, (streaks * 4.0) as i32, seed ^ 24);
            let decay = (-3.0 / (len * (1.0 - lr * r)).max(1.0)).exp();
            let mut col = vec![0.0f32; h];
            let mut acc = 0.0f32;
            // Tiling: a second pass carries drips over the bottom edge to the top.
            let passes = if ctx.tile { 2 } else { 1 };
            for pass in 0..passes {
                for (y, out) in col.iter_mut().enumerate() {
                    acc = src.data.get(y * w + x).copied().unwrap_or(0.0).max(acc * decay);
                    if pass + 1 == passes {
                        *out = acc;
                    }
                }
            }
            col
        })
        .collect();
    let run = Image::gray(ctx.w, ctx.h, |x, y| cols.get(x as usize).and_then(|c| c.get(y as usize)).copied().unwrap_or(0.0));
    Ok(vec![ctx.gen_gray(|u, v| {
        let dx = (fbm(u, v, 8.0, 3, 0.5, seed ^ 22) - 0.5) * wob * 0.04;
        run.sample_gray(u + dx, v, ctx.tile) * (0.75 + 0.25 * value_xy(u * streaks * 2.0, v * 3.0, (streaks * 2.0) as i32, 3, seed ^ 25))
    })])
}

fn edge_damage(ctx: &Ctx) -> CookResult {
    let hgt = ctx.gray_or(0, 0.0);
    let data = hgt.data.clone();
    let solid = |i: usize| data.get(i).is_some_and(|v| *v > 0.5);
    let dist = ops::distance_sq(ctx.w, ctx.h, |i| !solid(i), ctx.tile);
    let (wpx, amt, s, rough, depth, seed) = (px(ctx, ctx.f("width")), ctx.f("amount"), whole(ctx, "scale"), ctx.f("roughness"), ctx.f("depth"), ctx.seed);
    let mask = gen_px(ctx, |i, u, v| {
        if !solid(i) {
            return 0.0;
        }
        let d = dist.get(i).map_or(0.0, |d| d.sqrt());
        // Chunky bites per cell, made ragged by noise.
        let c = cellular(u * s, v * s, s as i32, 1.0, 0, seed);
        let r = c.id * (1.0 - rough * 0.5) + (fbm(u, v, s * 2.0, 4, 0.5, seed ^ 3) - 0.5) * rough * 1.5;
        let reach = wpx * 2.0 * amt * r.max(0.0);
        1.0 - smoothstep(reach - 0.75, reach + 0.75, d)
    });
    let out = Image::gray(ctx.w, ctx.h, |x, y| {
        let i = (y * ctx.w + x) as usize;
        data.get(i).copied().unwrap_or(0.0) * (1.0 - depth * mask.data.get(i).copied().unwrap_or(0.0))
    });
    Ok(vec![out, mask])
}

#[cfg(test)]
#[path = "weathering.test.rs"]
mod tests;
