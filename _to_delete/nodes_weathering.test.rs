//! Tiling tests for the weathering nodes.

use std::collections::BTreeMap;

use crate::{Ctx, find, resolve_params};

/// Mean absolute difference across the wrap versus across every interior neighbour pair, per axis.
fn wrap_vs_interior(id: &str) -> [(f32, f32); 2] {
    let def = find(id).unwrap();
    let params = resolve_params(def, &BTreeMap::new());
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let inputs = vec![Some(materialcraft_image::Image::filled(64, 64, materialcraft_image::Kind::Gray, [1.0; 4])); def.inputs.len()];
    let ctx = Ctx { w: 64, h: 64, seed: 9, tile: true, inputs: &inputs, params: &params, resource: &no_res, vars: &vars };
    let img = (def.cook)(&ctx).unwrap().remove(0);
    let g = |x: i64, y: i64| img.get(x, y, false)[0];
    let (mut hw, mut hi, mut vw, mut vi) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..64 {
        hw += (g(63, k) - g(0, k)).abs();
        vw += (g(k, 63) - g(k, 0)).abs();
        for j in 0..63 {
            hi += (g(j, k) - g(j + 1, k)).abs() / 63.0;
            vi += (g(k, j) - g(k, j + 1)).abs() / 63.0;
        }
    }
    [(hw, hi), (vw, vi)]
}

#[test]
fn leaks_tile_in_both_directions() {
    for (axis, (wrap, interior)) in wrap_vs_interior("leaks").into_iter().enumerate() {
        assert!(wrap <= interior * 3.0 + 0.5, "axis {axis}: wrap {wrap} vs interior {interior}");
    }
}
