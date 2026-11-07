//! More filters: tone remaps and per-channel adjustments, two-input math, median and unsharp
//! mask, Sobel edges, glows and shadows from masks, curvature from height, and normal map tools.

use std::f32::consts::TAU;

use materialcraft_image::{Image, Kind};

use crate::catalog::{CookResult, Ctx, NodeDef, PortKind, node};
use crate::flood_fill::region_rand;
use crate::noise::fbm;
use crate::noises_more::smoothstep;
use crate::ops;

const ADJ: &str = "Filters/Adjustments";
const BLENDING: &str = "Filters/Blending";
const BLURS: &str = "Filters/Blurs";
const EFFECTS: &str = "Filters/Effects";
const NORMAL: &str = "Filters/Normal map";

/// A tangent-space normal from a normal map pixel (0..1 → -1..1).
#[inline]
pub(crate) fn unpack(p: [f32; 4]) -> [f32; 3] {
    [p[0] * 2.0 - 1.0, p[1] * 2.0 - 1.0, p[2] * 2.0 - 1.0]
}

/// A normalized normal back to a normal map pixel.
#[inline]
pub(crate) fn pack(n: [f32; 3], a: f32) -> [f32; 4] {
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    [n[0] / l * 0.5 + 0.5, n[1] / l * 0.5 + 0.5, n[2] / l * 0.5 + 0.5, a]
}

/// The pixel offset of `dist` (pixels at 256) along `angle` (turns, y down).
fn offset(ctx: &Ctx, angle: f32, dist: f32) -> (i64, i64) {
    let (s, c) = (angle * TAU).sin_cos();
    let d = dist * ctx.w as f32 / 256.0;
    ((c * d).round() as i64, (-s * d).round() as i64)
}

