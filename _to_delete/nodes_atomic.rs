//! The atomic nodes: the built-in operations every graph is made of (blend, blur, levels,
//! curve, HSL, warp, transform, normal, distance, …) plus the graph's own input, output and
//! dot nodes.

use materialcraft_core::Gradient;
use materialcraft_image::{Image, Kind, luminance};

use crate::catalog::{CookResult, Ctx, NodeDef, PortKind, node};
use crate::ops::{self, BLEND_MODES};

pub const ATOMIC: &str = "Atomic nodes";
pub const GRAPH_ITEMS: &str = "Graph items";

/// Output usages (the material channel an output feeds).
pub const USAGES: &[&str] = &["Base color", "Normal", "Roughness", "Metallic", "Height", "Ambient occlusion", "Emissive", "Opacity", "Specular", "Any"];

/// Machine ids of [`USAGES`], used in exports and inspect output.
pub const USAGE_IDS: &[&str] = &["baseColor", "normal", "roughness", "metallic", "height", "ambientOcclusion", "emissive", "opacity", "specular", "any"];

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("bitmap", "Bitmap", ATOMIC)
            .desc("A bitmap resource of the package, resampled to the node size.")
            .atomic()
            .generator()
            .resource("resource", "Bitmap resource")
            .choice("colour_space", "Color space", &["sRGB", "Linear (data)"], 0)
            .color_out()
            .cook(bitmap),
        node("blend", "Blend", ATOMIC)
            .desc("Blends a foreground over a background with a blending mode, weighted by opacity and an optional mask.")
            .atomic()
            .any_in("foreground", "Foreground")
            .any_in("background", "Background")
            .gray_in("mask", "Opacity")
            .float("opacity", "Opacity", 1.0, 0.0, 1.0)
            .choice("mode", "Blending mode", BLEND_MODES, 0)
            .choice("alpha", "Alpha blending", &["Use source alpha", "Ignore alpha"], 0)
            .cook(blend),
        node("blur", "Blur", ATOMIC).desc("Gaussian blur.").atomic().any_in("input", "Input").float("intensity", "Intensity", 2.0, 0.0, 32.0).cook(blur),
        node("channels_shuffle", "Channels Shuffle", ATOMIC)
            .desc("Builds a color image from any channels of two inputs.")
            .atomic()
            .any_in("input1", "Input 1")
            .any_in("input2", "Input 2")
            .choice("red", "Red", CHANNEL_SOURCES, 0)
            .choice("green", "Green", CHANNEL_SOURCES, 1)
            .choice("blue", "Blue", CHANNEL_SOURCES, 2)
            .choice("alpha", "Alpha", CHANNEL_SOURCES, 3)
            .color_out()
            .cook(channels_shuffle),
        node("curve", "Curve", ATOMIC).desc("Remaps values through a tone curve.").atomic().any_in("input", "Input").curve("curve", "Curve").cook(curve),
        node("directional_blur", "Directional Blur", ATOMIC)
            .desc("Blurs along one direction.")
            .atomic()
            .any_in("input", "Input")
            .float("intensity", "Intensity", 10.0, 0.0, 64.0)
            .angle("angle", "Angle", 0.0)
            .cook(directional_blur),
        node("directional_warp", "Directional Warp", ATOMIC)
            .desc("Offsets the input along a direction, by an intensity map.")
            .atomic()
            .any_in("input", "Input")
            .gray_in("intensity_input", "Intensity input")
            .float("intensity", "Intensity", 10.0, 0.0, 64.0)
            .angle("angle", "Warp angle", 0.0)
            .cook(directional_warp),
        node("distance", "Distance", ATOMIC)
            .desc("Grows white areas of a mask into a distance gradient; with a source, spreads its colors outward.")
            .atomic()
            .gray_in("mask", "Mask")
            .any_in("source", "Source")
            .float("distance", "Maximum distance", 10.0, 0.0, 256.0)
            .boolean("combine", "Combine distance with source", false)
            .cook(distance),
        node("emboss", "Emboss", ATOMIC)
            .desc("Lights the input with the relief of a height map.")
            .atomic()
            .any_in("input", "Input")
            .gray_in("height", "Height")
            .float("intensity", "Intensity", 5.0, 0.0, 32.0)
            .angle("light_angle", "Light angle", 0.375)
            .color("highlight", "Highlight color", [1.0, 1.0, 1.0, 1.0])
            .color("shadow", "Shadow color", [0.0, 0.0, 0.0, 1.0])
            .cook(emboss),
        node("gradient_dynamic", "Gradient (Dynamic)", ATOMIC)
            .desc("Maps a grayscale input through the colors of a row of a gradient image.")
            .atomic()
            .gray_in("input", "Input")
            .color_in("gradient", "Gradient")
            .float("position", "Position", 0.5, 0.0, 1.0)
            .choice("orientation", "Orientation", &["Horizontal", "Vertical"], 0)
            .color_out()
            .cook(gradient_dynamic),
        node("gradient_map", "Gradient Map", ATOMIC)
            .desc("Maps grayscale values to colors along a gradient.")
            .atomic()
            .gray_in("input", "Input")
            .gradient("gradient", "Gradient", Gradient::default())
            .choice("mode", "Color mode", &["Color", "Grayscale"], 0)
            .cook(gradient_map),
        node("grayscale_conversion", "Grayscale Conversion", ATOMIC)
            .desc("Weighted sum of the RGBA channels.")
            .atomic()
            .color_in("input", "Input")
            .float("red", "Red weight", 0.333, 0.0, 1.0)
            .float("green", "Green weight", 0.333, 0.0, 1.0)
            .float("blue", "Blue weight", 0.333, 0.0, 1.0)
            .float("alpha", "Alpha weight", 0.0, 0.0, 1.0)
            .boolean("flatten_alpha", "Flatten alpha", false)
            .gray_out()
            .cook(grayscale_conversion),
        node("hsl", "HSL", ATOMIC)
            .desc("Hue, saturation and lightness adjustments (0.5 = unchanged).")
            .atomic()
            .color_in("input", "Input")
            .float("hue", "Hue", 0.5, 0.0, 1.0)
            .float("saturation", "Saturation", 0.5, 0.0, 1.0)
            .float("lightness", "Lightness", 0.5, 0.0, 1.0)
            .color_out()
            .cook(hsl),
        node("levels", "Levels", ATOMIC)
            .desc("Remaps the input range, midtones and output range.")
            .atomic()
            .any_in("input", "Input")
            .float("in_low", "Level in low", 0.0, 0.0, 1.0)
            .float("in_mid", "Level in mid", 0.5, 0.0, 1.0)
            .float("in_high", "Level in high", 1.0, 0.0, 1.0)
            .float("out_low", "Level out low", 0.0, 0.0, 1.0)
            .float("out_high", "Level out high", 1.0, 0.0, 1.0)
            .cook(levels),
        node("normal", "Normal", ATOMIC)
            .desc("Converts a height map to a tangent-space normal map.")
            .atomic()
            .gray_in("input", "Input")
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .choice("format", "Normal format", &["OpenGL", "DirectX"], 0)
            .boolean("height_alpha", "Height in alpha", false)
            .color_out()
            .cook(normal),
        node("pixel_processor", "Pixel Processor", ATOMIC)
            .desc("Computes every pixel with a function graph ($pos, $size; sample the inputs).")
            .atomic()
            .any_in("input0", "Input 0")
            .any_in("input1", "Input 1")
            .any_in("input2", "Input 2")
            .any_in("input3", "Input 3")
            .function("function", "Per pixel function", crate::function::default_pixel_function())
            .choice("output", "Output", &["Color", "Grayscale"], 0)
            .cook(crate::function::pixel_processor),
        node("value_processor", "Value Processor", ATOMIC)
            .desc("Computes one value with a function graph and outputs it as a uniform image.")
            .atomic()
            .any_in("input0", "Input 0")
            .function("function", "Function", materialcraft_core::function::FnGraph::constant(materialcraft_core::Value::Float(0.5)))
            .cook(crate::function::value_processor),
        node("sharpen", "Sharpen", ATOMIC)
            .desc("Unsharp mask.")
            .atomic()
            .any_in("input", "Input")
            .float("intensity", "Intensity", 1.0, 0.0, 10.0)
            .cook(sharpen),
        node("transformation", "Transformation 2D", ATOMIC)
            .desc("Offsets, scales and rotates the input.")
            .atomic()
            .any_in("input", "Input")
            .float2("offset", "Offset", [0.0, 0.0], -1.0, 1.0)
            .float2("scale", "Scale", [1.0, 1.0], 0.01, 10.0)
            .angle("rotation", "Rotation", 0.0)
            .choice("filtering", "Filtering", &["Bilinear", "Nearest"], 0)
            .color("matte", "Matte color", [0.0, 0.0, 0.0, 0.0])
            .cook(transformation),
        node("uniform", "Uniform Color", ATOMIC)
            .desc("A single color or gray value.")
            .atomic()
            .generator()
            .color("color", "Color", [0.5, 0.5, 0.5, 1.0])
            .choice("mode", "Output", &["Color", "Grayscale"], 0)
            .cook(uniform),
        node("warp", "Warp", ATOMIC)
            .desc("Distorts the input along the slopes of a gradient map.")
            .atomic()
            .any_in("input", "Input")
            .gray_in("gradient", "Gradient")
            .float("intensity", "Intensity", 1.0, 0.0, 32.0)
            .cook(warp),
        node("input_color", "Input Color", GRAPH_ITEMS)
            .desc("A color image input of the graph.")
            .atomic()
            .generator()
            .text("identifier", "Identifier", "input")
            .color("default", "Default color", [0.5, 0.5, 0.5, 1.0])
            .color_out()
            .cook(input_color_default),
        node("input_grayscale", "Input Grayscale", GRAPH_ITEMS)
            .desc("A grayscale image input of the graph.")
            .atomic()
            .generator()
            .text("identifier", "Identifier", "input")
            .color("default", "Default value", [0.5, 0.5, 0.5, 1.0])
            .gray_out()
            .cook(input_gray_default),
        node("output", "Output", GRAPH_ITEMS)
            .desc("An output of the graph: a material channel (usage) or any image.")
            .atomic()
            .any_in("input", "Input")
            .text("identifier", "Identifier", "output")
            .choice("usage", "Usage", USAGES, 0)
            .cook(passthrough),
        node("dot", "Dot", GRAPH_ITEMS).desc("A reroute point for wires.").atomic().any_in("input", "Input").cook(passthrough),
        node("graph_instance", "Graph Instance", GRAPH_ITEMS)
            .desc("Another graph of the package, used as a node.")
            .atomic()
            .text("graph", "Graph", "")
            .cook(crate::catalog::no_cook),
    ]
}

