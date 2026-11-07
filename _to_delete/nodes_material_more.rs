//! More material filters: blending whole PBR materials (by mask or by height), weathering masks
//! (edge wear, dirt, dust) and base color utilities.

use materialcraft_image::{Image, Kind};

use crate::catalog::{B, CookResult, Ctx, NodeDef, PortKind, node};
use crate::filters_more::{pack, unpack};
use crate::noise::fbm;
use crate::noises_more::smoothstep;
use crate::ops;

const MAT_BLEND: &str = "Material filters/Blending";
const WEATHER: &str = "Material filters/Weathering";
const PBR: &str = "Material filters/PBR utilities";

/// The channels of a blended material: kind and default (unconnected) value.
const KINDS: [(PortKind, [f32; 4]); 5] = [
    (PortKind::Color, [0.5, 0.5, 0.5, 1.0]),
    (PortKind::Color, [0.5, 0.5, 1.0, 1.0]),
    (PortKind::Gray, [0.5; 4]),
    (PortKind::Gray, [0.0; 4]),
    (PortKind::Gray, [0.5; 4]),
];
const IN_1: [(&str, &str); 5] =
    [("base_color_1", "Base color 1"), ("normal_1", "Normal 1"), ("roughness_1", "Roughness 1"), ("metallic_1", "Metallic 1"), ("height_1", "Height 1")];
const IN_2: [(&str, &str); 5] =
    [("base_color_2", "Base color 2"), ("normal_2", "Normal 2"), ("roughness_2", "Roughness 2"), ("metallic_2", "Metallic 2"), ("height_2", "Height 2")];
const OUT: [(&str, &str); 5] = [("base_color", "Base color"), ("normal", "Normal"), ("roughness", "Roughness"), ("metallic", "Metallic"), ("height", "Height")];

/// Inputs for two materials, then the blended material's outputs.
fn material_ports(mut b: B) -> B {
    for set in [IN_1, IN_2] {
        for ((id, label), (kind, _)) in set.into_iter().zip(KINDS) {
            b = b.input(id, label, kind);
        }
    }
    for ((id, label), (kind, _)) in OUT.into_iter().zip(KINDS) {
        b = b.output(id, label, kind);
    }
    b
}

fn kind_of(k: PortKind) -> Kind {
    if k == PortKind::Color { Kind::Color } else { Kind::Gray }
}

/// Material `slot` (0 or 1), channel `c`, at the output size.
fn channel(ctx: &Ctx, slot: usize, c: usize) -> Image {
    let (pk, def) = KINDS[c];
    let kind = kind_of(pk);
    ctx.any_or(slot * 5 + c, kind, def).to_kind(kind)
}

/// Blend the two input materials by `t` per pixel (0 = material 1); normals are renormalized.
fn mix_materials(ctx: &Ctx, t: &[f32]) -> Vec<Image> {
    (0..5)
        .map(|c| {
            let (a, b) = (channel(ctx, 0, c), channel(ctx, 1, c));
            Image::generate(ctx.w, ctx.h, a.kind, |x, y| {
                let t = t.get((y * ctx.w + x) as usize).copied().unwrap_or(0.0);
                let (p, q) = (a.get(x as i64, y as i64, false), b.get(x as i64, y as i64, false));
                let mut o: [f32; 4] = std::array::from_fn(|k| p[k] + (q[k] - p[k]) * t);
                if c == 1 {
                    let (n1, n2) = (unpack(p), unpack(q));
                    o = pack(std::array::from_fn(|k| n1[k] + (n2[k] - n1[k]) * t), o[3]);
                }
                o
            })
        })
        .collect()
}

/// Weathering parameters shared by the mask nodes.
fn weather_params(b: B) -> B {
    b.float("amount", "Amount", 0.5, 0.0, 1.0)
        .float("noise_scale", "Noise scale", 8.0, 1.0, 64.0)
        .float("noise_amount", "Noise amount", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 0.6, 0.0, 1.0)
}