fn px(ctx: &Ctx, v: f32) -> f32 {
    v * ctx.w as f32 / 256.0
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let two = |id, label, desc| node(id, label, BLENDING).desc(desc).any_in("input_a", "A").any_in("input_b", "B");
    let mask_in = |id, label, cat| node(id, label, cat).gray_in("input", "Mask");
    vec![
        // ---- Adjustments
        node("remap", "Remap", ADJ)
            .desc("Maps the input range linearly onto the output range.")
            .any_in("input", "Input")
            .float("in_min", "Input min", 0.0, 0.0, 1.0)
            .float("in_max", "Input max", 1.0, 0.0, 1.0)
            .float("out_min", "Output min", 0.0, 0.0, 1.0)
            .float("out_max", "Output max", 1.0, 0.0, 1.0)
            .boolean("clamp", "Clamp", true)
            .cook(remap),
        node("smoothstep", "Smoothstep", ADJ)
            .desc("A smooth S-shaped step from the low edge (black) to the high edge (white).")
            .any_in("input", "Input")
            .float("low", "Low edge", 0.25, 0.0, 1.0)
            .float("high", "High edge", 0.75, 0.0, 1.0)
            .cook(smoothstep_node),
        node("s_curve", "S-Curve Contrast", ADJ)
            .desc("Raises contrast around a pivot with a smooth S curve (keeps black and white).")
            .any_in("input", "Input")
            .float("amount", "Amount", 0.5, 0.0, 1.0)
            .float("pivot", "Pivot", 0.5, 0.01, 0.99)
            .cook(s_curve),
        node("channel_levels", "Channel Levels", ADJ)
            .desc("Levels with a separate input range per channel.")
            .any_in("input", "Input")
            .color("low", "Input low", [0.0, 0.0, 0.0, 0.0])
            .color("high", "Input high", [1.0, 1.0, 1.0, 1.0])
            .float("gamma", "Gamma", 1.0, 0.1, 4.0)
            .cook(channel_levels),
        node("auto_levels_channels", "Auto Levels per Channel", ADJ)
            .desc("Stretches each color channel separately to the full 0..1 range.")
            .any_in("input", "Input")
            .cook(auto_levels_channels),
        node("invert_channels", "Invert Channels", ADJ)
            .desc("Inverts the chosen channels.")
            .any_in("input", "Input")
            .boolean("red", "Red", true)
            .boolean("green", "Green", true)
            .boolean("blue", "Blue", true)
            .boolean("alpha", "Alpha", false)
            .cook(invert_channels),
        node("clamp_channels", "Clamp Channels", ADJ)
            .desc("Clamps each channel between its own minimum and maximum.")
            .any_in("input", "Input")
            .color("min", "Min", [0.0, 0.0, 0.0, 0.0])
            .color("max", "Max", [1.0, 1.0, 1.0, 1.0])
            .cook(clamp_channels),
        node("color_variation", "Color Variation", ADJ)
            .desc("Random hue, saturation and value shifts: per region when a flood fill index map is connected, else from soft noise.")
            .color_in("input", "Input")
            .color_in("index", "Index map (optional)")
            .float("hue", "Hue random", 0.03, 0.0, 0.5)
            .float("saturation", "Saturation random", 0.15, 0.0, 1.0)
            .float("value", "Value random", 0.2, 0.0, 1.0)
            .float("scale", "Noise scale", 4.0, 1.0, 64.0)
            .color_out()
            .cook(color_variation),
        // ---- Blending
        node("lerp", "Lerp", BLENDING)
            .desc("Linear interpolation from A to B by a mask (or a constant position without one).")
            .any_in("input_a", "A")
            .any_in("input_b", "B")
            .gray_in("mask", "Mask")
            .float("position", "Position (no mask)", 0.5, 0.0, 1.0)
            .cook(lerp),
        two("minimum", "Minimum", "The darker of A and B, per channel.").cook(minimum),
        two("maximum", "Maximum", "The brighter of A and B, per channel.").cook(maximum),
        two("multiply", "Multiply", "A times B.").cook(multiply),
        two("difference", "Difference", "The absolute difference of A and B.").cook(difference),
        two("add", "Add", "A plus B.").boolean("clamp", "Clamp", true).cook(add),
        two("subtract", "Subtract", "A minus B.").boolean("clamp", "Clamp", true).cook(subtract),
        // ---- Blurs
        node("median", "Median", BLURS)
            .desc("Median of each neighbourhood: removes specks while keeping edges.")
            .any_in("input", "Input")
            .choice("size", "Size", &["3 × 3", "5 × 5"], 0)
            .cook(median),
        node("unsharp_mask", "Unsharp Mask", BLURS)
            .desc("Sharpens by adding back the difference from a blurred copy.")
            .any_in("input", "Input")
            .float("radius", "Radius", 2.0, 0.0, 32.0)
            .float("amount", "Amount", 1.0, 0.0, 5.0)
            .float("threshold", "Threshold", 0.0, 0.0, 1.0)
            .cook(unsharp_mask),
        // ---- Effects
        node("sobel", "Sobel Edges", EFFECTS)
            .desc("Edge strength from the Sobel operator.")
            .gray_in("input", "Input")
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .gray_out()
            .cook(sobel),
        mask_in("outer_glow", "Outer Glow", EFFECTS)
            .desc("A glow fading outward from a mask (by exact distance).")
            .float("size", "Size", 16.0, 0.0, 256.0)
            .float("intensity", "Intensity", 1.0, 0.0, 1.0)
            .float("falloff", "Falloff", 2.0, 0.25, 8.0)
            .gray_out()
            .cook(outer_glow),
        mask_in("inner_shadow", "Inner Shadow", EFFECTS)
            .desc("Darkens the inside of a mask along the edges facing the light.")
            .angle("angle", "Light angle", -0.125)
            .float("distance", "Distance", 6.0, 0.0, 64.0)
            .float("size", "Size", 8.0, 0.0, 128.0)
            .float("opacity", "Opacity", 0.75, 0.0, 1.0)
            .gray_out()
            .cook(inner_shadow),
        mask_in("drop_shadow", "Drop Shadow", EFFECTS)
            .desc("An offset, blurred shadow under a mask.")
            .angle("angle", "Angle", -0.125)
            .float("distance", "Distance", 8.0, 0.0, 128.0)
            .float("size", "Blur", 4.0, 0.0, 64.0)
            .float("opacity", "Opacity", 0.75, 0.0, 1.0)
            .output("output", "Output", PortKind::Gray)
            .output("shadow", "Shadow", PortKind::Gray)
            .cook(drop_shadow),
        node("height_curvature", "Curvature from Height", EFFECTS)
            .desc("Convex areas above 0.5, concave below, from a height map.")
            .gray_in("input", "Height")
            .float("radius", "Radius", 2.0, 0.25, 32.0)
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .gray_out()
            .cook(height_curvature),
        mask_in("outline", "Outline", EFFECTS)
            .desc("A line of constant width around the edges of a mask.")
            .float("width", "Width", 4.0, 0.0, 64.0)
            .choice("position", "Position", &["Outside", "Center", "Inside"], 1)
            .gray_out()
            .cook(outline),
        // ---- Normal map
        node("normal_combine", "Normal Combine", NORMAL)
            .desc("Combines a detail normal map over a base one (whiteout blending).")
            .color_in("base", "Base normal")
            .color_in("detail", "Detail normal")
            .float("intensity", "Detail intensity", 1.0, 0.0, 4.0)
            .color_out()
            .cook(normal_combine),
        node("normal_rotate", "Normal Rotate", NORMAL)
            .desc("Rotates the directions of a normal map (for rotated textures).")
            .color_in("input", "Normal")
            .angle("angle", "Angle", 0.0)
            .color_out()
            .cook(normal_rotate),
        node("normal_sobel", "Normal Sobel", NORMAL)
            .desc("A normal map from a height map with Sobel slopes (smoother than plain differences).")
            .gray_in("input", "Height")
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .choice("format", "Format", &["OpenGL", "DirectX"], 0)
            .color_out()
            .cook(normal_sobel),
    ]
}