const CHANNEL_SOURCES: &[&str] =
    &["Input 1 red", "Input 1 green", "Input 1 blue", "Input 1 alpha", "Input 2 red", "Input 2 green", "Input 2 blue", "Input 2 alpha", "Black", "White"];

fn bitmap(ctx: &Ctx) -> CookResult {
    let name = ctx.text("resource");
    // "Linear (data)" keeps the file's values as stored (normal maps, masks); sRGB decodes color.
    let key = materialcraft_image::resource_key(name, ctx.i("colour_space") == 1);
    match (ctx.resource)(&key) {
        Some(img) => Ok(vec![img.resize(ctx.w, ctx.h, ctx.tile)]),
        None if name.is_empty() => Ok(vec![ctx.gen_color(|u, v| {
            let c = if ((u * 8.0) as i32 + (v * 8.0) as i32) % 2 == 0 { 0.8 } else { 0.2 };
            [c, c, c, 1.0]
        })]),
        None => Err(format!("no bitmap resource `{name}`")),
    }
}

fn blend(ctx: &Ctx) -> CookResult {
    let (fg, bg) = (ctx.sized(0), ctx.sized(1));
    let kind = if [&fg, &bg].iter().any(|i| i.as_ref().is_some_and(|i| !i.is_gray())) { Kind::Color } else { Kind::Gray };
    let fg = fg.unwrap_or_else(|| Image::filled(ctx.w, ctx.h, kind, [0.0, 0.0, 0.0, 0.0]));
    let bg = bg.unwrap_or_else(|| Image::filled(ctx.w, ctx.h, kind, [0.0, 0.0, 0.0, 1.0]));
    let mask = ctx.sized(2).map(|m| m.to_gray());
    let (mode, op) = (ctx.i("mode"), ctx.f("opacity"));
    let use_alpha = ctx.i("alpha") == 0 && !fg.is_gray();
    Ok(vec![Image::generate(ctx.w, ctx.h, kind, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, false));
        ops::blend_px(mode, bg.get(x, y, false), fg.get(x, y, false), op * m, use_alpha, true)
    })])
}

