//! Texture generators › Patterns: shapes, tilings, gradients and scattered elements.

use std::f32::consts::{PI, TAU};

use materialcraft_image::{Image, Kind};

use crate::catalog::{CookResult, Ctx, NodeDef, node};
use crate::noise::{rand1, rand2};

pub const PATTERNS: &str = "Texture generators/Patterns";

pub const SHAPES: &[&str] = &[
    "Square",
    "Disc",
    "Paraboloid",
    "Bell",
    "Gaussian",
    "Thorn",
    "Pyramid",
    "Brick",
    "Gradation",
    "Waves",
    "Half bell",
    "Ridged bell",
    "Crescent",
    "Capsule",
    "Cone",
    "Hexagon",
];

/// A shape's value at local coordinates `(x, y)` in -1..1 (1 at the center, 0 outside).
pub fn shape(kind: i32, x: f32, y: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    let m = x.abs().max(y.abs());
    let v = match kind {
        0 => (m <= 1.0) as i32 as f32,
        1 => (r <= 1.0) as i32 as f32,
        2 => 1.0 - r * r,
        3 => {
            let t = (1.0 - r).max(0.0);
            t * t * (3.0 - 2.0 * t)
        }
        4 => (-r * r * 4.0).exp() * (r <= 1.0) as i32 as f32,
        5 => (1.0 - r).max(0.0).powi(3),
        6 => 1.0 - m,
        7 => {
            let bx = (1.0 - x.abs()) * 6.0;
            let by = (1.0 - y.abs()) * 3.0;
            bx.min(by).clamp(0.0, 1.0)
        }
        8 => (x * 0.5 + 0.5) * (m <= 1.0) as i32 as f32,
        9 => (x * PI * 2.0).cos() * 0.5 + 0.5,
        10 => {
            let t = (1.0 - r).max(0.0);
            if y < 0.0 { 0.0 } else { t * t * (3.0 - 2.0 * t) }
        }
        11 => {
            let t = (1.0 - r).max(0.0);
            (t * t * (3.0 - 2.0 * t)) * (1.0 - ((r * PI * 3.0).sin()).abs() * 0.4)
        }
        12 => {
            let r2 = ((x - 0.35) * (x - 0.35) + y * y).sqrt();
            ((r <= 1.0) && (r2 > 0.75)) as i32 as f32
        }
        13 => {
            let dx = (x.abs() - 0.5).max(0.0);
            let d = (dx * dx + y * y * 4.0).sqrt();
            (d <= 0.5) as i32 as f32
        }
        14 => (1.0 - r).max(0.0),
        15 => {
            let (ax, ay) = (x.abs(), y.abs());
            ((ax * 0.866 + ay * 0.5).max(ay) <= 0.95) as i32 as f32
        }
        _ => 0.0,
    };
    v.clamp(0.0, 1.0)
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let p = |id, label| node(id, label, PATTERNS).generator();
    vec![
        p("brick_generator", "Brick Generator")
            .desc("Rows of bricks with mortar gaps, bevels and random heights.")
            .int("bricks_x", "Bricks X", 4, 1, 64)
            .int("bricks_y", "Bricks Y", 8, 1, 64)
            .float("offset", "Row offset", 0.5, 0.0, 1.0)
            .float("gap", "Gap", 0.04, 0.0, 0.5)
            .float("bevel", "Bevel", 0.12, 0.0, 1.0)
            .float("height_random", "Height random", 0.3, 0.0, 1.0)
            .float("lum_random", "Luminance random", 0.0, 0.0, 1.0)
            .gray_out()
            .cook(bricks),
        p("checker", "Checker").desc("A checkerboard.").int("tiling", "Tiling", 4, 1, 64).gray_out().cook(checker),
        p("gradient_linear_1", "Gradient Linear 1")
            .desc("A ramp from black to white.")
            .int("tiling", "Tiling", 1, 1, 16)
            .boolean("vertical", "Vertical", false)
            .gray_out()
            .cook(gradient_linear_1),
        p("gradient_linear_2", "Gradient Linear 2")
            .desc("A mirrored ramp (white in the middle).")
            .int("tiling", "Tiling", 1, 1, 16)
            .boolean("vertical", "Vertical", false)
            .gray_out()
            .cook(gradient_linear_2),
        p("gradient_radial", "Gradient Radial")
            .desc("White at the center fading outward.")
            .float("radius", "Radius", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(gradient_radial),
        p("hexagons", "Hexagons")
            .desc("A honeycomb of beveled hexagons.")
            .int("tiling", "Tiling", 4, 1, 64)
            .float("size", "Size", 0.9, 0.0, 1.0)
            .float("bevel", "Bevel", 0.3, 0.0, 1.0)
            .gray_out()
            .cook(hexagons),
        p("polygon", "Polygon")
            .desc("A regular polygon.")
            .int("sides", "Sides", 6, 3, 32)
            .float("scale", "Scale", 0.8, 0.0, 1.0)
            .angle("angle", "Angle", 0.0)
            .float("gradient", "Gradient", 0.0, 0.0, 1.0)
            .int("tiling", "Tiling", 1, 1, 16)
            .gray_out()
            .cook(polygon),
        p("scratches", "Scratches")
            .desc("Random thin scratches.")
            .int("count", "Count", 60, 0, 1000)
            .float("length", "Length", 0.25, 0.0, 1.0)
            .float("width", "Width", 1.0, 0.1, 8.0)
            .angle("angle", "Angle", 0.1)
            .float("angle_random", "Angle random", 0.2, 0.0, 1.0)
            .gray_out()
            .cook(scratches),
        p("shape", "Shape")
            .desc("A single shape, optionally tiled.")
            .choice("pattern", "Pattern", SHAPES, 1)
            .int("tiling", "Tiling", 1, 1, 64)
            .float("size", "Size", 0.8, 0.0, 2.0)
            .float2("size_xy", "Size XY", [1.0, 1.0], 0.0, 2.0)
            .angle("angle", "Angle", 0.0)
            .gray_out()
            .cook(shape_node),
        p("splatter", "Splatter")
            .desc("Scattered shapes of random size and gray.")
            .choice("pattern", "Pattern", SHAPES, 4)
            .int("count", "Count", 64, 1, 2000)
            .float("size", "Size", 0.08, 0.0, 1.0)
            .float("size_random", "Size random", 0.6, 0.0, 1.0)
            .float("lum_random", "Luminance random", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(splatter),
        p("star", "Star")
            .desc("A star shape.")
            .int("points", "Points", 5, 2, 32)
            .float("inner", "Inner radius", 0.45, 0.0, 1.0)
            .float("scale", "Scale", 0.8, 0.0, 1.0)
            .int("tiling", "Tiling", 1, 1, 16)
            .gray_out()
            .cook(star),
        p("stripes", "Stripes")
            .desc("Parallel bands with soft edges.")
            .int("count", "Count", 8, 1, 128)
            .float("width", "Width", 0.5, 0.0, 1.0)
            .float("softness", "Softness", 0.0, 0.0, 1.0)
            .angle("angle", "Angle", 0.0)
            .gray_out()
            .cook(stripes),
        p("tile_generator", "Tile Generator")
            .desc("Places a shape on a grid with per-tile random size, position, rotation and luminance.")
            .choice("pattern", "Pattern", SHAPES, 0)
            .int("x_amount", "X amount", 8, 1, 128)
            .int("y_amount", "Y amount", 8, 1, 128)
            .float("scale", "Scale", 0.9, 0.0, 2.0)
            .float2("size_xy", "Size XY", [1.0, 1.0], 0.0, 2.0)
            .float("scale_random", "Scale random", 0.0, 0.0, 1.0)
            .float("offset", "Row offset", 0.0, 0.0, 1.0)
            .float("position_random", "Position random", 0.0, 0.0, 1.0)
            .float("rotation_random", "Rotation random", 0.0, 0.0, 1.0)
            .float("lum_random", "Luminance random", 0.0, 0.0, 1.0)
            .float("mask_random", "Mask random", 0.0, 0.0, 1.0)
            .gray_out()
            .cook(tile_generator),
        p("tile_sampler", "Tile Sampler")
            .desc("Like Tile Generator, but each tile shows the pattern input.")
            .gray_in("pattern", "Pattern input")
            .gray_in("scale_map", "Scale map")
            .int("x_amount", "X amount", 8, 1, 128)
            .int("y_amount", "Y amount", 8, 1, 128)
            .float("scale", "Scale", 1.0, 0.0, 2.0)
            .float("scale_random", "Scale random", 0.0, 0.0, 1.0)
            .float("offset", "Row offset", 0.0, 0.0, 1.0)
            .float("position_random", "Position random", 0.0, 0.0, 1.0)
            .float("rotation_random", "Rotation random", 0.0, 0.0, 1.0)
            .float("lum_random", "Luminance random", 0.0, 0.0, 1.0)
            .choice("blend", "Blending", &["Max", "Add"], 0)
            .gray_out()
            .cook(tile_sampler),
        p("weave", "Weave")
            .desc("Interlaced threads (height).")
            .int("tiling", "Tiling", 6, 1, 64)
            .float("width", "Thread width", 0.8, 0.0, 1.0)
            .gray_out()
            .cook(weave),
        p("waveform", "Waveform")
            .desc("Sine bands.")
            .int("count", "Count", 8, 1, 128)
            .float("amplitude", "Amplitude", 0.0, 0.0, 1.0)
            .angle("angle", "Angle", 0.0)
            .gray_out()
            .cook(waveform),
    ]
}

/// Rotate `(x, y)` by `turns`.
#[inline]
fn rot(x: f32, y: f32, turns: f32) -> (f32, f32) {
    let (s, c) = (turns * TAU).sin_cos();
    (x * c - y * s, x * s + y * c)
}

fn bricks(ctx: &Ctx) -> CookResult {
    let (bx, by) = (ctx.i("bricks_x").max(1) as f32, ctx.i("bricks_y").max(1) as f32);
    let (off, gap, bevel, hr, lr, s) = (ctx.f("offset"), ctx.f("gap"), ctx.f("bevel").max(1e-4), ctx.f("height_random"), ctx.f("lum_random"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = (v * by).floor();
        let x = u * bx + off * row;
        let col = x.floor();
        let (fx, fy) = (x - col, v * by - row);
        let colw = (col as i32).rem_euclid(bx as i32);
        // Distance to the brick's edge, in brick-height units.
        let ex = fx.min(1.0 - fx) * by / bx;
        let ey = fy.min(1.0 - fy);
        let e = ex.min(ey) - gap * 0.5;
        if e <= 0.0 {
            return 0.0;
        }
        let t = (e / (bevel * 0.5)).min(1.0);
        let rnd = rand2(colw, row as i32, s);
        let base = 1.0 - hr * rnd;
        let lum = 1.0 - lr * rand2(colw, row as i32, s ^ 99);
        (t * base * lum).clamp(0.0, 1.0)
    })])
}

