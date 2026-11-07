//! Material filters: nodes that make or process whole PBR materials (several outputs).

use materialcraft_image::{Image, Kind, linear_to_srgb, srgb_to_linear};

use crate::atomic::height_to_normal;
use crate::catalog::{CookResult, Ctx, NodeDef, PortKind, node};
use crate::ops;

const PBR: &str = "Material filters/PBR utilities";

pub(crate) fn defs() -> Vec<NodeDef> {
    vec![
        node("pbr_base_material", "PBR Base Material", PBR)
            .desc("A uniform PBR material: base color, normal, roughness, metallic, height and ambient occlusion.")
            .generator()
            .color("base_color", "Base color", [0.5, 0.5, 0.5, 1.0])
            .float("roughness", "Roughness", 0.5, 0.0, 1.0)
            .float("metallic", "Metallic", 0.0, 0.0, 1.0)
            .float("height", "Height", 0.5, 0.0, 1.0)
            .float("ao", "Ambient occlusion", 1.0, 0.0, 1.0)
            .output("base_color", "Base color", PortKind::Color)
            .output("normal", "Normal", PortKind::Color)
            .output("roughness", "Roughness", PortKind::Gray)
            .output("metallic", "Metallic", PortKind::Gray)
            .output("height", "Height", PortKind::Gray)
            .output("ambient_occlusion", "Ambient occlusion", PortKind::Gray)
            .cook(pbr_base),
        node("height_normal_ao", "Height to Normal and AO", PBR)
            .desc("A normal map and an ambient occlusion map from one height map.")
            .gray_in("height", "Height")
            .float("normal_intensity", "Normal intensity", 1.0, 0.0, 10.0)
            .float("ao_spread", "AO spreading", 0.15, 0.0, 1.0)
            .float("ao_depth", "AO depth", 0.5, 0.0, 1.0)
            .output("normal", "Normal", PortKind::Color)
            .output("ambient_occlusion", "Ambient occlusion", PortKind::Gray)
            .cook(height_normal_ao),
        node("albedo_safe", "Albedo Safe Color", PBR)
            .desc("Keeps base colors inside the physically plausible range (sRGB 30-240).")
            .color_in("input", "Base color")
            .color_out()
            .cook(albedo_safe),
        node("metallic_mask", "Metallic From Mask", PBR)
            .desc("Turns a mask into a clean metallic map (fully metallic or not).")
            .gray_in("input", "Mask")
            .float("threshold", "Threshold", 0.5, 0.0, 1.0)
            .gray_out()
            .cook(metallic_mask),
    ]
}

fn pbr_base(ctx: &Ctx) -> CookResult {
    let (w, h) = (ctx.w, ctx.h);
    let g = |v: f32| Image::filled(w, h, Kind::Gray, [v; 4]);
    Ok(vec![
        Image::filled(w, h, Kind::Color, ctx.v4("base_color")),
        Image::filled(w, h, Kind::Color, [0.5, 0.5, 1.0, 1.0]),
        g(ctx.f("roughness")),
        g(ctx.f("metallic")),
        g(ctx.f("height")),
        g(ctx.f("ao")),
    ])
}

fn height_normal_ao(ctx: &Ctx) -> CookResult {
    let h = ctx.gray_or(0, 0.5);
    let n = height_to_normal(&h, ctx.f("normal_intensity") * ctx.w as f32 / 32.0, false, ctx.tile, false);
    let spread = ctx.f("ao_spread").max(0.001) * ctx.w as f32 * 0.25;
    let wide = ops::gaussian(&h, spread, ctx.tile);
    let depth = ctx.f("ao_depth");
    let ao = Image::generate(ctx.w, ctx.h, Kind::Gray, |x, y| {
        let i = (y * ctx.w + x) as usize;
        [(1.0 - (wide.data[i] - h.data[i]).max(0.0) * depth * 6.0).clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    });
    Ok(vec![n, ao])
}

fn albedo_safe(ctx: &Ctx) -> CookResult {
    let img = ctx.any_or(0, Kind::Color, [0.5, 0.5, 0.5, 1.0]).to_color();
    let (lo, hi) = (30.0 / 255.0, 240.0 / 255.0);
    Ok(vec![img.map(Kind::Color, |p| {
        let f = |v: f32| srgb_to_linear(linear_to_srgb(v).clamp(lo, hi));
        [f(p[0]), f(p[1]), f(p[2]), p[3]]
    })])
}

fn metallic_mask(ctx: &Ctx) -> CookResult {
    let t = ctx.f("threshold");
    let img = ctx.gray_or(0, 0.0);
    Ok(vec![img.map(Kind::Gray, |p| [if p[0] >= t { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0])])
}
