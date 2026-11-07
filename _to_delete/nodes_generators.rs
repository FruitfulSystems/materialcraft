//! Texture generators › Noises. All tile seamlessly: lattice scales are whole numbers of cells.

use materialcraft_image::{Image, Kind};

use crate::catalog::{CookResult, Ctx, NodeDef, node};
use crate::noise::{self, cellular, fbm, perlin, rand2, ridged, value, value_xy};
use crate::ops;

pub const NOISES: &str = "Texture generators/Noises";

fn scale(ctx: &Ctx) -> f32 {
    ctx.f("scale").max(1.0).round()
}

pub(crate) fn defs() -> Vec<NodeDef> {
    let n = |id, label| node(id, label, NOISES).generator();
    vec![
        n("anisotropic_noise", "Anisotropic Noise")
            .desc("Streaks of value noise stretched along one axis.")
            .float("scale", "X amount", 4.0, 1.0, 64.0)
            .float("scale_y", "Y amount", 64.0, 1.0, 512.0)
            .float("smoothness", "Smoothness", 0.5, 0.0, 1.0)
            .float("interpolation", "Interpolation", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(anisotropic),
        n("bnw_spots_1", "BnW Spots 1")
            .desc("Contrasted black and white spots.")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(bnw1),
        n("bnw_spots_2", "BnW Spots 2")
            .desc("Rounder black and white spots.")
            .float("scale", "Scale", 6.0, 1.0, 64.0)
            .float("roughness", "Roughness", 0.3, 0.0, 1.0)
            .gray_out()
            .cook(bnw2),
        n("bnw_spots_3", "BnW Spots 3")
            .desc("Fine black and white speckles.")
            .float("scale", "Scale", 16.0, 1.0, 64.0)
            .float("roughness", "Roughness", 0.6, 0.0, 1.0)
            .gray_out()
            .cook(bnw3),
        n("cells_1", "Cells 1")
            .desc("Distance to the nearest cell point (soft bumps).")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("disorder", "Disorder", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(cells1),
        n("cells_2", "Cells 2")
            .desc("Cell borders (second minus first distance).")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("disorder", "Disorder", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(cells2),
        n("cells_3", "Cells 3")
            .desc("Flat cells, each a random gray.")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("disorder", "Disorder", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(cells3),
        n("cells_4", "Cells 4")
            .desc("Chebyshev cells: square bumps.")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("disorder", "Disorder", 1.0, 0.0, 1.0)
            .gray_out()
            .cook(cells4),
        n("clouds_1", "Clouds 1")
            .desc("Soft fractal clouds.")
            .float("scale", "Scale", 4.0, 1.0, 32.0)
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(clouds1),
        n("clouds_2", "Clouds 2")
            .desc("Warped, wispy clouds.")
            .float("scale", "Scale", 3.0, 1.0, 32.0)
            .float("roughness", "Roughness", 0.55, 0.0, 1.0)
            .gray_out()
            .cook(clouds2),
        n("clouds_3", "Clouds 3")
            .desc("Detailed, contrasted clouds.")
            .float("scale", "Scale", 6.0, 1.0, 32.0)
            .float("roughness", "Roughness", 0.6, 0.0, 1.0)
            .gray_out()
            .cook(clouds3),
        n("creased", "Creased")
            .desc("Ridged fractal noise: folds and creases.")
            .float("scale", "Scale", 3.0, 1.0, 32.0)
            .float("roughness", "Roughness", 0.55, 0.0, 1.0)
            .float("warp", "Warp intensity", 0.3, 0.0, 1.0)
            .gray_out()
            .cook(creased),
        n("crystal_1", "Crystal 1").desc("Faceted crystal plates.").float("scale", "Scale", 6.0, 1.0, 64.0).gray_out().cook(crystal1),
        n("crystal_2", "Crystal 2").desc("Sharp crystal shards.").float("scale", "Scale", 6.0, 1.0, 64.0).gray_out().cook(crystal2),
        n("dirt", "Dirt").desc("Clumps of dirt.").float("scale", "Scale", 6.0, 1.0, 64.0).float("density", "Density", 0.5, 0.0, 1.0).gray_out().cook(dirt),
        n("dust", "Dust").desc("Sparse specks of dust.").float("density", "Density", 0.02, 0.0, 0.2).float("size", "Size", 1.0, 0.0, 8.0).gray_out().cook(dust),
        n("fractal_sum_base", "Fractal Sum Base")
            .desc("Sum of Perlin noise octaves.")
            .float("scale", "Min level", 2.0, 1.0, 64.0)
            .int("octaves", "Octaves", 8, 1, 12)
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(fractal_sum),
        n("fur", "Fur")
            .desc("Short directional fibers.")
            .float("scale", "Density", 1.0, 0.1, 4.0)
            .angle("angle", "Direction", 0.25)
            .float("length", "Length", 8.0, 0.0, 64.0)
            .gray_out()
            .cook(fur),
        n("gaussian_noise", "Gaussian Noise")
            .desc("Smoothly interpolated value noise.")
            .float("scale", "Scale", 16.0, 1.0, 256.0)
            .gray_out()
            .cook(gaussian_noise),
        n("grunge_map_1", "Grunge Map 1")
            .desc("Stains and blotches.")
            .float("scale", "Scale", 3.0, 1.0, 32.0)
            .float("contrast", "Contrast", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(grunge1),
        n("grunge_map_2", "Grunge Map 2")
            .desc("Speckled grime.")
            .float("scale", "Scale", 5.0, 1.0, 32.0)
            .float("contrast", "Contrast", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(grunge2),
        n("grunge_map_3", "Grunge Map 3")
            .desc("Streaky leaks.")
            .float("scale", "Scale", 4.0, 1.0, 32.0)
            .float("contrast", "Contrast", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(grunge3),
        n("liquid", "Liquid")
            .desc("Domain-warped flowing noise.")
            .float("scale", "Scale", 3.0, 1.0, 32.0)
            .float("warp", "Warp", 0.6, 0.0, 2.0)
            .gray_out()
            .cook(liquid),
        n("moisture_noise", "Moisture Noise")
            .desc("Damp patches with soft edges.")
            .float("scale", "Scale", 4.0, 1.0, 32.0)
            .float("contrast", "Contrast", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(moisture),
        n("perlin_noise", "Perlin Noise")
            .desc("Single-octave gradient noise.")
            .float("scale", "Scale", 8.0, 1.0, 256.0)
            .float("disorder", "Disorder", 0.0, 0.0, 1.0)
            .gray_out()
            .cook(perlin_noise),
        n("plasma", "Plasma").desc("Interfering sine waves through noise.").float("scale", "Scale", 4.0, 1.0, 32.0).gray_out().cook(plasma),
        n("turbulence", "Turbulence")
            .desc("Billowing absolute-value fractal noise.")
            .float("scale", "Scale", 4.0, 1.0, 32.0)
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(turbulence),
        n("voronoi", "Voronoi")
            .desc("Cellular noise with a choice of distance and output.")
            .float("scale", "Scale", 8.0, 1.0, 64.0)
            .float("disorder", "Disorder", 1.0, 0.0, 1.0)
            .choice("metric", "Distance", &["Euclidean", "Manhattan", "Chebyshev"], 0)
            .choice("style", "Output", &["F1", "F2", "F2 - F1", "Cell id"], 2)
            .gray_out()
            .cook(voronoi),
        n("white_noise", "White Noise").desc("Independent random value per pixel.").choice("mode", "Output", &["Grayscale", "Color"], 0).cook(white_noise),
    ]
}

fn anisotropic(ctx: &Ctx) -> CookResult {
    let (sx, sy) = (scale(ctx), ctx.f("scale_y").max(1.0).round());
    let smooth = ctx.f("smoothness");
    let s = ctx.seed;
    let base = ctx.gen_gray(|u, v| {
        let a = value_xy(u * sx, v * sy, sx as i32, sy as i32, s);
        let b = value_xy(u * sx * 2.0, v * sy, (sx * 2.0) as i32, sy as i32, s ^ 77) * 0.5;
        (a * 0.75 + b * 0.25 * (1.0 - smooth) + 0.125 * smooth).clamp(0.0, 1.0)
    });
    Ok(vec![if smooth > 0.0 { ops::gaussian(&base, smooth * 2.0, ctx.tile) } else { base }])
}

fn contrast(v: f32, c: f32) -> f32 {
    ((v - 0.5) * (1.0 + c * 8.0) + 0.5).clamp(0.0, 1.0)
}

fn bnw1(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| contrast(fbm(u, v, sc, 6, 0.3 + r * 0.4, s), 0.6))])
}

fn bnw2(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| contrast(fbm(u, v, sc, 3, 0.3 + r * 0.4, s), 1.2))])
}

fn bnw3(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| contrast(fbm(u, v, sc, 7, 0.4 + r * 0.4, s), 0.9))])
}