fn checker(ctx: &Ctx) -> CookResult {
    let n = ctx.i("tiling").max(1) as f32;
    Ok(vec![ctx.gen_gray(|u, v| (((u * n).floor() + (v * n).floor()) as i32 & 1) as f32)])
}

fn gradient_linear_1(ctx: &Ctx) -> CookResult {
    let (n, vert) = (ctx.i("tiling").max(1) as f32, ctx.b("vertical"));
    Ok(vec![ctx.gen_gray(|u, v| ((if vert { v } else { u }) * n).fract())])
}

fn gradient_linear_2(ctx: &Ctx) -> CookResult {
    let (n, vert) = (ctx.i("tiling").max(1) as f32, ctx.b("vertical"));
    Ok(vec![ctx.gen_gray(|u, v| 1.0 - ((((if vert { v } else { u }) * n).fract()) * 2.0 - 1.0).abs())])
}

fn gradient_radial(ctx: &Ctx) -> CookResult {
    let r = ctx.f("radius").max(1e-3);
    Ok(vec![ctx.gen_gray(|u, v| (1.0 - ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / r).clamp(0.0, 1.0))])
}

fn hexagons(ctx: &Ctx) -> CookResult {
    let n = ctx.i("tiling").max(1) as f32;
    let (size, bevel) = (ctx.f("size"), ctx.f("bevel").max(1e-3));
    let sq3 = 3f32.sqrt();
    // Flat-top hexes of circumradius 1: two lattices, (3i, √3j) and (3i + 1.5, √3j + √3/2). The
    // image holds n lattice periods across and the nearest whole number down, so it tiles.
    let m = (n * 3.0 / sq3).round().max(1.0);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (px, py) = (u * n * 3.0, v * m * sq3);
        let a = ((px / 3.0).round() * 3.0, (py / sq3).round() * sq3);
        let b = (((px - 1.5) / 3.0).round() * 3.0 + 1.5, ((py - sq3 * 0.5) / sq3).round() * sq3 + sq3 * 0.5);
        let da = (px - a.0).powi(2) + (py - a.1).powi(2);
        let db = (px - b.0).powi(2) + (py - b.1).powi(2);
        let (cx, cy) = if da < db { a } else { b };
        let (dx, dy) = ((px - cx).abs(), (py - cy).abs());
        let inr = sq3 * 0.5;
        let d = (dy / inr).max((dx * inr + dy * 0.5) / inr);
        ((size - d) / bevel).clamp(0.0, 1.0)
    })])
}