/// A weathering mask: `base(i)` (0..1, where the effect belongs) broken up by fractal noise and
/// thresholded by the amount.
fn weather(ctx: &Ctx, base: impl Fn(usize) -> f32 + Sync) -> Image {
    let (amt, ns, na, s) = (ctx.f("amount"), ctx.f("noise_scale").max(1.0).round(), ctx.f("noise_amount"), ctx.seed);
    let soft = (1.0 - ctx.f("contrast")) * 0.5 + 0.01;
    Image::generate_uv(ctx.w, ctx.h, Kind::Gray, |u, v| {
        let i = ((v * ctx.h as f32) as usize).min((ctx.h as usize).saturating_sub(1)) * ctx.w as usize
            + ((u * ctx.w as f32) as usize).min((ctx.w as usize).saturating_sub(1));
        let val = base(i) - 2.0 * na * (1.0 - fbm(u, v, ns, 6, 0.6, s));
        [smoothstep(1.0 - amt - soft, 1.0 - amt + soft, val), 0.0, 0.0, 0.0]
    })
}

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        material_ports(node("material_blend", "Material Blend", MAT_BLEND))
            .desc("Blends two whole PBR materials by a mask (white = material 2).")
            .gray_in("mask", "Mask")
            .float("opacity", "Opacity", 1.0, 0.0, 1.0)
            .cook(material_blend),
        material_ports(node("material_height_blend", "Material Height Blend", MAT_BLEND))
            .desc("Blends two whole PBR materials by their heights: the higher one shows, with a soft transition.")
            .float("height_offset", "Height offset", 0.5, 0.0, 1.0)
            .float("contrast", "Contrast", 0.9, 0.0, 1.0)
            .output("mask", "Mask", PortKind::Gray)
            .cook(material_height_blend),
        weather_params(
            node("edge_wear", "Edge Wear", WEATHER)
                .desc("A mask of worn, exposed edges from the convex areas of a height map (or a curvature map), broken up by noise.")
                .gray_in("height", "Height")
                .gray_in("curvature", "Curvature (optional)")
                .float("width", "Edge width", 4.0, 0.25, 64.0),
        )
        .gray_out()
        .cook(edge_wear),
        weather_params(
            node("dirt_mask", "Dirt Mask", WEATHER)
                .desc("A mask of dirt gathering in cavities of a height map (and dark ambient occlusion), broken up by noise.")
                .gray_in("height", "Height")
                .gray_in("ao", "Ambient occlusion (optional)")
                .float("spread", "Spread", 16.0, 0.5, 128.0),
        )
        .gray_out()
        .cook(dirt_mask),
        weather_params(
            node("dust_mask", "Dust Mask", WEATHER)
                .desc("A mask of dust settling on surfaces facing a direction (from a normal map) and on flat areas.")
                .color_in("normal", "Normal")
                .angle("direction", "Up direction", 0.25)
                .float("flat", "On flat areas", 0.3, 0.0, 1.0),
        )
        .gray_out()
        .cook(dust_mask),
        node("material_color_adjust", "Material Color Adjust", PBR)
            .desc("Hue, saturation and value of a base color, optionally through a mask.")
            .color_in("input", "Base color")
            .gray_in("mask", "Mask")
            .float("hue", "Hue", 0.0, -0.5, 0.5)
            .float("saturation", "Saturation", 0.0, -1.0, 1.0)
            .float("value", "Value", 0.0, -1.0, 1.0)
            .color_out()
            .cook(material_color_adjust),
        node("metal_base_color", "Metal Base Color", PBR)
            .desc("The measured base color (linear reflectance) of a common metal, with metallic = 1.")
            .generator()
            .choice("metal", "Metal", &["Iron", "Silver", "Aluminum", "Gold", "Copper", "Chromium", "Nickel", "Titanium", "Cobalt", "Platinum"], 0)
            .output("base_color", "Base color", PortKind::Color)
            .output("metallic", "Metallic", PortKind::Gray)
            .cook(metal_base_color),
    ]
}

fn material_blend(ctx: &Ctx) -> CookResult {
    let op = ctx.f("opacity");
    let m = ctx.gray_or(10, 1.0);
    let t: Vec<f32> = m.data.iter().map(|v| (v * op).clamp(0.0, 1.0)).collect();
    Ok(mix_materials(ctx, &t))
}

fn material_height_blend(ctx: &Ctx) -> CookResult {
    let (h1, h2) = (channel(ctx, 0, 4), channel(ctx, 1, 4));
    let (off, c) = (ctx.f("height_offset") - 0.5, ctx.f("contrast").clamp(0.0, 0.999));
    let soft = (1.0 - c) * 0.5 + 1e-4;
    let t: Vec<f32> = h1.data.iter().zip(h2.data.iter()).map(|(a, b)| ((b + off - a) / soft * 0.5 + 0.5).clamp(0.0, 1.0)).collect();
    let mut out = mix_materials(ctx, &t);
    out.push(ops::gray_from(ctx.w, ctx.h, t));
    Ok(out)
}

fn edge_wear(ctx: &Ctx) -> CookResult {
    let edge: Vec<f32> = match ctx.sized(1) {
        Some(c) => c.to_gray().data.iter().map(|v| ((v - 0.5) * 2.0).clamp(0.0, 1.0)).collect(),
        None => {
            let h = ctx.gray_or(0, 0.0);
            let wide = ops::gaussian(&h, ctx.f("width") * ctx.w as f32 / 256.0, ctx.tile);
            h.data.iter().zip(wide.data.iter()).map(|(a, b)| ((a - b) * 4.0).clamp(0.0, 1.0)).collect()
        }
    };
    Ok(vec![weather(ctx, |i| edge.get(i).copied().unwrap_or(0.0))])
}

