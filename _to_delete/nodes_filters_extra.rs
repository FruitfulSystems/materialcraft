//! More filters: random per-tile transforms, horizon-based ambient occlusion, a 5×5 Sobel normal
//! map, value band selection, palette quantization and block-averaged pixelation.

use std::f32::consts::TAU;

use materialcraft_image::{Image, Kind, luminance};
use rayon::prelude::*;

use crate::catalog::{CookResult, Ctx, NodeDef, node};
use crate::filters_more::pack;
use crate::noise::rand2;
use crate::noises_more::smoothstep;

const ADJ: &str = "Filters/Adjustments";
const EFFECTS: &str = "Filters/Effects";
const NORMAL: &str = "Filters/Normal map";
const TILING: &str = "Filters/Tiling";

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("tile_random", "Tile Random", TILING)
            .desc("Cuts the image into a grid and shows a randomly offset, flipped and rotated window of the input in each tile (breaks up repetition).")
            .any_in("input", "Input")
            .int("x_amount", "X amount", 4, 1, 256)
            .int("y_amount", "Y amount", 4, 1, 256)
            .float("offset_random", "Offset random", 1.0, 0.0, 1.0)
            .boolean("rotate", "Random 90° rotation", true)
            .boolean("flip", "Random flip", true)
            .float("luminance_random", "Luminance random", 0.0, 0.0, 1.0)
            .cook(tile_random),
        node("ambient_occlusion_fast", "Ambient Occlusion (Fast)", EFFECTS)
            .desc("Horizon-based ambient occlusion of a height map: how much of the sky each pixel sees within the radius.")
            .gray_in("height", "Height")
            .float("radius", "Radius", 12.0, 1.0, 128.0)
            .float("height_depth", "Height depth", 0.3, 0.0, 1.0)
            .float("intensity", "Intensity", 1.0, 0.0, 2.0)
            .gray_out()
            .cook(ambient_occlusion_fast),
        node("normal_from_height_hq", "Normal from Height HQ", NORMAL)
            .desc("A smooth normal map from a height map with 5×5 Sobel slopes.")
            .gray_in("input", "Height")
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .choice("format", "Normal format", &["OpenGL", "DirectX"], 0)
            .color_out()
            .cook(normal_from_height_hq),
        node("histogram_select", "Histogram Select", ADJ)
            .desc("White where values lie in a band around the position, fading out with the contrast.")
            .gray_in("input", "Input")
            .float("position", "Position", 0.5, 0.0, 1.0)
            .float("range", "Range", 0.25, 0.0, 1.0)
            .float("contrast", "Contrast", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(histogram_select),
        node("quantize_color", "Quantize Color", ADJ)
            .desc("Reduces the image to a palette of its most representative colors (k-means), with optional ordered dithering.")
            .color_in("input", "Input")
            .int("colors", "Colors", 8, 2, 64)
            .float("dither", "Dither", 0.0, 0.0, 1.0)
            .color_out()
            .cook(quantize_color),
        node("pixelate_color", "Pixelate Color", EFFECTS)
            .desc("Averages the image over blocks (separate X and Y counts).")
            .any_in("input", "Input")
            .int("x_cells", "X cells", 32, 1, 1024)
            .int("y_cells", "Y cells", 32, 1, 1024)
            .cook(pixelate_color),
    ]
}

fn tile_random(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let (nx, ny) = (ctx.i("x_amount").clamp(1, 256) as f32, ctx.i("y_amount").clamp(1, 256) as f32);
    let (off, rot, flip, lr, s) = (ctx.f("offset_random"), ctx.b("rotate"), ctx.b("flip"), ctx.f("luminance_random"), ctx.seed);
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, img.kind, |u, v| {
        let (cx, cy) = ((u * nx).floor(), (v * ny).floor());
        let (ci, cj) = ((cx as i32).rem_euclid(nx as i32), (cy as i32).rem_euclid(ny as i32));
        let r = |k: u32| rand2(ci, cj, s ^ k.wrapping_mul(0x2545_f491));
        // Offset from the tile center in uv, flipped and turned by quarter turns.
        let (mut a, mut b) = ((u * nx - cx - 0.5) / nx, (v * ny - cy - 0.5) / ny);
        if flip && r(1) < 0.5 {
            a = -a;
        }
        if rot {
            for _ in 0..(r(2) * 4.0) as i32 {
                (a, b) = (-b, a);
            }
        }
        let su = (cx + 0.5) / nx + a + (r(3) - 0.5) * off;
        let sv = (cy + 0.5) / ny + b + (r(4) - 0.5) * off;
        let p = img.sample(su, sv, true);
        let k = 1.0 - lr * r(5);
        [p[0] * k, p[1] * k, p[2] * k, p[3]]
    })])
}