fn src(ctx: &Ctx) -> Image {
    ctx.any_or(0, Kind::Gray, [0.0, 0.0, 0.0, 1.0])
}

/// Map R, G and B (gray: the value) with `f(value, channel)`; alpha kept.
fn per_channel(ctx: &Ctx, f: impl Fn(f32, usize) -> f32 + Sync) -> CookResult {
    let img = src(ctx);
    Ok(vec![img.map(img.kind, |p| [f(p[0], 0), f(p[1], 1), f(p[2], 2), p[3]])])
}

fn remap(ctx: &Ctx) -> CookResult {
    let (a, b, c, d, cl) = (ctx.f("in_min"), ctx.f("in_max"), ctx.f("out_min"), ctx.f("out_max"), ctx.b("clamp"));
    let span = if (b - a).abs() < 1e-6 { 1e-6 } else { b - a };
    per_channel(ctx, |v, _| {
        let t = (v - a) / span;
        c + (d - c) * if cl { t.clamp(0.0, 1.0) } else { t }
    })
}

fn smoothstep_node(ctx: &Ctx) -> CookResult {
    let (lo, hi) = (ctx.f("low"), ctx.f("high"));
    per_channel(ctx, |v, _| smoothstep(lo, hi.max(lo + 1e-4), v))
}

fn s_curve(ctx: &Ctx) -> CookResult {
    // A negative k on a zero base gives inf; clamp it (k = 0 gives the finite 1).
    let (k, p) = ((1.0 + ctx.f("amount") * 4.0).max(0.0), ctx.f("pivot").clamp(0.01, 0.99));
    per_channel(ctx, |v, _| {
        let v = v.clamp(0.0, 1.0);
        if v < p { p * (v / p).powf(k) } else { 1.0 - (1.0 - p) * ((1.0 - v) / (1.0 - p)).powf(k) }
    })
}

fn channel_levels(ctx: &Ctx) -> CookResult {
    let (lo, hi, g) = (ctx.v4("low"), ctx.v4("high"), 1.0 / ctx.f("gamma").max(0.01));
    per_channel(ctx, |v, c| {
        let span = (hi[c] - lo[c]).abs().max(1e-6).copysign(hi[c] - lo[c]);
        ((v - lo[c]) / span).clamp(0.0, 1.0).powf(g)
    })
}

