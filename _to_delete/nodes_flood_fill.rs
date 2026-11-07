//! Filters › Flood fill: labels the connected white regions of a tile mask and turns each region
//! into per-region data (random values, colors, gradients, sizes, positions).
//!
//! The index map that [`flood_fill`] writes holds, per region pixel: R, G = the region's
//! bounding-box center (UV), B = a random id, A = 1. Gaps are all zero. Downstream nodes re-label
//! the index map (pixels of one region share the same RGB), so they recover the exact bounding
//! box, and a pixel's place inside it, at any size. Labeling wraps across the image edges when
//! the graph tiles, so a tile crossing an edge is one region.

use std::f32::consts::TAU;

use materialcraft_image::{Image, Kind};

use crate::catalog::{CookResult, Ctx, NodeDef, PortKind, node};
use crate::noise::{hash3, rand1};

const FLOOD: &str = "Filters/Flood fill";
const NONE: u32 = u32::MAX;

/// Connected regions of an image.
pub struct Regions {
    /// Region index per pixel (`u32::MAX` = background).
    pub label: Vec<u32>,
    /// Unwrapped pixel coordinates (a region crossing an edge continues past it).
    pub pos: Vec<[i32; 2]>,
    /// Per region: min x, min y, max x, max y (unwrapped, inclusive).
    pub bbox: Vec<[i32; 4]>,
}

/// Label the 4-connected regions of pixels where `inside` holds and neighbours are `same`.
pub fn label(w: u32, h: u32, tile: bool, inside: impl Fn(usize) -> bool, same: impl Fn(usize, usize) -> bool) -> Regions {
    let (wi, hi) = (w as i32, h as i32);
    let n = w as usize * h as usize;
    let mut label = vec![NONE; n];
    let mut pos = vec![[0i32; 2]; n];
    let mut bbox = Vec::new();
    let mut queue: Vec<usize> = Vec::new();
    for start in 0..n {
        if label[start] != NONE || !inside(start) {
            continue;
        }
        let id = bbox.len() as u32;
        let s = [(start % w as usize) as i32, (start / w as usize) as i32];
        label[start] = id;
        pos[start] = s;
        let mut b = [s[0], s[1], s[0], s[1]];
        queue.clear();
        queue.push(start);
        let mut head = 0;
        while let Some(&i) = queue.get(head) {
            head += 1;
            let p = pos[i];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let q = [p[0] + dx, p[1] + dy];
                let (x, y) = if tile {
                    (q[0].rem_euclid(wi), q[1].rem_euclid(hi))
                } else if (0..wi).contains(&q[0]) && (0..hi).contains(&q[1]) {
                    (q[0], q[1])
                } else {
                    continue;
                };
                let j = (y * wi + x) as usize;
                if label[j] != NONE || !inside(j) || !same(i, j) {
                    continue;
                }
                label[j] = id;
                pos[j] = q;
                b = [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])];
                queue.push(j);
            }
        }
        bbox.push(b);
    }
    Regions { label, pos, bbox }
}

/// What a downstream node knows about the region under a pixel.
pub struct RegionPx {
    /// The region's random id (0..1).
    pub id: f32,
    /// Bounding-box center (UV, wrapped into 0..1).
    pub center: [f32; 2],
    /// Bounding-box size (UV).
    pub size: [f32; 2],
    /// The pixel's place inside the bounding box (0..1).
    pub local: [f32; 2],
}

/// Bounding-box center and size (UV) of a region.
fn bbox_uv(b: [i32; 4], w: u32, h: u32) -> ([f32; 2], [f32; 2]) {
    let (wf, hf) = (w as f32, h as f32);
    let (ex, ey) = ((b[2] - b[0] + 1) as f32, (b[3] - b[1] + 1) as f32);
    let center = [((b[0] + b[2] + 1) as f32 * 0.5 / wf).rem_euclid(1.0), ((b[1] + b[3] + 1) as f32 * 0.5 / hf).rem_euclid(1.0)];
    (center, [(ex / wf).min(1.0), (ey / hf).min(1.0)])
}