fn blur(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let sigma = ctx.f("intensity").max(0.0) * ctx.w as f32 / 256.0 * 0.5;
    Ok(vec![ops::gaussian(&img, sigma, ctx.tile)])
}

fn channels_shuffle(ctx: &Ctx) -> CookResult {
    let a = ctx.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]).to_color();
    let b = ctx.any_or(1, Kind::Color, [0.0, 0.0, 0.0, 1.0]).to_color();
    let src = [ctx.i("red"), ctx.i("green"), ctx.i("blue"), ctx.i("alpha")];
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let (pa, pb) = (a.get(x as i64, y as i64, false), b.get(x as i64, y as i64, false));
        src.map(|s| match s {
            0..=3 => pa[s as usize],
            4..=7 => pb[(s - 4) as usize],
            9 => 1.0,
            _ => 0.0,
        })
    })])
}

fn curve(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let c = ctx.curve("curve");
    // A 1024-entry table: the curve is smooth, so linear lookups are exact enough.
    let lut: Vec<f32> = (0..=1024).map(|i| c.eval(i as f32 / 1024.0)).collect();
    let f = |v: f32| {
        let t = v.clamp(0.0, 1.0) * 1024.0;
        let i = t as usize;
        let a = lut.get(i).copied().unwrap_or(0.0);
        let b = lut.get(i + 1).copied().unwrap_or(a);
        a + (b - a) * (t - i as f32)
    };
    Ok(vec![img.map(img.kind, |p| [f(p[0]), f(p[1]), f(p[2]), p[3]])])
}