fn auto_levels_channels(ctx: &Ctx) -> CookResult {
    let img = src(ctx);
    let c = img.channels();
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in img.data.chunks(c) {
        for k in 0..c.min(3) {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    per_channel(ctx, |v, k| {
        let k = if c == 1 { 0 } else { k };
        let span = hi[k] - lo[k];
        if span.is_finite() && span > 1e-6 { (v - lo[k]) / span } else { v }
    })
}

fn invert_channels(ctx: &Ctx) -> CookResult {
    let img = src(ctx);
    let flags = [ctx.b("red"), ctx.b("green"), ctx.b("blue"), ctx.b("alpha")];
    Ok(vec![img.map(img.kind, |p| std::array::from_fn(|k| if flags[k] { 1.0 - p[k] } else { p[k] }))])
}

fn clamp_channels(ctx: &Ctx) -> CookResult {
    let img = src(ctx);
    let (lo, hi) = (ctx.v4("min"), ctx.v4("max"));
    Ok(vec![img.map(img.kind, |p| std::array::from_fn(|k| p[k].max(lo[k]).min(hi[k].max(lo[k]))))])
}

fn color_variation(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]).to_color();
    let index = ctx.input(1);
    let (hr, sr, vr, sc, s, tile) = (ctx.f("hue"), ctx.f("saturation"), ctx.f("value"), ctx.f("scale").max(1.0).round(), ctx.seed, ctx.tile);
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, Kind::Color, |u, v| {
        let p = img.sample_nearest(u, v, false);
        // Three random shifts in -1..1: per flood fill region, or from noise.
        let r: [f32; 3] = match index.map(|i| i.sample_nearest(u, v, tile)) {
            Some(q) if q[3] > 0.5 => std::array::from_fn(|k| region_rand(q[2], 10 + k as u32, s) * 2.0 - 1.0),
            Some(_) => [0.0; 3],
            None => std::array::from_fn(|k| (fbm(u, v, sc, 3, 0.5, s.wrapping_add(k as u32 * 101)) - 0.5) * 4.0),
        };
        let hsl = ops::rgb_to_hsl(p);
        let c = ops::hsl_to_rgb(hsl[0] + hr * r[0], (hsl[1] * (1.0 + sr * r[1])).clamp(0.0, 1.0), (hsl[2] * (1.0 + vr * r[2])).max(0.0));
        [c[0], c[1], c[2], p[3]]
    })])
}

fn lerp(ctx: &Ctx) -> CookResult {
    let kind = ctx.main_kind(Kind::Gray);
    let (a, b) = (ctx.any_or(0, kind, [0.0, 0.0, 0.0, 1.0]).to_kind(kind), ctx.any_or(1, kind, [1.0; 4]).to_kind(kind));
    let m = ctx.gray_or(2, ctx.f("position"));
    Ok(vec![Image::generate(ctx.w, ctx.h, kind, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let (p, q, t) = (a.get(x, y, false), b.get(x, y, false), m.get_gray(x, y, false));
        std::array::from_fn(|k| p[k] + (q[k] - p[k]) * t)
    })])
}

/// Per-channel `f(a, b)` of two inputs (in the kind of the first connected one); B defaults to
/// `b_default`; alpha from A.
fn binary(ctx: &Ctx, b_default: f32, f: impl Fn(f32, f32) -> f32 + Sync) -> CookResult {
    let kind = ctx.main_kind(Kind::Gray);
    let a = ctx.any_or(0, kind, [0.0, 0.0, 0.0, 1.0]).to_kind(kind);
    let b = ctx.any_or(1, kind, [b_default, b_default, b_default, 1.0]).to_kind(kind);
    Ok(vec![Image::generate(ctx.w, ctx.h, kind, |x, y| {
        let (p, q) = (a.get(x as i64, y as i64, false), b.get(x as i64, y as i64, false));
        [f(p[0], q[0]), f(p[1], q[1]), f(p[2], q[2]), p[3]]
    })])
}

fn minimum(ctx: &Ctx) -> CookResult {
    binary(ctx, 1.0, f32::min)
}

fn maximum(ctx: &Ctx) -> CookResult {
    binary(ctx, 0.0, f32::max)
}

fn multiply(ctx: &Ctx) -> CookResult {
    binary(ctx, 1.0, |a, b| a * b)
}

fn difference(ctx: &Ctx) -> CookResult {
    binary(ctx, 0.0, |a, b| (a - b).abs())
}

fn add(ctx: &Ctx) -> CookResult {
    let cl = ctx.b("clamp");
    binary(ctx, 0.0, |a, b| if cl { (a + b).clamp(0.0, 1.0) } else { a + b })
}

fn subtract(ctx: &Ctx) -> CookResult {
    let cl = ctx.b("clamp");
    binary(ctx, 0.0, |a, b| if cl { (a - b).clamp(0.0, 1.0) } else { a - b })
}

