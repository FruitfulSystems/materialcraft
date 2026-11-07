//! Tiling tests for generators whose cell counts differ per axis.

use std::collections::BTreeMap;

use materialcraft_core::Value;
use materialcraft_image::Image;

use crate::{Ctx, find, resolve_params};

fn cook(id: &str, scale: f32, extra: &[(&str, Value)]) -> Image {
    let def = find(id).unwrap();
    let mut over: BTreeMap<String, Value> = extra.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    over.insert("scale".into(), Value::Float(scale));
    let params = resolve_params(def, &over);
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let inputs = vec![None; def.inputs.len()];
    let ctx = Ctx { w: 64, h: 64, seed: 5, tile: true, inputs: &inputs, params: &params, resource: &no_res, vars: &vars };
    (def.cook)(&ctx).unwrap().remove(0)
}

/// Wrap jump along both axes versus the typical neighbour difference.
fn seams(img: &Image) -> (f32, f32, f32, f32) {
    let (mut hs, mut hi, mut vs, mut vi) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..64 {
        hs += (img.get(63, k, false)[0] - img.get(0, k, false)[0]).abs();
        hi += (img.get(31, k, false)[0] - img.get(32, k, false)[0]).abs();
        vs += (img.get(k, 63, false)[0] - img.get(k, 0, false)[0]).abs();
        vi += (img.get(k, 31, false)[0] - img.get(k, 32, false)[0]).abs();
    }
    (hs, hi, vs, vi)
}

#[test]
fn grunge_map_3_tiles_at_odd_scales() {
    for scale in [3.0, 5.0, 7.0] {
        let (hs, hi, vs, vi) = seams(&cook("grunge_map_3", scale, &[]));
        assert!(hs <= hi * 3.0 + 0.5, "scale {scale}: horizontal seam {hs} vs {hi}");
        assert!(vs <= vi * 3.0 + 0.5, "scale {scale}: vertical seam {vs} vs {vi}");
    }
}

#[test]
fn anisotropic_noise_tiles_with_different_axis_scales() {
    let (hs, hi, vs, vi) = seams(&cook("anisotropic_noise", 8.0, &[("scale_y", Value::Float(3.0))]));
    assert!(hs <= hi * 3.0 + 0.5, "horizontal seam {hs} vs {hi}");
    assert!(vs <= vi * 3.0 + 0.5, "vertical seam {vs} vs {vi}");
}