fn cell_cook(ctx: &Ctx, metric: u32, f: impl Fn(noise::Cell) -> f32 + Sync) -> CookResult {
    let (sc, dis, s) = (scale(ctx), ctx.f("disorder"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| f(cellular(u * sc, v * sc, sc as i32, dis, metric, s)).clamp(0.0, 1.0))])
}

fn cells1(ctx: &Ctx) -> CookResult {
    cell_cook(ctx, 0, |c| 1.0 - c.f1 * 1.2)
}

fn cells2(ctx: &Ctx) -> CookResult {
    cell_cook(ctx, 0, |c| ((c.f2 - c.f1) * 2.5).min(1.0))
}

fn cells3(ctx: &Ctx) -> CookResult {
    cell_cook(ctx, 0, |c| c.id)
}

fn cells4(ctx: &Ctx) -> CookResult {
    cell_cook(ctx, 2, |c| 1.0 - c.f1 * 1.6)
}

fn crystal1(ctx: &Ctx) -> CookResult {
    let (sc, s) = (scale(ctx), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let c = cellular(u * sc, v * sc, sc as i32, 1.0, 0, s);
        (c.id * 0.6 + (c.f2 - c.f1).min(0.4) + 0.1).clamp(0.0, 1.0)
    })])
}

fn crystal2(ctx: &Ctx) -> CookResult {
    let (sc, s) = (scale(ctx), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let c = cellular(u * sc, v * sc, sc as i32, 1.0, 1, s);
        (c.id * 0.7 + (1.0 - c.f1).max(0.0) * 0.3).clamp(0.0, 1.0)
    })])
}