fn median(ctx: &Ctx) -> CookResult {
    let img = src(ctx);
    let r: i64 = if ctx.i("size") == 1 { 2 } else { 1 };
    let c = img.channels();
    let tile = ctx.tile;
    Ok(vec![Image::generate(ctx.w, ctx.h, img.kind, |x, y| {
        let mut buf = [[0.0f32; 25]; 4];
        let mut n = 0;
        for dy in -r..=r {
            for dx in -r..=r {
                let p = img.get(x as i64 + dx, y as i64 + dy, tile);
                for (k, b) in buf.iter_mut().enumerate() {
                    b[n] = p[k];
                }
                n += 1;
            }
        }
        let mut out = [0.0; 4];
        for (k, b) in buf.iter_mut().enumerate().take(c) {
            let s = &mut b[..n];
            let (_, m, _) = s.select_nth_unstable_by(n / 2, f32::total_cmp);
            out[k] = *m;
        }
        out
    })])
}

fn unsharp_mask(ctx: &Ctx) -> CookResult {
    let img = src(ctx);
    let blurred = ops::gaussian(&img, px(ctx, ctx.f("radius")), ctx.tile);
    let (amt, th) = (ctx.f("amount"), ctx.f("threshold"));
    Ok(vec![Image::generate(ctx.w, ctx.h, img.kind, |x, y| {
        let (p, q) = (img.get(x as i64, y as i64, false), blurred.get(x as i64, y as i64, false));
        let f = |a: f32, b: f32| if (a - b).abs() > th { a + (a - b) * amt } else { a };
        [f(p[0], q[0]), f(p[1], q[1]), f(p[2], q[2]), p[3]]
    })])
}

/// Sobel slopes (dx, dy) of a gray image, in value units per pixel.
fn sobel_slope(img: &Image, x: i64, y: i64, tile: bool) -> (f32, f32) {
    let g = |dx: i64, dy: i64| img.get_gray(x + dx, y + dy, tile);
    let gx = g(1, -1) + 2.0 * g(1, 0) + g(1, 1) - g(-1, -1) - 2.0 * g(-1, 0) - g(-1, 1);
    let gy = g(-1, 1) + 2.0 * g(0, 1) + g(1, 1) - g(-1, -1) - 2.0 * g(0, -1) - g(1, -1);
    (gx / 8.0, gy / 8.0)
}

fn sobel(ctx: &Ctx) -> CookResult {
    let img = ctx.gray_or(0, 0.0);
    let k = ctx.f("intensity") * 2.0;
    Ok(vec![Image::gray(ctx.w, ctx.h, |x, y| {
        let (dx, dy) = sobel_slope(&img, x as i64, y as i64, ctx.tile);
        ((dx * dx + dy * dy).sqrt() * k).min(1.0)
    })])
}

/// The mask input (inside where above 0.5).
fn mask(ctx: &Ctx) -> Image {
    ctx.gray_or(0, 0.0)
}

fn outer_glow(ctx: &Ctx) -> CookResult {
    let m = mask(ctx);
    let d2 = ops::distance_sq(ctx.w, ctx.h, |i| m.data.get(i).is_some_and(|v| *v > 0.5), ctx.tile);
    let (size, amt, fall) = (px(ctx, ctx.f("size")).max(1e-3), ctx.f("intensity"), ctx.f("falloff").max(0.01));
    let v = d2.iter().zip(m.data.iter()).map(|(d, mv)| mv.max((1.0 - d.sqrt() / size).max(0.0).powf(fall) * amt)).collect();
    Ok(vec![ops::gray_from(ctx.w, ctx.h, v)])
}

fn inner_shadow(ctx: &Ctx) -> CookResult {
    let m = mask(ctx);
    let (ox, oy) = offset(ctx, ctx.f("angle"), ctx.f("distance"));
    let (w, tile) = (ctx.w as usize, ctx.tile);
    // Distance to the nearest pixel outside the mask shifted toward the shadow.
    let d2 = ops::distance_sq(ctx.w, ctx.h, |i| m.get_gray((i % w) as i64 - ox, (i / w) as i64 - oy, tile) <= 0.5, tile);
    let (size, op) = (px(ctx, ctx.f("size")).max(1e-3), ctx.f("opacity"));
    let v = d2.iter().zip(m.data.iter()).map(|(d, mv)| mv * (1.0 - op * (1.0 - d.sqrt() / size).clamp(0.0, 1.0))).collect();
    Ok(vec![ops::gray_from(ctx.w, ctx.h, v)])
}

