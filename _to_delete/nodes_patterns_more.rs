//! More Texture generators › Patterns: tilings (herringbone, basketweave, planks, scales,
//! triangles, octagons, tread plate), grids, shapes and gradients. Tile patterns share gap, bevel
//! and luminance-random parameters; every tiling repeats a whole number of times per image.

use std::f32::consts::{SQRT_2, TAU};

use crate::catalog::{B, CookResult, Ctx, NodeDef, node};
use crate::noise::rand2;
use crate::patterns::PATTERNS;

/// Rotate `(x, y)` by `turns`.
#[inline]
fn rot(x: f32, y: f32, turns: f32) -> (f32, f32) {
    let (s, c) = (turns * TAU).sin_cos();
    (x * c - y * s, x * s + y * c)
}

/// Coverage from a signed inside distance `e`: hard when `soft` is 0, else a ramp of width `soft`.
#[inline]
fn fill(e: f32, soft: f32) -> f32 {
    if soft <= 0.0 { (e >= 0.0) as i32 as f32 } else { (e / soft).clamp(0.0, 1.0) }
}

/// A beveled tile from the distance `e` to its edge: `gap` eats the edge, `bevel` ramps up.
#[inline]
fn tile_height(e: f32, t: &TileLook, rnd: f32) -> f32 {
    let e = e - t.gap * 0.5;
    if e <= 0.0 { 0.0 } else { (e / t.bevel).min(1.0) * (1.0 - t.lum_random * rnd) }
}

/// Soft bands around integer + 0.5 of `t`: `width` 0..1 of each period, `soft` edge.
#[inline]
fn band(t: f32, width: f32, soft: f32) -> f32 {
    let d = (t.rem_euclid(1.0) - 0.5).abs() * 2.0;
    ((width - d) / soft.max(1e-3) + 0.5).clamp(0.0, 1.0)
}

/// Local coordinates -1..1 in an `n`×`n` repetition.
#[inline]
fn tiled(u: f32, v: f32, n: f32) -> (f32, f32) {
    ((u * n).rem_euclid(1.0) * 2.0 - 1.0, (v * n).rem_euclid(1.0) * 2.0 - 1.0)
}

struct TileLook {
    gap: f32,
    bevel: f32,
    lum_random: f32,
    seed: u32,
}

fn look(ctx: &Ctx) -> TileLook {
    TileLook { gap: ctx.f("gap"), bevel: ctx.f("bevel").max(1e-4), lum_random: ctx.f("lum_random"), seed: ctx.seed }
}

/// The gap, bevel and luminance-random parameters of tile patterns.
fn with_look(b: B, gap: f32, bevel: f32) -> B {
    b.float("gap", "Gap", gap, 0.0, 0.5).float("bevel", "Bevel", bevel, 0.0, 1.0).float("lum_random", "Luminance random", 0.3, 0.0, 1.0)
}