fn dirt(ctx: &Ctx) -> CookResult {
    let (sc, d, s) = (scale(ctx), ctx.f("density"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let base = fbm(u, v, sc, 6, 0.55, s);
        let spots = 1.0 - cellular(u * sc * 4.0, v * sc * 4.0, (sc * 4.0) as i32, 1.0, 0, s ^ 9).f1;
        let t = 1.0 - d;
        (((base * 0.7 + spots * 0.3) - t * 0.6) * 3.0).clamp(0.0, 1.0)
    })])
}

fn dust(ctx: &Ctx) -> CookResult {
    let (d, s) = (ctx.f("density"), ctx.seed);
    let img = Image::gray(ctx.w, ctx.h, |x, y| if noise::rand2(x as i32, y as i32, s) < d { 0.5 + 0.5 * noise::rand2(y as i32, x as i32, s ^ 3) } else { 0.0 });
    let sigma = ctx.f("size") * ctx.w as f32 / 1024.0;
    let b = ops::gaussian(&img, sigma, ctx.tile);
    let gain = 1.0 + sigma * sigma * 4.0;
    Ok(vec![b.map(Kind::Gray, |p| [(p[0] * gain).min(1.0), 0.0, 0.0, 0.0])])
}

fn fractal_sum(ctx: &Ctx) -> CookResult {
    let (sc, o, r, s) = (scale(ctx), ctx.i("octaves").clamp(1, 12) as u32, ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| fbm(u, v, sc, o, r, s))])
}