fn tiled(u: f32, v: f32, n: f32) -> (f32, f32) {
    (((u * n).fract() - 0.5) * 2.0, ((v * n).fract() - 0.5) * 2.0)
}

fn polygon(ctx: &Ctx) -> CookResult {
    let sides = ctx.i("sides").clamp(3, 64) as f32;
    let (sc, ang, grad, n) = (ctx.f("scale").max(1e-3), ctx.f("angle"), ctx.f("gradient"), ctx.i("tiling").max(1) as f32);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, n);
        let (x, y) = rot(x, y, ang);
        let a = y.atan2(x);
        let seg = TAU / sides;
        let d = (x * x + y * y).sqrt() * ((a.rem_euclid(seg) - seg * 0.5).cos()) / (seg * 0.5).cos();
        let e = 1.0 - d / sc;
        if grad <= 0.0 { (e >= 0.0) as i32 as f32 } else { (e / grad).clamp(0.0, 1.0) }
    })])
}

fn scratches(ctx: &Ctx) -> CookResult {
    let count = ctx.i("count").clamp(0, 5000) as u32;
    let (len, width, ang, ar, s) = (ctx.f("length"), ctx.f("width") / ctx.w as f32, ctx.f("angle"), ctx.f("angle_random"), ctx.seed);
    let segs: Vec<([f32; 2], [f32; 2], f32)> = (0..count)
        .map(|i| {
            let p = [rand1(i * 4, s), rand1(i * 4 + 1, s)];
            let a = (ang + (rand1(i * 4 + 2, s) - 0.5) * ar) * TAU;
            let l = len * (0.5 + 0.5 * rand1(i * 4 + 3, s));
            ([p[0], p[1]], [a.cos() * l, -a.sin() * l], 0.4 + 0.6 * rand1(i * 7 + 5, s))
        })
        .collect();
    // Bin segments by their bounding boxes on a coarse grid so each pixel tests only nearby ones.
    const G: usize = 16;
    let mut bins: Vec<Vec<usize>> = vec![Vec::new(); G * G];
    for (k, (p, d, _)) in segs.iter().enumerate() {
        let (x0, x1) = (p[0].min(p[0] + d[0]) - width, p[0].max(p[0] + d[0]) + width);
        let (y0, y1) = (p[1].min(p[1] + d[1]) - width, p[1].max(p[1] + d[1]) + width);
        // At most one full turn of bins per axis, so no segment lands twice in a bin.
        let (gy0, gx0) = ((y0 * G as f32).floor() as i64, (x0 * G as f32).floor() as i64);
        let gy1 = ((y1 * G as f32).floor() as i64).min(gy0.saturating_add(G as i64 - 1));
        let gx1 = ((x1 * G as f32).floor() as i64).min(gx0.saturating_add(G as i64 - 1));
        for gy in gy0..=gy1 {
            for gx in gx0..=gx1 {
                if let Some(b) = bins.get_mut(gy.rem_euclid(G as i64) as usize * G + gx.rem_euclid(G as i64) as usize) {
                    b.push(k);
                }
            }
        }
    }
    Ok(vec![ctx.gen_gray(|u, v| {
        let bin = &bins[((v * G as f32) as usize).min(G - 1) * G + ((u * G as f32) as usize).min(G - 1)];
        let mut best: f32 = 0.0;
        for k in bin {
            let (p, d, lum) = segs[*k];
            for (ox, oy) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let (qx, qy) = (u - p[0] + ox, v - p[1] + oy);
                let t = ((qx * d[0] + qy * d[1]) / (d[0] * d[0] + d[1] * d[1]).max(1e-9)).clamp(0.0, 1.0);
                let (ex, ey) = (qx - d[0] * t, qy - d[1] * t);
                let dist = (ex * ex + ey * ey).sqrt();
                if dist < width {
                    best = best.max((1.0 - dist / width) * lum * (1.0 - (t * 2.0 - 1.0).abs().powi(4)));
                }
            }
        }
        best
    })])
}