fn n(ctx: &Ctx, id: &str) -> f32 {
    ctx.i(id).max(1) as f32
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let p = |id, label| node(id, label, PATTERNS).generator();
    vec![
        p("grid_lines", "Grid Lines")
            .desc("A grid of lines.")
            .int("tiling", "Tiling", 8, 1, 128)
            .float("width", "Line width", 0.06, 0.0, 1.0)
            .float("softness", "Softness", 0.0, 0.0, 0.5)
            .gray_out()
            .cook(grid_lines),
        p("dots_grid", "Dots Grid")
            .desc("Round dots on a grid (perforated metal, polka dots).")
            .int("tiling", "Tiling", 8, 1, 128)
            .float("radius", "Diameter", 0.6, 0.0, 1.0)
            .float("softness", "Softness", 0.0, 0.0, 0.5)
            .boolean("staggered", "Staggered rows", true)
            .gray_out()
            .cook(dots_grid),
        with_look(p("diamond_grid", "Diamond Grid").desc("Beveled diamond (45°) tiles.").int("tiling", "Tiling", 4, 1, 64), 0.05, 0.15)
            .gray_out()
            .cook(diamond_grid),
        with_look(
            p("herringbone", "Herringbone").desc("Planks laid in a herringbone pattern.").int("tiling", "Tiling", 2, 1, 32).int(
                "length",
                "Plank length",
                3,
                2,
                8,
            ),
            0.08,
            0.2,
        )
        .gray_out()
        .cook(herringbone),
        with_look(
            p("basketweave", "Basketweave").desc("Squares of parallel planks alternating direction.").int("tiling", "Tiling", 4, 1, 64).int(
                "strips",
                "Planks per square",
                3,
                1,
                16,
            ),
            0.08,
            0.2,
        )
        .gray_out()
        .cook(basketweave),
        p("chevron", "Chevron")
            .desc("Zigzag bands.")
            .int("count", "Bands", 8, 1, 128)
            .int("zigs", "Zigzags", 4, 1, 64)
            .float("height", "Zig height", 1.0, 0.0, 8.0)
            .float("width", "Width", 0.5, 0.0, 1.0)
            .float("softness", "Softness", 0.1, 0.0, 1.0)
            .gray_out()
            .cook(chevron),
        p("fish_scales", "Fish Scales")
            .desc("Overlapping rows of round scales (height).")
            .int("tiling", "Tiling", 6, 1, 64)
            .float("radius", "Radius", 0.65, 0.5, 1.0)
            .float("lum_random", "Luminance random", 0.2, 0.0, 1.0)
            .gray_out()
            .cook(fish_scales),
        with_look(p("triangle_grid", "Triangle Grid").desc("Beveled triangle tiles.").int("tiling", "Tiling", 6, 1, 64), 0.04, 0.12)
            .gray_out()
            .cook(triangle_grid),
        p("spiral", "Spiral")
            .desc("An Archimedean spiral.")
            .float("turns", "Turns", 6.0, 0.0, 64.0)
            .float("width", "Width", 0.5, 0.0, 1.0)
            .float("softness", "Softness", 0.3, 0.0, 1.0)
            .int("tiling", "Tiling", 1, 1, 16)
            .gray_out()
            .cook(spiral),
        p("concentric_rings", "Concentric Rings")
            .desc("Rings around the center.")
            .float("count", "Rings", 8.0, 0.0, 64.0)
            .float("width", "Width", 0.5, 0.0, 1.0)
            .float("softness", "Softness", 0.3, 0.0, 1.0)
            .int("tiling", "Tiling", 1, 1, 16)
            .gray_out()
            .cook(concentric_rings),
        p("gradient_axial", "Gradient Axial")
            .desc("A ramp from point 1 (black) to point 2 (white).")
            .float2("point_1", "Point 1", [0.2, 0.5], 0.0, 1.0)
            .float2("point_2", "Point 2", [0.8, 0.5], 0.0, 1.0)
            .gray_out()
            .cook(gradient_axial),
        p("gradient_circular", "Gradient Circular")
            .desc("A ramp around the center (by angle).")
            .angle("angle", "Start angle", 0.0)
            .int("tiling", "Tiling", 1, 1, 16)
            .gray_out()
            .cook(gradient_circular),
        p("cross", "Cross")
            .desc("A plus-shaped cross.")
            .float("length", "Arm length", 0.8, 0.0, 1.0)
            .float("thickness", "Thickness", 0.25, 0.0, 1.0)
            .float("softness", "Softness", 0.0, 0.0, 1.0)
            .angle("angle", "Angle", 0.0)
            .int("tiling", "Tiling", 1, 1, 32)
            .gray_out()
            .cook(cross),
        p("rounded_rectangle", "Rounded Rectangle")
            .desc("A rectangle with rounded corners.")
            .float2("size", "Size", [0.8, 0.6], 0.0, 1.0)
            .float("radius", "Corner radius", 0.2, 0.0, 1.0)
            .float("softness", "Softness", 0.0, 0.0, 1.0)
            .angle("angle", "Angle", 0.0)
            .int("tiling", "Tiling", 1, 1, 32)
            .gray_out()
            .cook(rounded_rectangle),
        p("ring", "Ring")
            .desc("A circular ring.")
            .float("radius", "Radius", 0.6, 0.0, 1.0)
            .float("width", "Width", 0.15, 0.0, 1.0)
            .float("softness", "Softness", 0.05, 0.0, 1.0)
            .int("tiling", "Tiling", 1, 1, 32)
            .gray_out()
            .cook(ring),
        with_look(
            p("arc_pavement", "Arc Pavement")
                .desc("Cobblestones laid in overlapping fan-shaped arcs.")
                .int("tiling", "Tiling", 3, 1, 32)
                .int("bands", "Rows per arc", 4, 1, 16)
                .int("stones", "Stones in outer row", 7, 1, 32),
            0.08,
            0.25,
        )
        .gray_out()
        .cook(arc_pavement),
        with_look(p("octagon_tiles", "Octagon Tiles").desc("Octagons with small square tiles between them.").int("tiling", "Tiling", 4, 1, 64), 0.03, 0.08)
            .gray_out()
            .cook(octagon_tiles),
        with_look(
            p("planks", "Planks")
                .desc("Rows of floor boards of random length, staggered.")
                .int("rows", "Rows", 8, 1, 64)
                .float("length", "Plank length", 0.33, 0.02, 1.0)
                .float("length_random", "Length random", 0.5, 0.0, 1.0),
            0.06,
            0.15,
        )
        .gray_out()
        .cook(planks),
        p("tread_plate", "Tread Plate")
            .desc("Raised diagonal bars alternating direction, like metal floor plates (height).")
            .int("tiling", "Tiling", 6, 1, 64)
            .float("length", "Bar length", 1.0, 0.0, 1.4)
            .float("width", "Bar width", 0.3, 0.01, 1.0)
            .gray_out()
            .cook(tread_plate),
    ]
}

