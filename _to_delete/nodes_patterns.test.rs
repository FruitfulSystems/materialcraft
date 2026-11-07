//! Hostile-parameter tests for the binned pattern generators in `patterns.rs`.

use std::collections::BTreeMap;

use materialcraft_core::Value;
use materialcraft_image::Image;

use crate::{Ctx, find, resolve_params};

fn cook(id: &str, over: &[(&str, Value)]) -> Image {
    let def = find(id).unwrap();
    let over: BTreeMap<String, Value> = over.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    let params = resolve_params(def, &over);
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let inputs = vec![None; def.inputs.len()];
    let ctx = Ctx { w: 32, h: 32, seed: 3, tile: true, inputs: &inputs, params: &params, resource: &no_res, vars: &vars };
    (def.cook)(&ctx).unwrap().remove(0)
}

fn assert_quick(id: &str, over: &[(&str, Value)]) {
    let t = std::time::Instant::now();
    let img = cook(id, over);
    assert!(img.data.iter().all(|v| v.is_finite()), "{id} produced non-finite values");
    assert!(t.elapsed().as_secs() < 20, "{id} took {:?}", t.elapsed());
}

#[test]
fn splatter_huge_size_is_bounded() {
    assert_quick("splatter", &[("size", Value::Float(1e6)), ("count", Value::Int(2000))]);
}

#[test]
fn scratches_huge_length_is_bounded() {
    assert_quick("scratches", &[("length", Value::Float(1e6)), ("width", Value::Float(1e6)), ("count", Value::Int(5000))]);
}