fn shape_node(ctx: &Ctx) -> CookResult {
    let (k, n, size, sxy, ang) = (ctx.i("pattern"), ctx.i("tiling").max(1) as f32, ctx.f("size").max(1e-3), ctx.v2("size_xy"), ctx.f("angle"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, n);
        let (x, y) = rot(x, y, ang);
        shape(k, x / (size * sxy[0].max(1e-3)), y / (size * sxy[1].max(1e-3)))
    })])
}

fn splatter(ctx: &Ctx) -> CookResult {
    let count = ctx.i("count").clamp(1, 2000) as u32;
    let (k, size, sr, lr, s) = (ctx.i("pattern"), ctx.f("size"), ctx.f("size_random"), ctx.f("lum_random"), ctx.seed);
    let items: Vec<([f32; 2], f32, f32, f32)> = (0..count)
        .map(|i| ([rand1(i * 5, s), rand1(i * 5 + 1, s)], size * (1.0 - sr * rand1(i * 5 + 2, s)), 1.0 - lr * rand1(i * 5 + 3, s), rand1(i * 5 + 4, s)))
        .collect();
    let base = Image::filled(ctx.w, ctx.h, Kind::Gray, [0.0; 4]);
    Ok(vec![stamp(ctx, base, &items, |x, y, _| shape(k, x, y), |a, b| a.max(b))])
}