fn dir_vec(turns: f32) -> (f32, f32) {
    let a = turns * std::f32::consts::TAU;
    (a.cos(), -a.sin())
}

fn directional_blur(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let len = ctx.f("intensity").max(0.0) * ctx.w as f32 / 256.0;
    if len < 0.5 {
        return Ok(vec![img]);
    }
    let (dx, dy) = dir_vec(ctx.f("angle"));
    let n = (len.ceil() as i32).clamp(1, 512);
    let (iw, ih) = (1.0 / ctx.w as f32, 1.0 / ctx.h as f32);
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, img.kind, |u, v| {
        let mut acc = [0.0; 4];
        for i in -n..=n {
            let t = i as f32 / n as f32 * len * 0.5;
            let p = img.sample(u + dx * t * iw, v + dy * t * ih, ctx.tile);
            for k in 0..4 {
                acc[k] += p[k];
            }
        }
        acc.map(|a| a / (2 * n + 1) as f32)
    })])
}

fn directional_warp(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let map = ctx.gray_or(1, 0.0);
    let (dx, dy) = dir_vec(ctx.f("angle"));
    let k = ctx.f("intensity") / 256.0;
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, img.kind, |u, v| {
        let m = map.sample_gray(u, v, ctx.tile) * k;
        img.sample(u + dx * m, v + dy * m, ctx.tile)
    })])
}