fn grid_lines(ctx: &Ctx) -> CookResult {
    let (t, w, soft) = (n(ctx, "tiling"), ctx.f("width"), ctx.f("softness"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let d = |a: f32| {
            let f = (a * t).rem_euclid(1.0);
            f.min(1.0 - f)
        };
        fill(w * 0.5 - d(u).min(d(v)), soft)
    })])
}

fn dots_grid(ctx: &Ctx) -> CookResult {
    let (t, r, soft, stag) = (n(ctx, "tiling"), ctx.f("radius"), ctx.f("softness"), ctx.b("staggered"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = (v * t).floor();
        // A parity shift (not row × offset) keeps the pattern seamless for any row count.
        let shift = if stag && (row as i32) & 1 == 1 { 0.5 } else { 0.0 };
        let (x, y) = ((u * t + shift).rem_euclid(1.0) - 0.5, (v * t).rem_euclid(1.0) - 0.5);
        fill(r * 0.5 - (x * x + y * y).sqrt(), soft * 0.5)
    })])
}

fn diamond_grid(ctx: &Ctx) -> CookResult {
    let (t, l) = (n(ctx, "tiling"), look(ctx));
    let ti = t as i32;
    Ok(vec![ctx.gen_gray(|u, v| {
        let (a, b) = ((u + v) * t, (u - v) * t);
        let (fa, fb) = (a.rem_euclid(1.0), b.rem_euclid(1.0));
        let e = fa.min(1.0 - fa).min(fb).min(1.0 - fb);
        // Canonical diamond index: (A, B) ~ (A + t, B ± t).
        let (ia, ib) = (a.floor() as i32, b.floor() as i32);
        let k = ib.div_euclid(ti);
        tile_height(e, &l, rand2((ia - k * ti).rem_euclid(2 * ti), ib - k * ti, l.seed))
    })])
}

fn herringbone(ctx: &Ctx) -> CookResult {
    let (t, len, lk) = (n(ctx, "tiling") as i32, ctx.i("length").clamp(2, 8), look(ctx));
    let lf = len as f32;
    let span = (t * 2 * len) as f32;
    // 1 × L planks: a horizontal plank [0, L)×[0, 1) and a vertical one [L, L+1)×[1-L, 1) repeat
    // on the lattice m·(1, 1) + n·(L, -L), which has period 2L on both axes.
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = (u * span, v * span);
        let (m0, n0) = (((x + y) * 0.5).floor() as i32, ((x - y) / (2.0 * lf)).floor() as i32);
        for dm in -(len / 2 + 2)..=1 {
            for dn in -2..=1 {
                let (m, nn) = (m0 + dm, n0 + dn);
                let (qx, qy) = (x - (m + nn * len) as f32, y - (m - nn * len) as f32);
                let hit = if (0.0..lf).contains(&qx) && (0.0..1.0).contains(&qy) {
                    Some((qx.min(lf - qx).min(qy).min(1.0 - qy), 0))
                } else if (lf..lf + 1.0).contains(&qx) && (1.0 - lf..1.0).contains(&qy) {
                    Some(((qx - lf).min(lf + 1.0 - qx).min(qy - 1.0 + lf).min(1.0 - qy), 1))
                } else {
                    None
                };
                if let Some((e, kind)) = hit {
                    let k = nn.div_euclid(t);
                    let (cm, cn) = ((m - k * len * t).rem_euclid(2 * len * t), nn - k * t);
                    return tile_height(e, &lk, rand2(cm * 2 + kind, cn, lk.seed));
                }
            }
        }
        0.0
    })])
}