/// Splat `items` (center, radius, luminance, rotation turns) onto `base` with `f` combining.
fn stamp(
    ctx: &Ctx,
    base: Image,
    items: &[([f32; 2], f32, f32, f32)],
    sh: impl Fn(f32, f32, usize) -> f32 + Sync,
    combine: impl Fn(f32, f32) -> f32 + Sync,
) -> Image {
    const G: usize = 32;
    let mut bins: Vec<Vec<usize>> = vec![Vec::new(); G * G];
    for (i, (c, r, _, _)) in items.iter().enumerate() {
        let r = r * 1.42;
        // At most one full turn of bins per axis, so no item lands twice in a bin.
        let (gy0, gx0) = (((c[1] - r) * G as f32).floor() as i64, ((c[0] - r) * G as f32).floor() as i64);
        let gy1 = (((c[1] + r) * G as f32).floor() as i64).min(gy0.saturating_add(G as i64 - 1));
        let gx1 = (((c[0] + r) * G as f32).floor() as i64).min(gx0.saturating_add(G as i64 - 1));
        for gy in gy0..=gy1 {
            for gx in gx0..=gx1 {
                if let Some(b) = bins.get_mut(gy.rem_euclid(G as i64) as usize * G + gx.rem_euclid(G as i64) as usize) {
                    b.push(i);
                }
            }
        }
    }
    let tile = ctx.tile;
    Image::generate_uv(ctx.w, ctx.h, Kind::Gray, |u, v| {
        let mut acc = base.sample_nearest(u, v, false)[0];
        let bin = &bins[((v * G as f32) as usize).min(G - 1) * G + ((u * G as f32) as usize).min(G - 1)];
        for i in bin {
            let (c, r, lum, rt) = items[*i];
            if r <= 0.0 {
                continue;
            }
            let mut dx = u - c[0];
            let mut dy = v - c[1];
            if tile {
                dx -= dx.round();
                dy -= dy.round();
            }
            let (x, y) = rot(dx / r, dy / r, rt);
            if x.abs() <= 1.0 && y.abs() <= 1.0 {
                acc = combine(acc, sh(x, y, *i) * lum);
            }
        }
        [acc, 0.0, 0.0, 0.0]
    })
}