fn fur(ctx: &Ctx) -> CookResult {
    let s = ctx.seed;
    let density = ctx.f("scale").clamp(0.1, 4.0);
    let img = Image::gray(ctx.w, ctx.h, |x, y| {
        let r = rand2(x as i32, y as i32, s);
        if r < 0.15 * density { rand2(y as i32, x as i32, s ^ 5) } else { 0.0 }
    });
    let len = ctx.f("length") * ctx.w as f32 / 256.0;
    let (dx, dy) = ((ctx.f("angle") * std::f32::consts::TAU).cos(), -(ctx.f("angle") * std::f32::consts::TAU).sin());
    let n = (len.ceil() as i32).clamp(1, 256);
    let (iw, ih) = (1.0 / ctx.w as f32, 1.0 / ctx.h as f32);
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, Kind::Gray, |u, v| {
        let mut acc = 0.0;
        for i in 0..=n {
            let t = i as f32 / n as f32 * len;
            acc += img.sample_nearest(u - dx * t * iw, v - dy * t * ih, ctx.tile)[0] * (1.0 - i as f32 / (n + 1) as f32);
        }
        [(acc * 2.5 / (n as f32).sqrt()).min(1.0), 0.0, 0.0, 0.0]
    })])
}

fn gaussian_noise(ctx: &Ctx) -> CookResult {
    let (sc, s) = (scale(ctx), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let a = value(u * sc, v * sc, sc as i32, s);
        let b = value(u * sc * 2.0, v * sc * 2.0, (sc * 2.0) as i32, s ^ 1);
        (a * 0.7 + b * 0.3).clamp(0.0, 1.0)
    })])
}

fn grunge(ctx: &Ctx, f: impl Fn(f32, f32, f32, u32) -> f32 + Sync) -> CookResult {
    let (sc, c, s) = (scale(ctx), ctx.f("contrast"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| contrast(f(u, v, sc, s), c))])
}

fn grunge1(ctx: &Ctx) -> CookResult {
    grunge(ctx, |u, v, sc, s| {
        let a = fbm(u, v, sc, 7, 0.6, s);
        let b = 1.0 - cellular(u * sc * 3.0, v * sc * 3.0, (sc * 3.0) as i32, 1.0, 0, s ^ 4).f1;
        a * 0.75 + b * 0.25
    })
}

fn grunge2(ctx: &Ctx) -> CookResult {
    grunge(ctx, |u, v, sc, s| {
        let a = fbm(u, v, sc, 8, 0.65, s);
        let speck = if rand2((u * 512.0) as i32, (v * 512.0) as i32, s ^ 8) > 0.9 { 0.25 } else { 0.0 };
        (a - speck).clamp(0.0, 1.0)
    })
}

fn grunge3(ctx: &Ctx) -> CookResult {
    grunge(ctx, |u, v, sc, s| {
        // Whole cell counts per axis (and the matching periods) so both edges tile.
        let cy = (sc * 0.5).round().max(1.0);
        let streak = value_xy(u * sc * 8.0, v * cy, (sc * 8.0) as i32, cy as i32, s);
        let a = fbm(u, v, sc, 6, 0.55, s ^ 2);
        streak * 0.5 + a * 0.5
    })
}

fn liquid(ctx: &Ctx) -> CookResult {
    let (sc, w, s) = (scale(ctx), ctx.f("warp"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let qx = fbm(u, v, sc, 4, 0.5, s) - 0.5;
        let qy = fbm(u + 0.37, v + 0.71, sc, 4, 0.5, s ^ 3) - 0.5;
        fbm(u + qx * w, v + qy * w, sc, 5, 0.5, s ^ 7)
    })])
}