fn ambient_occlusion_fast(ctx: &Ctx) -> CookResult {
    const DIRS: usize = 8;
    const STEPS: usize = 6;
    let h = ctx.gray_or(0, 0.0);
    let scale = ctx.w as f32 / 256.0;
    let rad = (ctx.f("radius") * scale).clamp(1.0, 1024.0);
    let hs = ctx.f("height_depth") * 64.0 * scale;
    let k = ctx.f("intensity");
    // Integer offsets along each direction, denser near the pixel, and their true distances.
    let taps: Vec<(i64, i64, f32)> = (0..DIRS)
        .flat_map(|d| {
            let (sn, cs) = ((d as f32 + 0.5) / DIRS as f32 * TAU).sin_cos();
            (1..=STEPS).map(move |s| {
                let dist = rad * (s as f32 / STEPS as f32).powf(1.5);
                let (ox, oy) = ((cs * dist).round(), (sn * dist).round());
                (ox as i64, oy as i64, (ox * ox + oy * oy).sqrt().max(1.0))
            })
        })
        .collect();
    let tile = ctx.tile;
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Gray, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let h0 = h.get_gray(x, y, tile);
        let occ: f32 = taps
            .chunks(STEPS)
            .map(|dir| {
                // Tangent of the highest horizon in this direction, as the sine of its angle.
                let m = dir.iter().fold(0.0f32, |m, &(ox, oy, d)| m.max((h.get_gray(x + ox, y + oy, tile) - h0) * hs / d));
                m / (1.0 + m * m).sqrt()
            })
            .sum();
        [(1.0 - k * occ / DIRS as f32).clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}

fn normal_from_height_hq(ctx: &Ctx) -> CookResult {
    const SMOOTH: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    const DERIV: [f32; 5] = [-1.0, -2.0, 0.0, 2.0, 1.0];
    let h = ctx.gray_or(0, 0.0);
    // The kernels sum to 128 × the slope of a linear ramp.
    let k = ctx.f("intensity") * ctx.w as f32 / 32.0 / 128.0;
    let dx_fmt = ctx.i("format") == 1;
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let (mut gx, mut gy) = (0.0, 0.0);
        for j in 0..5 {
            for i in 0..5 {
                let v = h.get_gray(x as i64 + i as i64 - 2, y as i64 + j as i64 - 2, ctx.tile);
                gx += SMOOTH[j] * DERIV[i] * v;
                gy += DERIV[j] * SMOOTH[i] * v;
            }
        }
        let ny = gy * k;
        pack([-gx * k, if dx_fmt { -ny } else { ny }, 1.0], 1.0)
    })])
}

fn histogram_select(ctx: &Ctx) -> CookResult {
    let (pos, half) = (ctx.f("position"), ctx.f("range") * 0.5);
    let soft = (1.0 - ctx.f("contrast").clamp(0.0, 1.0)) * 0.25 + 1e-3;
    let img = ctx.gray_or(0, 0.0);
    Ok(vec![img.map(Kind::Gray, |p| [1.0 - smoothstep(half, half + soft, (p[0] - pos).abs()), 0.0, 0.0, 0.0])])
}

/// Index of the color in `pal` nearest to `c`.
fn nearest(pal: &[[f32; 3]], c: [f32; 3]) -> usize {
    let d = |p: &[f32; 3]| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2);
    pal.iter().enumerate().min_by(|a, b| d(a.1).total_cmp(&d(b.1))).map_or(0, |(i, _)| i)
}