/// A uniform random value per region (and per `k`), from the region's id and the node's seed.
pub fn region_rand(id: f32, k: u32, seed: u32) -> f32 {
    rand1(id.to_bits() ^ k.wrapping_mul(0x9e37_79b9), seed)
}

/// The index map input at the output size (nearest sampling keeps region values exact).
fn index_input(ctx: &Ctx, i: usize) -> Image {
    match ctx.input(i) {
        Some(img) => Image::generate_uv(ctx.w, ctx.h, Kind::Color, |u, v| img.sample_nearest(u, v, false)),
        None => Image::filled(ctx.w, ctx.h, Kind::Color, [0.0; 4]),
    }
}

/// Run `f` for every region pixel of index map input `i`; background pixels get `bg`.
pub fn per_region(ctx: &Ctx, i: usize, kind: Kind, bg: [f32; 4], f: impl Fn(&RegionPx, f32, f32) -> [f32; 4] + Sync) -> Image {
    let idx = index_input(ctx, i).to_color();
    let d = idx.data.as_slice();
    let px = |i: usize| d.get(i * 4..i * 4 + 4).unwrap_or(&[0.0; 4]);
    let regions = label(ctx.w, ctx.h, ctx.tile, |i| px(i)[3] > 0.5, |a, b| px(a)[..3] == px(b)[..3]);
    let (w, h) = (ctx.w, ctx.h);
    Image::generate(w, h, kind, |x, y| {
        let i = (y * w + x) as usize;
        let Some(b) = regions.label.get(i).and_then(|l| regions.bbox.get(*l as usize)) else {
            return bg;
        };
        let (center, size) = bbox_uv(*b, w, h);
        let p = regions.pos[i];
        let local = [(p[0] - b[0]) as f32 + 0.5, (p[1] - b[1]) as f32 + 0.5];
        let local = [local[0] / (b[2] - b[0] + 1) as f32, local[1] / (b[3] - b[1] + 1) as f32];
        let r = RegionPx { id: px(i)[2], center, size, local };
        f(&r, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
    })
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let index_in = |id, label| node(id, label, FLOOD).color_in("input", "Index map");
    vec![
        node("flood_fill", "Flood Fill", FLOOD)
            .desc("Finds every connected white region of a tile mask (regions crossing the edges are one) and writes an index map: R, G = region center, B = random id, A = 1.")
            .gray_in("input", "Mask")
            .float("threshold", "Threshold", 0.5, 0.0, 1.0)
            .output("output", "Index map", PortKind::Color)
            .cook(flood_fill),
        index_in("flood_fill_to_random_grayscale", "Flood Fill to Random Grayscale")
            .desc("A random gray per region.")
            .float("min", "Min", 0.0, 0.0, 1.0)
            .float("max", "Max", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(to_random_gray),
        index_in("flood_fill_to_random_color", "Flood Fill to Random Color")
            .desc("A random color per region.")
            .float("saturation", "Saturation", 0.7, 0.0, 1.0)
            .float("lightness", "Lightness", 0.5, 0.0, 1.0)
            .float("lightness_random", "Lightness random", 0.3, 0.0, 1.0)
            .color_out()
            .cook(to_random_color),
        index_in("flood_fill_to_gradient", "Flood Fill to Gradient")
            .desc("A linear gradient across each region's bounding box.")
            .angle("angle", "Angle", 0.0)
            .float("angle_random", "Angle random", 0.0, 0.0, 1.0)
            .float("slope_random", "Slope random", 0.0, 0.0, 1.0)
            .gray_out()
            .cook(to_gradient),
        index_in("flood_fill_to_bbox_size", "Flood Fill to BBox Size")
            .desc("Each region's bounding-box size.")
            .choice("mode", "Size", &["Width", "Height", "Max", "Min", "Area"], 2)
            .gray_out()
            .cook(to_bbox_size),
        index_in("flood_fill_to_position", "Flood Fill to Position")
            .desc("Each region's center position (R = X, G = Y).")
            .color_out()
            .cook(to_position),
        index_in("flood_fill_to_grayscale", "Flood Fill to Grayscale")
            .desc("Each region takes the value of the input at its center.")
            .gray_in("source", "Source")
            .gray_out()
            .cook(to_grayscale),
        index_in("flood_fill_to_color", "Flood Fill to Color")
            .desc("Each region takes the color of the input at its center.")
            .color_in("source", "Source")
            .color_out()
            .cook(to_color),
    ]
}

fn flood_fill(ctx: &Ctx) -> CookResult {
    let t = ctx.f("threshold");
    let m = ctx.gray_or(0, 0.0);
    let d = m.data.as_slice();
    let regions = label(ctx.w, ctx.h, ctx.tile, |i| d.get(i).is_some_and(|v| *v > t), |_, _| true);
    let (w, h, s) = (ctx.w, ctx.h, ctx.seed);
    let info: Vec<[f32; 4]> = regions
        .bbox
        .iter()
        .map(|b| {
            let (c, size) = bbox_uv(*b, w, h);
            // Stable per region: from its center and size, not its scan order.
            let k = hash3((c[0] * 65536.0) as i32, (c[1] * 65536.0) as i32, s ^ (((size[0] * 4096.0) as u32) << 12) ^ ((size[1] * 4096.0) as u32));
            [c[0], c[1], ((k >> 8) as f32 / (1u32 << 24) as f32).max(1e-4), 1.0]
        })
        .collect();
    Ok(vec![Image::generate(w, h, Kind::Color, |x, y| {
        regions.label.get((y * w + x) as usize).and_then(|l| info.get(*l as usize)).copied().unwrap_or([0.0; 4])
    })])
}

fn to_random_gray(ctx: &Ctx) -> CookResult {
    let (lo, hi, s) = (ctx.f("min"), ctx.f("max"), ctx.seed);
    Ok(vec![per_region(ctx, 0, Kind::Gray, [0.0; 4], |r, _, _| [lo + (hi - lo) * region_rand(r.id, 0, s), 0.0, 0.0, 0.0])])
}

fn to_random_color(ctx: &Ctx) -> CookResult {
    let (sat, l, lr, s) = (ctx.f("saturation"), ctx.f("lightness"), ctx.f("lightness_random"), ctx.seed);
    Ok(vec![per_region(ctx, 0, Kind::Color, [0.0, 0.0, 0.0, 1.0], |r, _, _| {
        let light = (l + (region_rand(r.id, 2, s) - 0.5) * lr).clamp(0.0, 1.0);
        let c = crate::ops::hsl_to_rgb(region_rand(r.id, 1, s), sat, light);
        [c[0], c[1], c[2], 1.0]
    })])
}

fn to_gradient(ctx: &Ctx) -> CookResult {
    let (a, ar, sr, s) = (ctx.f("angle"), ctx.f("angle_random"), ctx.f("slope_random"), ctx.seed);
    Ok(vec![per_region(ctx, 0, Kind::Gray, [0.0; 4], |r, _, _| {
        let (sn, cs) = ((a + (region_rand(r.id, 3, s) - 0.5) * ar) * TAU).sin_cos();
        // Projection onto the direction, normalized so the gradient spans 0..1 over the box.
        let (x, y) = (r.local[0] - 0.5, r.local[1] - 0.5);
        let t = 0.5 + (x * cs + y * sn) / (cs.abs() + sn.abs()).max(1e-6);
        let slope = 1.0 - sr * region_rand(r.id, 4, s);
        [(t * slope).clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}

fn to_bbox_size(ctx: &Ctx) -> CookResult {
    let mode = ctx.i("mode");
    Ok(vec![per_region(ctx, 0, Kind::Gray, [0.0; 4], |r, _, _| {
        let [w, h] = r.size;
        let v = match mode {
            0 => w,
            1 => h,
            3 => w.min(h),
            4 => w * h,
            _ => w.max(h),
        };
        [v, 0.0, 0.0, 0.0]
    })])
}

fn to_position(ctx: &Ctx) -> CookResult {
    Ok(vec![per_region(ctx, 0, Kind::Color, [0.0, 0.0, 0.0, 1.0], |r, _, _| [r.center[0], r.center[1], 0.0, 1.0])])
}

fn sample_at_center(ctx: &Ctx, kind: Kind) -> Image {
    let src = ctx.input(1).map(|i| i.to_kind(kind));
    let tile = ctx.tile;
    per_region(ctx, 0, kind, [0.0, 0.0, 0.0, 1.0], |r, _, _| src.as_ref().map_or([1.0; 4], |img| img.sample(r.center[0], r.center[1], tile)))
}

fn to_grayscale(ctx: &Ctx) -> CookResult {
    Ok(vec![sample_at_center(ctx, Kind::Gray)])
}

fn to_color(ctx: &Ctx) -> CookResult {
    Ok(vec![sample_at_center(ctx, Kind::Color)])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{find, resolve_params};

    fn cook(id: &str, inputs: &[Option<Image>], w: u32) -> Vec<Image> {
        let def = find(id).unwrap();
        let params = resolve_params(def, &BTreeMap::new());
        let no_res = |_: &str| None;
        let vars = BTreeMap::new();
        let ctx = Ctx { w, h: w, seed: 3, tile: true, inputs, params: &params, resource: &no_res, vars: &vars };
        (def.cook)(&ctx).unwrap()
    }

    #[test]
    fn separate_squares_differ_and_edge_square_wraps() {
        // Square A at 2..6, square B at 10..14, square C straddling the right edge (x 14..16 and 0..1)
        // in rows 10..13.
        let mask = Image::gray(16, 16, |x, y| {
            let a = (2..6).contains(&x) && (2..6).contains(&y);
            let b = (9..13).contains(&x) && (2..6).contains(&y);
            let c = !(1..14).contains(&x) && (10..13).contains(&y);
            (a || b || c) as i32 as f32
        });
        let idx = &cook("flood_fill", &[Some(mask)], 16)[0];
        let (a, b) = (idx.get(3, 3, false), idx.get(10, 3, false));
        assert_eq!(a[3], 1.0);
        assert_ne!(a[2], b[2], "separate squares get different ids");
        assert_eq!(idx.get(0, 0, false), [0.0; 4], "gaps stay empty");
        let (l, r) = (idx.get(0, 11, false), idx.get(15, 11, false));
        assert_eq!(l, r, "a square across the edge is one region");
        // Pixels 14, 15 and 0 (wrapped to 16): the box middle is 15.5 px.
        assert!((l[0] - 15.5 / 16.0).abs() < 1e-5, "{l:?}");
        let gray = &cook("flood_fill_to_random_grayscale", &[Some(idx.clone())], 16)[0];
        assert_eq!(gray.get(0, 11, false), gray.get(15, 11, false));
        assert_ne!(gray.get(3, 3, false), gray.get(10, 3, false));
        let size = &cook("flood_fill_to_bbox_size", &[Some(idx.clone())], 16)[0];
        assert!((size.get(15, 11, false)[0] - 3.0 / 16.0).abs() < 1e-5);
        let grad = &cook("flood_fill_to_gradient", &[Some(idx.clone())], 16)[0];
        assert!(grad.get(2, 3, false)[0] < grad.get(5, 3, false)[0]);
        assert!(grad.get(14, 11, false)[0] < grad.get(0, 11, false)[0], "the gradient continues across the edge");
    }
}