fn moisture(ctx: &Ctx) -> CookResult {
    let (sc, c, s) = (scale(ctx), ctx.f("contrast"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let a = fbm(u, v, sc, 6, 0.5, s);
        let b = (perlin(u * sc * 2.0, v * sc * 2.0, (sc * 2.0) as i32, s ^ 6)).abs();
        contrast(a * 0.8 + (1.0 - b) * 0.2, c)
    })])
}

fn perlin_noise(ctx: &Ctx) -> CookResult {
    let (sc, d, s) = (scale(ctx), ctx.f("disorder"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let (wu, wv) = if d > 0.0 { ((perlin(u * 3.0, v * 3.0, 3, s ^ 1)) * d * 0.1, (perlin(u * 3.0, v * 3.0, 3, s ^ 2)) * d * 0.1) } else { (0.0, 0.0) };
        (perlin((u + wu) * sc, (v + wv) * sc, sc as i32, s) * 0.5 + 0.5).clamp(0.0, 1.0)
    })])
}

fn plasma(ctx: &Ctx) -> CookResult {
    let (sc, s) = (scale(ctx), ctx.seed);
    let tau = std::f32::consts::TAU;
    Ok(vec![ctx.gen_gray(|u, v| {
        let n = fbm(u, v, sc, 3, 0.5, s) * 2.0;
        let a = (u * tau * sc + n * 3.0).sin() + (v * tau * sc + n * 2.0).sin() + ((u + v) * tau * sc.round() + n).sin();
        (a / 6.0 + 0.5).clamp(0.0, 1.0)
    })])
}

fn turbulence(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    // A negative roughness makes `ridged` negative; clamp so the sqrt can't be NaN.
    Ok(vec![ctx.gen_gray(|u, v| 1.0 - ridged(u, v, sc, 6, r, s).max(0.0).sqrt())])
}

fn creased(ctx: &Ctx) -> CookResult {
    let (sc, r, w, s) = (scale(ctx), ctx.f("roughness"), ctx.f("warp"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let q = fbm(u, v, sc, 3, 0.5, s ^ 11) - 0.5;
        ridged(u + q * w * 0.3, v - q * w * 0.3, sc, 6, r, s)
    })])
}

fn clouds1(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| fbm(u, v, sc, 8, r, s))])
}

fn clouds2(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| {
        let q = fbm(u, v, sc, 4, 0.5, s ^ 5) - 0.5;
        fbm(u + q * 0.35, v + q * 0.35, sc, 8, r, s)
    })])
}

fn clouds3(ctx: &Ctx) -> CookResult {
    let (sc, r, s) = (scale(ctx), ctx.f("roughness"), ctx.seed);
    Ok(vec![ctx.gen_gray(|u, v| contrast(fbm(u, v, sc, 9, r, s), 0.35))])
}

fn voronoi(ctx: &Ctx) -> CookResult {
    let (sc, dis, s) = (scale(ctx), ctx.f("disorder"), ctx.seed);
    let (metric, style) = (ctx.i("metric").clamp(0, 2) as u32, ctx.i("style"));
    Ok(vec![ctx.gen_gray(|u, v| {
        let c = cellular(u * sc, v * sc, sc as i32, dis, metric, s);
        match style {
            0 => c.f1.min(1.0),
            1 => (c.f2 * 0.8).min(1.0),
            3 => c.id,
            _ => ((c.f2 - c.f1) * 2.0).min(1.0),
        }
    })])
}

fn white_noise(ctx: &Ctx) -> CookResult {
    let s = ctx.seed;
    Ok(vec![if ctx.i("mode") == 1 {
        Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
            [rand2(x as i32, y as i32, s), rand2(x as i32, y as i32, s ^ 1), rand2(x as i32, y as i32, s ^ 2), 1.0]
        })
    } else {
        Image::gray(ctx.w, ctx.h, |x, y| rand2(x as i32, y as i32, s))
    }])
}

#[cfg(test)]
#[path = "generators.test.rs"]
mod tests;