fn star(ctx: &Ctx) -> CookResult {
    let pts = ctx.i("points").clamp(2, 64) as f32;
    let (inner, sc, n) = (ctx.f("inner"), ctx.f("scale").max(1e-3), ctx.i("tiling").max(1) as f32);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, n);
        let a = y.atan2(x) + PI / 2.0;
        let seg = TAU / pts;
        let t = ((a.rem_euclid(seg) / seg) * 2.0 - 1.0).abs();
        let rad = sc * (inner + (1.0 - inner) * t);
        ((x * x + y * y).sqrt() <= rad) as i32 as f32
    })])
}

fn stripes(ctx: &Ctx) -> CookResult {
    let n = ctx.i("count").max(1) as f32;
    let (w, soft, ang) = (ctx.f("width"), ctx.f("softness").max(1e-3), ctx.f("angle"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, _) = rot(u - 0.5, v - 0.5, ang);
        let f = (x * n).rem_euclid(1.0);
        let d = (f - 0.5).abs() * 2.0;
        ((w - d) / soft + 0.5).clamp(0.0, 1.0)
    })])
}

fn tile_params(ctx: &Ctx) -> (f32, f32, f32, f32, f32, f32, f32, f32, u32) {
    (
        ctx.i("x_amount").max(1) as f32,
        ctx.i("y_amount").max(1) as f32,
        ctx.f("scale"),
        ctx.f("scale_random"),
        ctx.f("offset"),
        ctx.f("position_random"),
        ctx.f("rotation_random"),
        ctx.f("lum_random"),
        ctx.seed,
    )
}

fn tile_generator(ctx: &Ctx) -> CookResult {
    let (nx, ny, sc, sr, off, pr, rr, lr, s) = tile_params(ctx);
    let (k, sxy, mr) = (ctx.i("pattern"), ctx.v2("size_xy"), ctx.f("mask_random"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = (v * ny).floor();
        let mut best: f32 = 0.0;
        // Shapes may reach into neighbouring cells (scale > 1, position jitter): check 3×3.
        for dj in -1..=1 {
            for di in -1..=1 {
                let r = row + dj as f32;
                let x = u * nx + off * r;
                let col = x.floor() + di as f32;
                let (ci, ri) = ((col as i32).rem_euclid(nx as i32), (r as i32).rem_euclid(ny as i32));
                if mr > 0.0 && rand2(ci, ri, s ^ 0x55) < mr {
                    continue;
                }
                let jx = (rand2(ci, ri, s ^ 1) - 0.5) * pr;
                let jy = (rand2(ci, ri, s ^ 2) - 0.5) * pr;
                let size = sc * (1.0 - sr * rand2(ci, ri, s ^ 3));
                if size <= 0.0 {
                    continue;
                }
                let (lx, ly) = ((x - col - 0.5 - jx) * 2.0, (v * ny - r - 0.5 - jy) * 2.0);
                let (lx, ly) = rot(lx, ly, (rand2(ci, ri, s ^ 4) - 0.5) * rr);
                let val = shape(k, lx / (size * sxy[0].max(1e-3)), ly / (size * sxy[1].max(1e-3)));
                best = best.max(val * (1.0 - lr * rand2(ci, ri, s ^ 5)));
            }
        }
        best
    })])
}

