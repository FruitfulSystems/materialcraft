use materialcraft_engine::{execute, new_session};
use serde_json::json;

#[test]
fn rejected_2d_settings_leave_the_view_unchanged() {
    let mut s = new_session();
    let a = execute(&mut s, "graph.node.add", &json!({"def": "rgba_split"})).unwrap()["id"].as_u64().unwrap();
    let b = execute(&mut s, "graph.node.add", &json!({"def": "rgba_split"})).unwrap()["id"].as_u64().unwrap();
    execute(&mut s, "view.2d", &json!({"node": a, "port": 1, "channel": "g"})).unwrap();
    let before = s.view2d.clone();
    // A bad channel used to leave all the preceding fields changed despite returning an error.
    assert!(execute(&mut s, "view.2d", &json!({"node": b, "port": 2, "tiled": true, "histogram": true, "channel": "invalid"})).is_err());
    assert_eq!(s.view2d, before);
    assert!(execute(&mut s, "view.2d", &json!({"node": u64::MAX, "channel": "b", "tiled": true})).is_err());
    assert_eq!(s.view2d, before);
    assert_eq!(execute(&mut s, "session.inspect", &json!({})).unwrap()["view2d"], serde_json::to_value(&before).unwrap());
}

#[test]
fn rejected_3d_settings_leave_the_view_unchanged() {
    let mut s = new_session();
    execute(&mut s, "view.3d", &json!({"scene": "sphere", "environment": "sunset", "yaw": 70})).unwrap();
    let before = s.view3d.clone();
    // A valid scene must not be applied when another setting in the request is rejected.
    for params in [
        json!({"scene": "cube", "environment": "invalid", "yaw": 10}),
        json!({"scene": "invalid", "environment": "studio"}),
        json!({"scene": "custom", "environment": "studio"}),
        json!({"scene": "cube", "target": [1, 2]}),
    ] {
        assert!(execute(&mut s, "view.3d", &params).is_err(), "{params}");
        assert_eq!(s.view3d, before, "{params}");
    }
    assert_eq!(execute(&mut s, "session.inspect", &json!({})).unwrap()["view3d"], serde_json::to_value(&before).unwrap());
}

#[test]
fn valid_view_updates_still_apply_and_return_the_complete_settings() {
    let mut s = new_session();
    let node = execute(&mut s, "graph.node.add", &json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap();
    let result = execute(&mut s, "view.2d", &json!({"node": node, "channel": "r", "tiled": true, "histogram": true})).unwrap();
    assert_eq!(result["node"], node);
    assert_eq!(result["channel"], "r");
    assert_eq!(result["tiled"], true);
    assert_eq!(result["histogram"], true);
    assert_eq!(result, serde_json::to_value(&s.view2d).unwrap());
    let result = execute(&mut s, "view.3d", &json!({"scene": "cube", "environment": "night", "yaw": 370, "target": [1, 2, 3]})).unwrap();
    assert_eq!(result["scene"], "cube");
    assert_eq!(result["environment"], "night");
    assert_eq!(result["yaw"], 10.0);
    assert_eq!(result["target"], json!([1.0, 2.0, 3.0]));
    assert_eq!(result, serde_json::to_value(&s.view3d).unwrap());
}

#[test]
fn nested_3d_settings_merge_sanitize_and_reject_as_a_whole() {
    let mut s = new_session();
    // Only the keys given change; the rest of the sub-struct stays.
    let v = execute(&mut s, "view.3d", &json!({"camera": {"focalLength": 50}, "lights": {"point": [{"on": true, "intensity": 3}]}})).unwrap();
    assert_eq!(v["camera"]["focalLength"], 50.0);
    assert_eq!(v["camera"]["projection"], "perspective");
    assert_eq!(v["lights"]["point"][0]["on"], true);
    assert_eq!(v["lights"]["point"][0]["intensity"], 3.0);
    assert_eq!(v["lights"]["point"][1]["on"], false);
    let before = s.view3d.clone();
    // A malformed nested object rejects the whole request, scalars included.
    assert!(execute(&mut s, "view.3d", &json!({"yaw": 10, "camera": "nope"})).is_err());
    assert!(execute(&mut s, "view.3d", &json!({"yaw": 10, "material": {"shader": "unknown"}})).is_err());
    assert_eq!(s.view3d, before);
    // Hostile values are clamped, not kept.
    let v = execute(&mut s, "view.3d", &json!({"camera": {"focalLength": 1e30, "near": -5, "far": 1e-9}, "material": {"common": {"ior": f64::NAN, "baseColor": [99.0, -1.0, 0.5]}, "tessellation": 1e9}})).unwrap();
    assert_eq!(v["camera"]["focalLength"], 1200.0);
    assert!(v["camera"]["far"].as_f64().unwrap() > v["camera"]["near"].as_f64().unwrap());
    assert_eq!(v["material"]["common"]["ior"], 1.5);
    assert_eq!(v["material"]["common"]["baseColor"], json!([64.0, 0.0, 0.5]));
    assert_eq!(v["material"]["tessellation"], 4.0);
    // Reset one part leaves the others.
    execute(&mut s, "view.3d", &json!({"scene": "torus"})).unwrap();
    execute(&mut s, "view.3d.reset", &json!({"what": "camera"})).unwrap();
    assert_eq!(s.view3d.camera, materialcraft_engine::Camera3d::default());
    assert_eq!(s.view3d.scene, "torus");
    assert!(execute(&mut s, "view.3d.reset", &json!({"what": "nope"})).is_err());
}

#[test]
fn state_round_trips_and_garbage_state_falls_back_to_defaults() {
    let mut s = new_session();
    execute(&mut s, "view.3d", &json!({"scene": "cloth", "environment": "warehouse", "exposure": 1.5, "lights": {"ambient": [0.2, 0.2, 0.2]}})).unwrap();
    let state = execute(&mut s, "view.3d.state", &json!({})).unwrap();
    execute(&mut s, "view.3d.reset", &json!({})).unwrap();
    assert_eq!(s.view3d.scene, "rounded_cube");
    execute(&mut s, "view.3d.loadState", &json!({"state": state, "keepScene": true})).unwrap();
    assert_eq!(s.view3d.scene, "rounded_cube", "keepScene leaves the mesh");
    assert_eq!(s.view3d.environment, "warehouse");
    assert_eq!(s.view3d.exposure, 1.5);
    execute(&mut s, "view.3d.loadState", &json!({"state": {"scene": "nope", "environment": "nope", "yaw": f64::INFINITY, "distance": 1e9}})).unwrap();
    assert_eq!(s.view3d.scene, "rounded_cube");
    assert_eq!(s.view3d.environment, "studio");
    assert_eq!(s.view3d.distance, 20.0);
    assert!(execute(&mut s, "view.3d.loadState", &json!({"state": 5})).is_err());
    assert!(execute(&mut s, "view.3d.loadState", &json!({})).is_err());
}

#[test]
fn session_settings_bound_history_cache_and_cook_size() {
    let mut s = new_session();
    let v = execute(&mut s, "session.settings", &json!({"historySize": 3, "cacheMb": 1, "cookLimit": 100})).unwrap();
    assert_eq!(v["historySize"], 3);
    assert_eq!(v["cacheMb"], 16, "the cache keeps at least 16 MB");
    assert_eq!(v["cookLimit"], 256);
    for _ in 0..6 {
        execute(&mut s, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    }
    assert_eq!(s.history().len(), 3, "only 3 steps are kept");
    assert_eq!(execute(&mut s, "session.settings", &json!({"cookLimit": 0})).unwrap()["cookLimit"], 0);
}