fn basketweave(ctx: &Ctx) -> CookResult {
    let (t, strips, l) = (n(ctx, "tiling"), n(ctx, "strips"), look(ctx));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (cx, cy) = ((u * t).floor() as i32, (v * t).floor() as i32);
        let (fx, fy) = ((u * t).rem_euclid(1.0), (v * t).rem_euclid(1.0));
        let horizontal = (cx + cy) & 1 == 0;
        let (across, along) = if horizontal { (fy * strips, fx * strips) } else { (fx * strips, fy * strips) };
        let fa = across.rem_euclid(1.0);
        let e = fa.min(1.0 - fa).min(along).min(strips - along);
        let id = (cy.rem_euclid(t as i32) * t as i32 + cx.rem_euclid(t as i32)) * 16 + across.floor() as i32;
        tile_height(e, &l, rand2(id, 0, l.seed))
    })])
}

fn chevron(ctx: &Ctx) -> CookResult {
    let (c, z, hgt, w, soft) = (n(ctx, "count"), n(ctx, "zigs"), ctx.f("height"), ctx.f("width"), ctx.f("softness"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let zig = ((u * z).rem_euclid(1.0) - 0.5).abs() * 2.0;
        band(v * c + zig * hgt, w, soft)
    })])
}

/// The topmost scale over `(x, y)` (in cells) of a staggered scale layout: rows 0.5 apart, odd
/// rows shifted by half a cell, earlier rows on top. Returns (column, row, dx, dy, d / r).
fn scale_hit(x: f32, y: f32, r: f32) -> Option<(i32, i32, f32, f32, f32)> {
    let j0 = ((y - r) * 2.0).floor() as i32;
    let j1 = ((y + r) * 2.0).ceil() as i32;
    for j in j0..=j1 {
        let shift = if j & 1 == 1 { 0.5 } else { 0.0 };
        let i = (x - shift).round() as i32;
        let (dx, dy) = (x - (i as f32 + shift), y - j as f32 * 0.5);
        let d = (dx * dx + dy * dy).sqrt();
        if d < r {
            return Some((i, j, dx, dy, d / r));
        }
    }
    None
}

fn fish_scales(ctx: &Ctx) -> CookResult {
    let (t, r, lr, s) = (n(ctx, "tiling"), ctx.f("radius").clamp(0.5, 1.0), ctx.f("lum_random"), ctx.seed);
    let ti = t as i32;
    Ok(vec![ctx.gen_gray(|u, v| {
        let Some((i, j, _, _, d)) = scale_hit(u * t, v * t, r) else {
            return 0.0;
        };
        (1.0 - d * d).sqrt() * (1.0 - lr * rand2(i.rem_euclid(ti), j.rem_euclid(2 * ti), s))
    })])
}

fn triangle_grid(ctx: &Ctx) -> CookResult {
    let (t, l) = (n(ctx, "tiling"), look(ctx));
    // Rows of near-equilateral triangles; an even row count keeps the half-cell row shift seamless.
    let rows = (2.0 * (t * 0.577).round()).max(2.0);
    let (ti, ri) = (t as i32, rows as i32);
    let k = 2.0 / 5f32.sqrt();
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = (v * rows).floor() as i32;
        let fy = (v * rows).rem_euclid(1.0);
        let x = u * t + if row & 1 == 1 { 0.5 } else { 0.0 };
        let col = x.floor() as i32;
        let dx = x.rem_euclid(1.0) - 0.5;
        let (e, id) = if dx.abs() <= fy * 0.5 {
            ((1.0 - fy).min((fy * 0.5 - dx.abs()) * k), col * 2)
        } else {
            // A down-pointing triangle centered on the nearer whole column.
            let (c, ddx) = if dx < 0.0 { (col, dx + 0.5) } else { (col + 1, dx - 0.5) };
            (fy.min(((1.0 - fy) * 0.5 - ddx.abs()) * k), c * 2 + 1)
        };
        tile_height(e, &l, rand2(id.rem_euclid(2 * ti), row.rem_euclid(ri), l.seed))
    })])
}