fn distance(ctx: &Ctx) -> CookResult {
    let mask = ctx.gray_or(0, 0.0);
    let maxd = (ctx.f("distance").max(0.0) * ctx.w as f32 / 256.0).max(1e-3);
    let data = mask.data.clone();
    let inside = |i| data.get(i).copied().unwrap_or(0.0) > 0.5;
    let Some(src) = ctx.sized(1) else {
        let d2 = ops::distance_sq(ctx.w, ctx.h, inside, ctx.tile);
        return Ok(vec![ops::gray_from(ctx.w, ctx.h, d2.iter().map(|d| (1.0 - d.sqrt() / maxd).clamp(0.0, 1.0)).collect())]);
    };
    // Propagate the exact nearest mask pixel. Gradient walks can stall at stationary points or
    // stop before reaching the mask, leaving background-colored gaps inside the distance range.
    let (d2, sites) = ops::distance_sq_with_sites(ctx.w, ctx.h, inside, ctx.tile);
    let combine = ctx.b("combine");
    Ok(vec![Image::generate(ctx.w, ctx.h, src.kind, |x, y| {
        let i = y as usize * ctx.w as usize + x as usize;
        let site = sites.get(i).copied().unwrap_or(usize::MAX);
        let mut c = if site == usize::MAX {
            [0.0, 0.0, 0.0, if src.is_gray() { 1.0 } else { 0.0 }]
        } else {
            src.get((site % ctx.w as usize) as i64, (site / ctx.w as usize) as i64, false)
        };
        let d = d2.get(i).copied().unwrap_or(f32::INFINITY).sqrt();
        if d > maxd {
            c = [0.0, 0.0, 0.0, if src.is_gray() { 1.0 } else { 0.0 }];
        } else if combine {
            let f = (1.0 - d / maxd).clamp(0.0, 1.0);
            c = [c[0] * f, c[1] * f, c[2] * f, c[3]];
        }
        c
    })])
}

fn emboss(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]);
    let hmap = ctx.gray_or(1, 0.0);
    let (lx, ly) = dir_vec(ctx.f("light_angle"));
    let k = ctx.f("intensity") * ctx.w as f32 / 64.0;
    let (hi, sh) = (ctx.v4("highlight"), ctx.v4("shadow"));
    Ok(vec![Image::generate(ctx.w, ctx.h, img.kind, |x, y| {
        let (gx, gy) = ops::slope(&hmap, x as i64, y as i64, ctx.tile);
        let l = (-(gx * lx + gy * ly) * k).clamp(-1.0, 1.0);
        let p = img.get(x as i64, y as i64, false);
        let tint = if l >= 0.0 { hi } else { sh };
        let t = l.abs();
        [p[0] + (tint[0] - p[0]) * t, p[1] + (tint[1] - p[1]) * t, p[2] + (tint[2] - p[2]) * t, p[3]]
    })])
}

fn gradient_dynamic(ctx: &Ctx) -> CookResult {
    let input = ctx.gray_or(0, 0.0);
    let grad = ctx.input(1).map(|g| g.to_color()).unwrap_or_else(|| Image::generate_uv(256, 1, Kind::Color, |u, _| [u, u, u, 1.0]));
    let pos = ctx.f("position");
    let vertical = ctx.i("orientation") == 1;
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let t = input.get_gray(x as i64, y as i64, false).clamp(0.0, 1.0);
        if vertical { grad.sample(pos, t, false) } else { grad.sample(t, pos, false) }
    })])
}

fn gradient_map(ctx: &Ctx) -> CookResult {
    let input = ctx.gray_or(0, 0.0);
    let g = ctx.gradient("gradient");
    let lut: Vec<[f32; 4]> = (0..=512).map(|i| g.eval(i as f32 / 512.0)).collect();
    let gray = ctx.i("mode") == 1;
    let look = |v: f32| lut.get((v.clamp(0.0, 1.0) * 512.0).round() as usize).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
    Ok(vec![if gray { input.map(Kind::Gray, |p| [luminance(look(p[0])), 0.0, 0.0, 0.0]) } else { input.map(Kind::Color, |p| look(p[0])) }])
}

fn grayscale_conversion(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]).to_color();
    let w = [ctx.f("red"), ctx.f("green"), ctx.f("blue"), ctx.f("alpha")];
    let flatten = ctx.b("flatten_alpha");
    Ok(vec![img.map(Kind::Gray, |p| {
        let mut v = p[0] * w[0] + p[1] * w[1] + p[2] * w[2] + p[3] * w[3];
        if flatten {
            v *= p[3];
        }
        [v, 0.0, 0.0, 0.0]
    })])
}