fn dirt_mask(ctx: &Ctx) -> CookResult {
    let h = ctx.gray_or(0, 0.0);
    let wide = ops::gaussian(&h, ctx.f("spread") * ctx.w as f32 / 256.0, ctx.tile);
    let ao = ctx.sized(1).map(|a| a.to_gray());
    let cav: Vec<f32> = (0..h.data.len())
        .map(|i| {
            let c = ((wide.data[i] - h.data[i]) * 4.0).clamp(0.0, 1.0);
            ao.as_ref().and_then(|a| a.data.get(i)).map_or(c, |a| c.max(1.0 - a))
        })
        .collect();
    Ok(vec![weather(ctx, |i| cav.get(i).copied().unwrap_or(0.0))])
}

fn dust_mask(ctx: &Ctx) -> CookResult {
    let n = ctx.any_or(0, Kind::Color, [0.5, 0.5, 1.0, 1.0]).to_color();
    let (s, c) = (ctx.f("direction") * std::f32::consts::TAU).sin_cos();
    let flat = ctx.f("flat");
    let base: Vec<f32> = n
        .data
        .chunks(4)
        .map(|p| {
            let v = unpack([p[0], p[1], p[2], 1.0]);
            ((v[0] * c + v[1] * s) * 3.0 + flat * ((v[2] - 0.8) * 5.0).max(0.0)).clamp(0.0, 1.0)
        })
        .collect();
    Ok(vec![weather(ctx, |i| base.get(i).copied().unwrap_or(0.0))])
}

fn material_color_adjust(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]).to_color();
    let m = ctx.gray_or(1, 1.0);
    let (dh, ds, dv) = (ctx.f("hue"), ctx.f("saturation"), ctx.f("value"));
    Ok(vec![Image::generate(ctx.w, ctx.h, Kind::Color, |x, y| {
        let p = img.get(x as i64, y as i64, false);
        let t = m.get_gray(x as i64, y as i64, false);
        let hsl = ops::rgb_to_hsl(p);
        let q = ops::hsl_to_rgb(hsl[0] + dh, (hsl[1] * (1.0 + ds)).clamp(0.0, 1.0), (hsl[2] * (1.0 + dv)).max(0.0));
        [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, p[2] + (q[2] - p[2]) * t, p[3]]
    })])
}

/// Linear base colors (normal-incidence reflectance) of metals, from published measurements.
const METALS: [[f32; 3]; 10] = [
    [0.560, 0.570, 0.580],
    [0.972, 0.960, 0.915],
    [0.913, 0.922, 0.924],
    [1.000, 0.766, 0.336],
    [0.955, 0.638, 0.538],
    [0.550, 0.556, 0.554],
    [0.660, 0.609, 0.526],
    [0.542, 0.497, 0.449],
    [0.662, 0.655, 0.634],
    [0.672, 0.637, 0.585],
];

fn metal_base_color(ctx: &Ctx) -> CookResult {
    let c = METALS.get(ctx.i("metal").max(0) as usize).copied().unwrap_or(METALS[0]);
    Ok(vec![Image::filled(ctx.w, ctx.h, Kind::Color, [c[0], c[1], c[2], 1.0]), Image::filled(ctx.w, ctx.h, Kind::Gray, [1.0; 4])])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use materialcraft_image::{Image, Kind};

    use crate::{Ctx, find, resolve_params};

    #[test]
    fn material_blend_picks_material_2_under_white_mask() {
        let def = find("material_blend").unwrap();
        let params = resolve_params(def, &BTreeMap::new());
        let red = Image::filled(8, 8, Kind::Color, [1.0, 0.0, 0.0, 1.0]);
        let tilted = Image::filled(8, 8, Kind::Color, [0.8, 0.5, 0.9, 1.0]);
        let mut inputs = vec![None; def.inputs.len()];
        inputs[5] = Some(red);
        inputs[6] = Some(tilted);
        inputs[7] = Some(Image::filled(8, 8, Kind::Gray, [0.1; 4]));
        inputs[10] = Some(Image::gray(8, 8, |x, _| if x < 4 { 0.0 } else { 1.0 }));
        let no_res = |_: &str| None;
        let vars = BTreeMap::new();
        let ctx = Ctx { w: 8, h: 8, seed: 0, tile: true, inputs: &inputs, params: &params, resource: &no_res, vars: &vars };
        let out = (def.cook)(&ctx).unwrap();
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].get(6, 2, false), [1.0, 0.0, 0.0, 1.0], "white mask: material 2's base color");
        assert_eq!(out[0].get(1, 2, false), [0.5, 0.5, 0.5, 1.0], "black mask: material 1 (default gray)");
        assert!((out[2].get(6, 2, false)[0] - 0.1).abs() < 1e-6);
        let n = out[1].get(6, 2, false);
        let len = ((n[0] * 2.0 - 1.0).powi(2) + (n[1] * 2.0 - 1.0).powi(2) + (n[2] * 2.0 - 1.0).powi(2)).sqrt();
        assert!((len - 1.0).abs() < 1e-4, "blended normals are unit length");
        assert!(n[0] > 0.7, "material 2's normal: {n:?}");
    }
}