fn spiral(ctx: &Ctx) -> CookResult {
    let (turns, w, soft, t) = (ctx.f("turns"), ctx.f("width"), ctx.f("softness"), n(ctx, "tiling"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        let r = (x * x + y * y).sqrt();
        if r > 1.0 {
            return 0.0;
        }
        band(r * turns - y.atan2(x) / TAU, w, soft)
    })])
}

fn concentric_rings(ctx: &Ctx) -> CookResult {
    let (c, w, soft, t) = (ctx.f("count"), ctx.f("width"), ctx.f("softness"), n(ctx, "tiling"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        band((x * x + y * y).sqrt() * c, w, soft)
    })])
}

fn gradient_axial(ctx: &Ctx) -> CookResult {
    let (a, b) = (ctx.v2("point_1"), ctx.v2("point_2"));
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = (dx * dx + dy * dy).max(1e-8);
    Ok(vec![ctx.gen_gray(|u, v| (((u - a[0]) * dx + (v - a[1]) * dy) / len2).clamp(0.0, 1.0))])
}

fn gradient_circular(ctx: &Ctx) -> CookResult {
    let (a, t) = (ctx.f("angle"), n(ctx, "tiling"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        ((-y).atan2(x) / TAU - a).rem_euclid(1.0)
    })])
}

fn cross(ctx: &Ctx) -> CookResult {
    let (len, th, soft, a, t) = (ctx.f("length"), ctx.f("thickness") * 0.5, ctx.f("softness"), ctx.f("angle"), n(ctx, "tiling"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        let (x, y) = rot(x, y, a);
        let arm = |p: f32, q: f32| (len - p.abs()).min(th - q.abs());
        fill(arm(x, y).max(arm(y, x)), soft)
    })])
}

fn rounded_rectangle(ctx: &Ctx) -> CookResult {
    let (sz, rad, soft, a, t) = (ctx.v2("size"), ctx.f("radius"), ctx.f("softness"), ctx.f("angle"), n(ctx, "tiling"));
    let r = rad.min(sz[0]).min(sz[1]).max(0.0);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        let (x, y) = rot(x, y, a);
        // Signed distance to a rounded box (negative inside).
        let (qx, qy) = (x.abs() - sz[0] + r, y.abs() - sz[1] + r);
        let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
        fill(-d, soft)
    })])
}

fn ring(ctx: &Ctx) -> CookResult {
    let (rad, w, soft, t) = (ctx.f("radius"), ctx.f("width") * 0.5, ctx.f("softness"), n(ctx, "tiling"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let (x, y) = tiled(u, v, t);
        fill(w - ((x * x + y * y).sqrt() - rad).abs(), soft)
    })])
}

fn arc_pavement(ctx: &Ctx) -> CookResult {
    let (t, bands, stones, l) = (n(ctx, "tiling"), n(ctx, "bands"), n(ctx, "stones"), look(ctx));
    let ti = t as i32;
    let r = 0.75;
    Ok(vec![ctx.gen_gray(|u, v| {
        let Some((i, j, dx, dy, rr)) = scale_hit(u * t, v * t, r) else {
            return 0.0;
        };
        let band_f = rr * bands;
        let k = band_f.floor();
        let count = (stones * (k + 0.5) / bands).round().max(1.0);
        let a = (dy.atan2(dx) / TAU + 0.5) * count;
        let fr = band_f.rem_euclid(1.0);
        let fa = a.rem_euclid(1.0);
        // Distances in band-width units: radial, and along the arc at this radius.
        let e = fr.min(1.0 - fr).min(fa.min(1.0 - fa) * TAU * band_f.max(0.5) / count);
        let id = ((j.rem_euclid(2 * ti) * ti + i.rem_euclid(ti)) * 32 + k as i32) * 64 + a.floor() as i32;
        tile_height(e, &l, rand2(id, 7, l.seed))
    })])
}