fn hsl(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]).to_color();
    let (dh, sat, lig) = (ctx.f("hue") - 0.5, ctx.f("saturation"), ctx.f("lightness"));
    Ok(vec![img.map(Kind::Color, |p| {
        let [h, s, l] = ops::rgb_to_hsl(p);
        let s = if sat <= 0.5 { s * sat * 2.0 } else { s + (1.0 - s) * (sat - 0.5) * 2.0 };
        let l = if lig <= 0.5 { l * lig * 2.0 } else { l + (1.0 - l) * (lig - 0.5) * 2.0 };
        let c = ops::hsl_to_rgb(h + dh, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
        [c[0], c[1], c[2], p[3]]
    })])
}

/// The Levels mapping for one value.
pub fn levels_fn(v: f32, in_low: f32, in_mid: f32, in_high: f32, out_low: f32, out_high: f32) -> f32 {
    let span = in_high - in_low;
    let t = if span.abs() < 1e-6 { if v >= in_high { 1.0 } else { 0.0 } } else { ((v - in_low) / span).clamp(0.0, 1.0) };
    // NaN (from an input or a hostile parameter) would otherwise spread through the graph.
    let t = if t.is_finite() { t } else { 0.0 };
    let mid = in_mid.clamp(0.001, 0.999);
    let g = 0.5f32.ln() / mid.ln();
    out_low + t.powf(g) * (out_high - out_low)
}

fn levels(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let p = [ctx.f("in_low"), ctx.f("in_mid"), ctx.f("in_high"), ctx.f("out_low"), ctx.f("out_high")];
    let f = |v: f32| levels_fn(v, p[0], p[1], p[2], p[3], p[4]);
    Ok(vec![img.map(img.kind, |c| [f(c[0]), f(c[1]), f(c[2]), c[3]])])
}

/// Tangent-space normal from a height map; `k` scales the slopes.
pub fn height_to_normal(h: &Image, k: f32, directx: bool, tile: bool, alpha_height: bool) -> Image {
    Image::generate(h.w, h.h, Kind::Color, |x, y| {
        let (dx, dy) = ops::slope(h, x as i64, y as i64, tile);
        let (nx, ny, nz) = (-dx * k, dy * k, 1.0);
        let l = (nx * nx + ny * ny + nz * nz).sqrt();
        let ny = if directx { -ny } else { ny };
        let a = if alpha_height { h.get_gray(x as i64, y as i64, tile) } else { 1.0 };
        [nx / l * 0.5 + 0.5, ny / l * 0.5 + 0.5, nz / l * 0.5 + 0.5, a]
    })
}

fn normal(ctx: &Ctx) -> CookResult {
    let h = ctx.gray_or(0, 0.0);
    let k = ctx.f("intensity") * ctx.w as f32 / 32.0;
    Ok(vec![height_to_normal(&h, k, ctx.i("format") == 1, ctx.tile, ctx.b("height_alpha"))])
}

fn sharpen(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let soft = ops::gaussian(&img, 1.0, ctx.tile);
    let k = ctx.f("intensity");
    Ok(vec![Image::generate(ctx.w, ctx.h, img.kind, |x, y| {
        let (a, b) = (img.get(x as i64, y as i64, false), soft.get(x as i64, y as i64, false));
        [a[0] + (a[0] - b[0]) * k, a[1] + (a[1] - b[1]) * k, a[2] + (a[2] - b[2]) * k, a[3]]
    })])
}

/// Sample `img` through the inverse of offset → rotate → scale about the center.
pub fn transform_sample(img: &Image, u: f32, v: f32, offset: [f32; 2], scale: [f32; 2], rot_turns: f32, nearest: bool, tile: bool) -> Option<[f32; 4]> {
    let (s, c) = (rot_turns * std::f32::consts::TAU).sin_cos();
    let (x, y) = (u - 0.5 - offset[0], v - 0.5 - offset[1]);
    let (x, y) = (x * c - y * s, x * s + y * c);
    let (sx, sy) = (if scale[0].abs() < 1e-4 { 1e-4 } else { scale[0] }, if scale[1].abs() < 1e-4 { 1e-4 } else { scale[1] });
    let (su, sv) = (x / sx + 0.5, y / sy + 0.5);
    if !tile && (!(0.0..1.0).contains(&su) || !(0.0..1.0).contains(&sv)) {
        return None;
    }
    Some(if nearest { img.sample_nearest(su, sv, tile) } else { img.sample(su, sv, tile) })
}