fn tile_sampler(ctx: &Ctx) -> CookResult {
    let (nx, ny, sc, sr, off, pr, rr, lr, s) = tile_params(ctx);
    let pattern =
        ctx.input(0).map(|p| p.to_gray()).unwrap_or_else(|| Image::generate_uv(64, 64, Kind::Gray, |u, v| [shape(3, u * 2.0 - 1.0, v * 2.0 - 1.0); 4]));
    let smap = ctx.input(1).map(|p| p.to_gray());
    let add = ctx.i("blend") == 1;
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = (v * ny).floor();
        let mut acc: f32 = 0.0;
        for dj in -1..=1 {
            for di in -1..=1 {
                let r = row + dj as f32;
                let x = u * nx + off * r;
                let col = x.floor() + di as f32;
                let (ci, ri) = ((col as i32).rem_euclid(nx as i32), (r as i32).rem_euclid(ny as i32));
                let jx = (rand2(ci, ri, s ^ 1) - 0.5) * pr;
                let jy = (rand2(ci, ri, s ^ 2) - 0.5) * pr;
                let mapk = smap.as_ref().map_or(1.0, |m| m.sample_gray((col + 0.5) / nx, (r + 0.5) / ny, true));
                let size = sc * mapk * (1.0 - sr * rand2(ci, ri, s ^ 3));
                if size <= 0.0 {
                    continue;
                }
                let (lx, ly) = ((x - col - 0.5 - jx) / size, (v * ny - r - 0.5 - jy) / size);
                let (lx, ly) = rot(lx, ly, (rand2(ci, ri, s ^ 4) - 0.5) * rr);
                if lx.abs() > 0.5 || ly.abs() > 0.5 {
                    continue;
                }
                let val = pattern.sample_gray(lx + 0.5, ly + 0.5, false) * (1.0 - lr * rand2(ci, ri, s ^ 5));
                acc = if add { acc + val } else { acc.max(val) };
            }
        }
        acc.min(1.0)
    })])
}

fn weave(ctx: &Ctx) -> CookResult {
    let n = ctx.i("tiling").max(1) as f32;
    let w = ctx.f("width").clamp(0.01, 1.0);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = (u * n, v * n);
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x.fract(), y.fract());
        // Horizontal thread across the cell row, vertical thread down the column; which is on
        // top alternates in a checker.
        let hthread = ((fy - 0.5).abs() * 2.0 <= w) as i32 as f32 * (1.0 - ((fy - 0.5) * 2.0 / w).powi(2)).max(0.0).sqrt();
        let vthread = ((fx - 0.5).abs() * 2.0 <= w) as i32 as f32 * (1.0 - ((fx - 0.5) * 2.0 / w).powi(2)).max(0.0).sqrt();
        let h_top = (cx + cy) & 1 == 0;
        // The thread on top bulges along its length.
        let hb = hthread * (0.6 + 0.4 * (fx * PI).sin());
        let vb = vthread * (0.6 + 0.4 * (fy * PI).sin());
        if h_top { hb.max(vb * 0.6) } else { vb.max(hb * 0.6) }
    })])
}

fn waveform(ctx: &Ctx) -> CookResult {
    let n = ctx.i("count").max(1) as f32;
    let (amp, ang) = (ctx.f("amplitude"), ctx.f("angle"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = rot(u - 0.5, v - 0.5, ang);
        let w = (y * TAU * 2.0).sin() * amp * 0.1;
        ((x + w) * TAU * n).sin() * 0.5 + 0.5
    })])
}

#[cfg(test)]
#[path = "patterns.test.rs"]
mod tests;