fn octagon_tiles(ctx: &Ctx) -> CookResult {
    let (t, l) = (n(ctx, "tiling"), look(ctx));
    let ti = t as i32;
    // A regular octagon in a unit cell: its corner cuts lie on |x| + |y| = 0.5 + side / 2.
    let cut = 0.5 + 0.5 / (1.0 + SQRT_2);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (cx, cy) = ((u * t).floor() as i32, (v * t).floor() as i32);
        let (x, y) = ((u * t).rem_euclid(1.0) - 0.5, (v * t).rem_euclid(1.0) - 0.5);
        let (ax, ay) = (x.abs(), y.abs());
        if ax + ay <= cut {
            let e = (0.5 - ax.max(ay)).min((cut - ax - ay) / SQRT_2);
            tile_height(e, &l, rand2(cx.rem_euclid(ti), cy.rem_euclid(ti), l.seed))
        } else {
            // The small square around the nearest cell corner.
            let e = (ax + ay - cut) / SQRT_2;
            let (kx, ky) = (cx + (x > 0.0) as i32, cy + (y > 0.0) as i32);
            tile_height(e, &l, rand2(kx.rem_euclid(ti), ky.rem_euclid(ti), l.seed ^ 0x6d))
        }
    })])
}

fn planks(ctx: &Ctx) -> CookResult {
    let (rows, l) = (n(ctx, "rows"), look(ctx));
    let count = (1.0 / ctx.f("length").max(0.02)).round().clamp(1.0, 64.0);
    let (ci, ri) = (count as i32, rows as i32);
    let jit = ctx.f("length_random").clamp(0.0, 1.0) * 0.4;
    let s = ctx.seed;
    Ok(vec![ctx.gen_gray(|u, v| {
        let row = ((v * rows).floor() as i32).rem_euclid(ri);
        let fy = (v * rows).rem_euclid(1.0);
        let t = u * count + rand2(row, 0, s ^ 0x11) * count;
        // Plank ends: one per whole step of `t`, jittered; indices wrap so each row tiles.
        let cut = |j: i32| j as f32 + (rand2(j.rem_euclid(ci), row, s) - 0.5) * 2.0 * jit;
        let j = t.floor() as i32;
        let j = if t < cut(j) {
            j - 1
        } else if t >= cut(j + 1) {
            j + 1
        } else {
            j
        };
        let along = (t - cut(j)).min(cut(j + 1) - t) / count * rows;
        let e = along.min(fy).min(1.0 - fy);
        tile_height(e, &l, rand2(j.rem_euclid(ci), row, s ^ 0x22))
    })])
}

fn tread_plate(ctx: &Ctx) -> CookResult {
    let (t, len, w) = (n(ctx, "tiling"), ctx.f("length") * 0.5, ctx.f("width").max(0.01) * 0.5);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (cx, cy) = ((u * t).floor() as i32, (v * t).floor() as i32);
        let (x, y) = ((u * t).rem_euclid(1.0) - 0.5, (v * t).rem_euclid(1.0) - 0.5);
        let turn = if (cx + cy) & 1 == 0 { 0.125 } else { -0.125 };
        let (x, y) = rot(x, y, turn);
        // An elongated lens with a flat top and rounded flanks.
        let d = ((x / len.max(1e-3)).powi(2) + (y / w).powi(2)).sqrt();
        (1.0 - d.powi(4)).max(0.0).sqrt()
    })])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{Ctx, find, resolve_params};

    #[test]
    fn tilings_wrap_seamlessly() {
        for id in ["herringbone", "planks", "triangle_grid", "fish_scales", "arc_pavement", "diamond_grid", "octagon_tiles"] {
            let def = find(id).unwrap();
            let params = resolve_params(def, &BTreeMap::new());
            let no_res = |_: &str| None;
            let vars = BTreeMap::new();
            let ctx = Ctx { w: 128, h: 128, seed: 5, tile: true, inputs: &[], params: &params, resource: &no_res, vars: &vars };
            let img = &(def.cook)(&ctx).unwrap()[0];
            // Sampling the pattern one period further along either axis gives the same image: the
            // last column/row continues into the first like any interior neighbour.
            let (mut seam, mut inner) = (0.0f32, 0.0f32);
            for k in 0..128 {
                seam += (img.get(127, k, false)[0] - img.get(0, k, false)[0]).abs() + (img.get(k, 127, false)[0] - img.get(k, 0, false)[0]).abs();
                inner += (img.get(63, k, false)[0] - img.get(64, k, false)[0]).abs() + (img.get(k, 63, false)[0] - img.get(k, 64, false)[0]).abs();
            }
            assert!(seam <= inner * 3.0 + 1.0, "{id}: seam {seam} vs inner {inner}");
            assert!(img.mean()[0] > 0.05, "{id} is empty");
        }
    }
}