fn transformation(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let (off, sc, rot) = (ctx.v2("offset"), ctx.v2("scale"), ctx.f("rotation"));
    let nearest = ctx.i("filtering") == 1;
    let matte = ctx.v4("matte");
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, img.kind, |u, v| transform_sample(&img, u, v, off, sc, rot, nearest, ctx.tile).unwrap_or(matte))])
}

fn uniform(ctx: &Ctx) -> CookResult {
    let c = ctx.v4("color");
    Ok(vec![if ctx.i("mode") == 1 { Image::filled(ctx.w, ctx.h, Kind::Gray, [luminance(c); 4]) } else { Image::filled(ctx.w, ctx.h, Kind::Color, c) }])
}

fn warp(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Gray, [0.0; 4]);
    let grad = ctx.gray_or(1, 0.0);
    let k = ctx.f("intensity");
    Ok(vec![Image::generate_uv(ctx.w, ctx.h, img.kind, |u, v| {
        let (x, y) = ((u * ctx.w as f32) as i64, (v * ctx.h as f32) as i64);
        let (dx, dy) = ops::slope(&grad, x, y, ctx.tile);
        img.sample(u + dx * k, v + dy * k, ctx.tile)
    })])
}

fn input_color_default(ctx: &Ctx) -> CookResult {
    Ok(vec![Image::filled(ctx.w, ctx.h, Kind::Color, ctx.v4("default"))])
}

fn input_gray_default(ctx: &Ctx) -> CookResult {
    Ok(vec![Image::filled(ctx.w, ctx.h, Kind::Gray, [ctx.v4("default")[0]; 4])])
}

pub(crate) fn passthrough(ctx: &Ctx) -> CookResult {
    Ok(vec![ctx.sized(0).unwrap_or_else(|| Image::filled(ctx.w, ctx.h, Kind::Gray, [0.0; 4]))])
}

/// The kind an output port produces given the node's inputs.
pub fn output_kind(def: &NodeDef, port: usize, inputs: &[Option<Image>]) -> Kind {
    match def.outputs.get(port).map(|p| p.kind) {
        Some(PortKind::Gray) => Kind::Gray,
        Some(PortKind::Color) => Kind::Color,
        _ => inputs.iter().flatten().next().map_or(Kind::Gray, |i| i.kind),
    }
}

#[cfg(test)]
mod levels_tests {
    use super::levels_fn;

    #[test]
    fn levels_remaps_the_input_range_midtones_and_output_range() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // Identity by default.
        for v in [0.0, 0.25, 0.5, 1.0] {
            assert!(close(levels_fn(v, 0.0, 0.5, 1.0, 0.0, 1.0), v));
        }
        // The input range stretches and clips.
        assert!(close(levels_fn(0.4, 0.2, 0.5, 0.6, 0.0, 1.0), 0.5));
        assert!(close(levels_fn(0.1, 0.2, 0.5, 0.6, 0.0, 1.0), 0.0));
        assert!(close(levels_fn(0.9, 0.2, 0.5, 0.6, 0.0, 1.0), 1.0));
        // Mid is relative to the input range: the value there becomes 0.5.
        assert!(close(levels_fn(0.25, 0.0, 0.25, 1.0, 0.0, 1.0), 0.5));
        assert!(close(levels_fn(0.3, 0.2, 0.25, 0.6, 0.0, 1.0), 0.5));
        // The output range, inverted when low > high.
        assert!(close(levels_fn(0.5, 0.0, 0.5, 1.0, 0.2, 0.6), 0.4));
        assert!(close(levels_fn(0.0, 0.0, 0.5, 1.0, 1.0, 0.0), 1.0));
        // A collapsed input range is a threshold; NaN never escapes.
        assert_eq!(levels_fn(0.49, 0.5, 0.5, 0.5, 0.0, 1.0), 0.0);
        assert_eq!(levels_fn(0.5, 0.5, 0.5, 0.5, 0.0, 1.0), 1.0);
        assert!(levels_fn(f32::NAN, 0.0, 0.5, 1.0, 0.0, 1.0).is_finite());
        assert!(levels_fn(0.5, f32::NAN, 0.5, f32::NAN, 0.0, 1.0).is_finite());
    }
}