fn drop_shadow(ctx: &Ctx) -> CookResult {
    let m = mask(ctx);
    let (ox, oy) = offset(ctx, ctx.f("angle"), ctx.f("distance"));
    let shifted = Image::gray(ctx.w, ctx.h, |x, y| m.get_gray(x as i64 - ox, y as i64 - oy, ctx.tile));
    let op = ctx.f("opacity");
    let shadow = ops::gaussian(&shifted, px(ctx, ctx.f("size")), ctx.tile).map(Kind::Gray, |p| [p[0] * op, 0.0, 0.0, 0.0]);
    let out = Image::gray(ctx.w, ctx.h, |x, y| m.get_gray(x as i64, y as i64, false).max(shadow.get_gray(x as i64, y as i64, false)));
    Ok(vec![out, shadow])
}

fn height_curvature(ctx: &Ctx) -> CookResult {
    let h = ctx.gray_or(0, 0.0);
    let r = px(ctx, ctx.f("radius"));
    // Difference of Gaussians: a band-limited Laplacian that ignores pixel noise.
    let (fine, wide) = (ops::gaussian(&h, r * 0.5, ctx.tile), ops::gaussian(&h, r * 2.0, ctx.tile));
    let k = ctx.f("intensity") * 8.0;
    let v = fine.data.iter().zip(wide.data.iter()).map(|(a, b)| (0.5 + (a - b) * k).clamp(0.0, 1.0)).collect();
    Ok(vec![ops::gray_from(ctx.w, ctx.h, v)])
}

fn outline(ctx: &Ctx) -> CookResult {
    let m = mask(ctx);
    let inside = |i: usize| m.data.get(i).is_some_and(|v| *v > 0.5);
    let to_in = ops::distance_sq(ctx.w, ctx.h, inside, ctx.tile);
    let to_out = ops::distance_sq(ctx.w, ctx.h, |i| !inside(i), ctx.tile);
    let w = px(ctx, ctx.f("width"));
    let (lo, hi) = match ctx.i("position") {
        0 => (0.0, w),
        2 => (-w, 0.0),
        _ => (-w * 0.5, w * 0.5),
    };
    // Signed distance to the edge (positive outside), half a pixel off each side of it.
    let v = (0..to_in.len())
        .map(|i| {
            let sd = if inside(i) { 0.5 - to_out[i].sqrt() } else { to_in[i].sqrt() - 0.5 };
            (sd - lo + 0.5).clamp(0.0, 1.0) * (hi - sd + 0.5).clamp(0.0, 1.0)
        })
        .collect();
    Ok(vec![ops::gray_from(ctx.w, ctx.h, v)])
}

fn flat(ctx: &Ctx, i: usize) -> Image {
    ctx.any_or(i, Kind::Color, [0.5, 0.5, 1.0, 1.0]).to_color()
}

fn normal_combine(ctx: &Ctx) -> CookResult {
    let (base, det) = (flat(ctx, 0), flat(ctx, 1));
    let k = ctx.f("intensity");
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let (b, d) = (unpack(base.get(x as i64, y as i64, false)), unpack(det.get(x as i64, y as i64, false)));
        // Whiteout blending: add the slopes, multiply the heights.
        pack([b[0] + d[0] * k, b[1] + d[1] * k, b[2] * (1.0 + (d[2] - 1.0) * k.min(1.0)).max(1e-3)], 1.0)
    })])
}

fn normal_rotate(ctx: &Ctx) -> CookResult {
    let n = flat(ctx, 0);
    let (s, c) = (ctx.f("angle") * TAU).sin_cos();
    Ok(vec![n.map(Kind::Color, |p| {
        let v = unpack(p);
        pack([v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]], p[3])
    })])
}

fn normal_sobel(ctx: &Ctx) -> CookResult {
    let h = ctx.gray_or(0, 0.0);
    let k = ctx.f("intensity") * ctx.w as f32 / 32.0;
    let dx_fmt = ctx.i("format") == 1;
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let (dx, dy) = sobel_slope(&h, x as i64, y as i64, ctx.tile);
        let ny = dy * k;
        pack([-dx * k, if dx_fmt { -ny } else { ny }, 1.0], 1.0)
    })])
}