fn quantize_color(ctx: &Ctx) -> CookResult {
    const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]).to_color();
    let k = ctx.i("colors").clamp(2, 64) as usize;
    let rgb = |p: &[f32]| [p[0], p[1], p[2]];
    // Lloyd's k-means on up to ~4096 samples, seeded at luminance quantiles.
    let step = (img.data.len() / 4 / 4096).max(1);
    let mut samples: Vec<[f32; 3]> = img.data.chunks_exact(4).step_by(step).map(rgb).collect();
    samples.sort_by(|a, b| luminance([a[0], a[1], a[2], 1.0]).total_cmp(&luminance([b[0], b[1], b[2], 1.0])));
    let mut pal: Vec<[f32; 3]> = (0..k).filter_map(|j| samples.get(((j as f32 + 0.5) / k as f32 * samples.len() as f32) as usize).copied()).collect();
    for _ in 0..8 {
        let mut acc = vec![[0.0f32; 4]; pal.len()];
        for s in &samples {
            if let Some(a) = acc.get_mut(nearest(&pal, *s)) {
                *a = [a[0] + s[0], a[1] + s[1], a[2] + s[2], a[3] + 1.0];
            }
        }
        for (p, a) in pal.iter_mut().zip(&acc) {
            if a[3] > 0.0 {
                *p = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
            }
        }
    }
    let spread = ctx.f("dither") / (k as f32).cbrt();
    Ok(vec![Image::generate(img.w, img.h, Kind::Color, |x, y| {
        let p = img.get(x as i64, y as i64, false);
        let d = ((BAYER[(y as usize % 4) * 4 + x as usize % 4] + 0.5) / 16.0 - 0.5) * spread;
        let c = pal.get(nearest(&pal, [p[0] + d, p[1] + d, p[2] + d])).copied().unwrap_or([p[0], p[1], p[2]]);
        [c[0], c[1], c[2], p[3]]
    })])
}

fn pixelate_color(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]);
    let (w, h) = (img.w as usize, img.h as usize);
    let (nx, ny) = (ctx.i("x_cells").clamp(1, 1024) as usize, ctx.i("y_cells").clamp(1, 1024) as usize);
    // Block `b` of `n` over `len` pixels covers `b·len/n .. (b+1)·len/n` (at least one pixel).
    let range = |n: usize, len: usize, b: usize| (b * len / n, ((b + 1) * len / n).max(b * len / n + 1).min(len));
    let block = |n: usize, len: usize, p: usize| (((p + 1) * n).saturating_sub(1) / len).min(n - 1);
    let means: Vec<[f32; 4]> = (0..nx * ny)
        .into_par_iter()
        .map(|bi| {
            let ((x0, x1), (y0, y1)) = (range(nx, w, bi % nx), range(ny, h, bi / nx));
            let mut s = [0.0f32; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = img.get(x as i64, y as i64, false);
                    s = std::array::from_fn(|c| s[c] + p[c]);
                }
            }
            let n = ((x1 - x0) * (y1 - y0)).max(1) as f32;
            s.map(|v| v / n)
        })
        .collect();
    Ok(vec![Image::generate(img.w, img.h, img.kind, |x, y| {
        means.get(block(ny, h, y as usize) * nx + block(nx, w, x as usize)).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])
    })])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_and_block_math() {
        assert_eq!(nearest(&[[0.0; 3], [1.0; 3]], [0.7; 3]), 1);
        // Pixelate: a 2-cell split of a black/white image averages each half exactly.
        let img = Image::gray(8, 2, |x, _| if x < 4 { 0.0 } else { 1.0 });
        let def = crate::catalog::find("pixelate_color").unwrap();
        let mut p = crate::catalog::resolve_params(def, &Default::default());
        p.insert("x_cells".into(), materialcraft_core::Value::Int(2));
        p.insert("y_cells".into(), materialcraft_core::Value::Int(1));
        let no_res = |_: &str| None;
        let vars = Default::default();
        let inputs = [Some(img)];
        let ctx = Ctx { w: 8, h: 2, seed: 0, tile: true, inputs: &inputs, params: &p, resource: &no_res, vars: &vars };
        let out = &pixelate_color(&ctx).unwrap()[0];
        assert_eq!((out.get(3, 1, false)[0], out.get(4, 0, false)[0]), (0.0, 1.0));
    }
}
