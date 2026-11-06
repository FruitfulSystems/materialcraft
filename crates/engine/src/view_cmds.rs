//! View commands: the 2D and 3D views, the preview size, the library listing and inspection.

use serde_json::{Value as J, json};

use materialcraft_core::NodeId;

use crate::commands::{CommandSpec, always, bad, boolp, f32p, has_graph, strp, u64p};
use crate::{Result, Session, cmd};

/// Built-in 3D view scenes: (id, label).
pub const SCENES: &[(&str, &str)] = &[
    ("cube", "Cube"),
    ("cylinder", "Cylinder"),
    ("plane", "Plane"),
    ("plane_hires", "Plane (hi-res)"),
    ("sphere", "Sphere"),
    ("cloth", "Cloth"),
    ("material_ball", "Mat ball"),
    ("rounded_cube", "Rounded cube"),
    ("rounded_cylinder", "Rounded cylinder"),
    ("sphere_2_tiles", "Sphere 2 tiles"),
    ("torus", "Torus"),
];

/// Environments: (id, label).
pub const ENVIRONMENTS: &[(&str, &str)] =
    &[("studio", "Studio"), ("contrast", "Studio contrast"), ("warehouse", "Warehouse"), ("sunset", "Sunset"), ("overcast", "Overcast"), ("night", "Night")];

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("view.2d", "View output in 2D view", [], None, "{node?, port?, tiled?, channel? (rgba|r|g|b|a), histogram?}", has_graph, view2d),
        cmd!(
            "view.3d",
            "3D view settings",
            [],
            None,
            "{scene? (cube|cylinder|plane|plane_hires|sphere|cloth|material_ball|rounded_cube|rounded_cylinder|sphere_2_tiles|torus), tiling?, yaw?, pitch?, distance?, target? ([x, y, z] orbit point in mesh radii from its center, within the pan limit preference), environment? (studio|sunset|overcast|night|contrast|warehouse), envRotation?, exposure?, envEnabled?, heightScale? (displacement depth in mesh radii, 0..0.5), heightLevel? (height that stays on the surface, 0..1; 0 = rises out), normalYFlip? (DirectX normal maps; default false/OpenGL), axis?, grid?, transparency? (see through where alpha < 1), alphaView? (show the alpha channel), camera? ({focalLength, projection, background, backgroundColor, overrideResolution, resolution, resolutionScale, autoClip, near, far}: the keys given change), lights? ({ambientOn, ambient, point: [{on, color, intensity, position}, …]}), renderer? ({useViewportResolution, resolution, scale, samples, aoRays}), display? ({wireframe, backfaceCulling, boundingBox, lights, resolution, stats, fps}), material? ({shader, common: {…}, useMap: {…}, tessellation, uvScale})} returns the whole 3D view state",
            always,
            view3d
        ),
        cmd!("view.3d.loadMesh", "Load mesh...", [], None, "{path} shows an OBJ, glTF or GLB mesh in the 3D view (scene \"custom\")", always, load_mesh),
        cmd!("view.3d.reset", "Reset scene", [], None, "{what?: all|scene|camera|lights|environment|material|renderer|display} (default all)", always, reset3d),
        cmd!("view.3d.state", "3D view state", [], None, "{} the whole 3D view state as JSON (Scene ▸ Save current state)", always, |s, _| Ok(
            serde_json::to_value(&s.view3d).unwrap_or(J::Null)
        )),
        cmd!(
            "view.3d.loadState",
            "Load 3D view state",
            [],
            None,
            "{state: JSON from view.3d.state, keepScene?: bool} restores the 3D view",
            always,
            load_state
        ),
        cmd!("view.previewSize", "Preview size", [], None, "{size: 64..8192, or 0 for full size} caps node outputs while editing", always, preview_size),
        cmd!(
            "session.settings",
            "Session settings",
            [],
            None,
            "{historySize? (1..10000 undo steps), cacheMb? (16..65536, the cooked image cache), cookLimit? (256..8192 px, or 0 for none: caps every cook, exports included)} sets what is given; returns all three",
            always,
            session_settings
        ),
        cmd!(
            "library.list",
            "Library",
            [],
            None,
            "{filter?, category?} every node definition: id, label, category, inputs, outputs, parameters",
            always,
            library
        ),
        cmd!(
            "session.inspect",
            "Inspect",
            [],
            None,
            "{graph?} the package, the open graph (nodes, wires, parameters, errors, timings), views and history",
            always,
            |s, p| { Ok(crate::inspect::session(s, u64p(p, "graph").map(|g| g as usize))) }
        ),
    ]
}

fn view2d(s: &mut Session, p: &J) -> Result<J> {
    // Validate the channel before applying any part of the request.
    let channel = strp(p, "channel");
    if channel.is_some_and(|c| !["rgba", "r", "g", "b", "a"].contains(&c)) {
        return Err(bad("view.2d", "`channel` is rgba, r, g, b or a"));
    }
    if let Some(n) = u64p(p, "node") {
        s.graph()?.node(NodeId(n)).map_err(|_| bad("view.2d", format!("no node {n}")))?;
        s.view2d.node = Some(NodeId(n));
        s.view2d.port = 0;
    }
    if let Some(port) = u64p(p, "port") {
        s.view2d.port = port as usize;
    }
    if let Some(t) = boolp(p, "tiled") {
        s.view2d.tiled = t;
    }
    if let Some(h) = boolp(p, "histogram") {
        s.view2d.histogram = h;
    }
    if let Some(c) = channel {
        s.view2d.channel = c.into();
    }
    Ok(serde_json::to_value(&s.view2d).unwrap_or(J::Null))
}

/// Overlay `patch` on `base`: objects merge key by key, arrays element by element (a shorter
/// patch leaves the rest), a `null` leaves the value alone, anything else replaces.
fn merge_json(base: &mut J, patch: &J) {
    match (base, patch) {
        (_, J::Null) => {}
        (J::Object(b), J::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(slot) => merge_json(slot, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (J::Array(b), J::Array(p)) => {
            for (i, v) in p.iter().enumerate() {
                match b.get_mut(i) {
                    Some(slot) => merge_json(slot, v),
                    None => b.push(v.clone()),
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// Merge a nested object of the `view.3d` request into one of the view's sub-structs.
fn merge_into<T: serde::Serialize + serde::de::DeserializeOwned>(key: &str, current: &T, patch: &J) -> Result<T> {
    if !patch.is_object() {
        return Err(bad("view.3d", format!("`{key}` is an object")));
    }
    let mut v = serde_json::to_value(current).map_err(|e| bad("view.3d", e.to_string()))?;
    merge_json(&mut v, patch);
    serde_json::from_value(v).map_err(|e| bad("view.3d", format!("`{key}`: {e}")))
}

fn reset3d(s: &mut Session, p: &J) -> Result<J> {
    let d = crate::View3d::default();
    let v = &mut s.view3d;
    match strp(p, "what").unwrap_or("all") {
        "all" => *v = d,
        "scene" => {
            v.scene = d.scene;
            v.tiling = d.tiling;
            v.height_scale = d.height_scale;
            v.height_level = d.height_level;
        }
        "camera" => {
            v.yaw = d.yaw;
            v.pitch = d.pitch;
            v.distance = d.distance;
            v.target = d.target;
            v.camera = d.camera;
        }
        "lights" => v.lights = d.lights,
        "environment" => {
            v.environment = d.environment;
            v.env_rotation = d.env_rotation;
            v.exposure = d.exposure;
            v.env_enabled = d.env_enabled;
        }
        "material" => v.material = d.material,
        "renderer" => v.renderer = d.renderer,
        "display" => v.display = d.display,
        other => return Err(bad("view.3d.reset", format!("unknown `what` `{other}`"))),
    }
    Ok(serde_json::to_value(&s.view3d).unwrap_or(J::Null))
}

fn load_state(s: &mut Session, p: &J) -> Result<J> {
    let state = p.get("state").ok_or_else(|| bad("view.3d.loadState", "`state` is required"))?;
    let mut v: crate::View3d = merge_into("state", &crate::View3d::default(), state)?;
    v.sanitize();
    if !SCENES.iter().any(|(id, _)| *id == v.scene) && !(v.scene == "custom" && s.custom_mesh.is_some()) {
        v.scene = crate::View3d::default().scene;
    }
    if !ENVIRONMENTS.iter().any(|(id, _)| *id == v.environment) {
        v.environment = crate::View3d::default().environment;
    }
    if boolp(p, "keepScene").unwrap_or(false) {
        v.scene = s.view3d.scene.clone();
    }
    v.target = s.camera.clamp_target(v.target);
    v.yaw = if v.yaw.is_finite() { v.yaw.rem_euclid(360.0) } else { 0.0 };
    v.pitch = if v.pitch.is_finite() { v.pitch.clamp(-89.0, 89.0) } else { 0.0 };
    v.distance = if v.distance.is_finite() { v.distance.clamp(1.2, 20.0) } else { 3.2 };
    v.tiling = if v.tiling.is_finite() { v.tiling.clamp(0.1, 64.0) } else { 1.0 };
    v.exposure = if v.exposure.is_finite() { v.exposure.clamp(-10.0, 10.0) } else { 0.0 };
    v.env_rotation = if v.env_rotation.is_finite() { v.env_rotation.rem_euclid(360.0) } else { 0.0 };
    v.height_scale = if v.height_scale.is_finite() { v.height_scale.clamp(0.0, 0.5) } else { 0.0 };
    v.height_level = if v.height_level.is_finite() { v.height_level.clamp(0.0, 1.0) } else { 0.0 };
    s.view3d = v;
    Ok(serde_json::to_value(&s.view3d).unwrap_or(J::Null))
}

fn view3d(s: &mut Session, p: &J) -> Result<J> {
    // Validated up front so a bad target changes nothing.
    let target = match p.get("target") {
        None => None,
        Some(t) => {
            let xyz: Option<Vec<f32>> =
                t.as_array().filter(|a| a.len() == 3).map(|a| a.iter().filter_map(J::as_f64).map(|f| f as f32).filter(|f| f.is_finite()).collect());
            match xyz.as_deref() {
                Some(&[x, y, z]) => Some(s.camera.clamp_target([x, y, z])),
                _ => return Err(bad("view.3d", "`target` is [x, y, z]")),
            }
        }
    };
    // Nested objects are parsed first: a bad one changes nothing.
    let camera = p.get("camera").map(|c| merge_into("camera", &s.view3d.camera, c)).transpose()?;
    let lights = p.get("lights").map(|l| merge_into("lights", &s.view3d.lights, l)).transpose()?;
    let renderer = p.get("renderer").map(|r| merge_into("renderer", &s.view3d.renderer, r)).transpose()?;
    let display = p.get("display").map(|d| merge_into("display", &s.view3d.display, d)).transpose()?;
    let material = p.get("material").map(|m| merge_into("material", &s.view3d.material, m)).transpose()?;
    // A rejected environment must not leave a new scene applied.
    let environment = strp(p, "environment");
    if let Some(e) = environment
        && !ENVIRONMENTS.iter().any(|(id, _)| *id == e)
    {
        return Err(bad("view.3d", format!("unknown environment `{e}`")));
    }
    if let Some(sc) = strp(p, "scene") {
        if sc == "custom" && s.custom_mesh.is_none() {
            return Err(bad("view.3d", "no mesh is loaded: use view.3d.loadMesh"));
        }
        let v = &mut s.view3d;
        if sc != "custom" && !SCENES.iter().any(|(id, _)| *id == sc) {
            return Err(bad("view.3d", format!("unknown scene `{sc}`")));
        }
        v.scene = sc.into();
    }
    let v = &mut s.view3d;
    if let Some(e) = environment {
        v.environment = e.into();
    }
    if let Some(t) = f32p(p, "tiling") {
        v.tiling = t.clamp(0.1, 64.0);
    }
    if let Some(t) = f32p(p, "yaw") {
        v.yaw = t.rem_euclid(360.0);
    }
    if let Some(t) = f32p(p, "pitch") {
        v.pitch = t.clamp(-89.0, 89.0);
    }
    if let Some(t) = f32p(p, "distance") {
        v.distance = t.clamp(1.2, 20.0);
    }
    if let Some(t) = target {
        v.target = t;
    }
    if let Some(t) = f32p(p, "envRotation") {
        v.env_rotation = t.rem_euclid(360.0);
    }
    if let Some(t) = f32p(p, "exposure") {
        v.exposure = t.clamp(-10.0, 10.0);
    }
    if let Some(t) = f32p(p, "heightScale") {
        v.height_scale = t.clamp(0.0, 0.5);
    }
    if let Some(t) = f32p(p, "heightLevel") {
        v.height_level = t.clamp(0.0, 1.0);
    }
    if let Some(flip) = boolp(p, "normalYFlip") {
        v.normal_y_flip = flip;
    }
    if let Some(b) = boolp(p, "axis") {
        v.show_axis = b;
    }
    if let Some(b) = boolp(p, "grid") {
        v.show_grid = b;
    }
    if let Some(b) = boolp(p, "transparency") {
        v.transparency = b;
    }
    if let Some(b) = boolp(p, "alphaView") {
        v.alpha_view = b;
    }
    if let Some(b) = boolp(p, "envEnabled") {
        v.env_enabled = b;
    }
    if let Some(c) = camera {
        v.camera = c;
    }
    if let Some(l) = lights {
        v.lights = l;
    }
    if let Some(r) = renderer {
        v.renderer = r;
    }
    if let Some(d) = display {
        v.display = d;
    }
    if let Some(m) = material {
        v.material = m;
    }
    v.sanitize();
    Ok(serde_json::to_value(&s.view3d).unwrap_or(J::Null))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_mesh(s: &mut Session, p: &J) -> Result<J> {
    let path = strp(p, "path").ok_or_else(|| bad("view.3d.loadMesh", "`path` is required"))?;
    let bytes = std::fs::read(path).map_err(|e| bad("view.3d.loadMesh", format!("{path}: {e}")))?;
    let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string();
    s.load_mesh_bytes(&name, &bytes)?;
    let tris = s.custom_mesh.as_ref().map_or(0, |(_, m)| m.triangle_count());
    Ok(json!({"name": name, "triangles": tris}))
}

#[cfg(target_arch = "wasm32")]
fn load_mesh(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.loadMesh", "the browser loads meshes with the file picker"))
}

fn preview_size(s: &mut Session, p: &J) -> Result<J> {
    let size = u64p(p, "size").ok_or_else(|| bad("view.previewSize", "`size` is required"))?;
    s.preview_size = if size == 0 { None } else { Some(size.clamp(64, 8192) as u32) };
    s.dirty = true;
    Ok(json!({"previewSize": s.preview_size}))
}

fn session_settings(s: &mut Session, p: &J) -> Result<J> {
    if let Some(n) = u64p(p, "historySize") {
        s.max_undo = n.clamp(1, 10_000) as usize;
        s.trim_undo();
    }
    if let Some(mb) = u64p(p, "cacheMb") {
        let bytes = (mb.clamp(16, 65_536) as usize) << 20;
        if let Ok(mut c) = s.cache.lock() {
            c.set_cap(bytes);
        }
    }
    if let Some(px) = u64p(p, "cookLimit") {
        let limit = if px == 0 { None } else { Some(px.clamp(256, 8192) as u32) };
        if limit != s.cook_limit {
            s.cook_limit = limit;
            s.dirty = true;
        }
    }
    let cache_mb = s.cache.lock().map(|c| c.cap() >> 20).unwrap_or(0);
    Ok(json!({"historySize": s.max_undo, "cacheMb": cache_mb, "cookLimit": s.cook_limit.unwrap_or(0)}))
}

fn library(_: &mut Session, p: &J) -> Result<J> {
    let f = strp(p, "filter").map(str::to_ascii_lowercase);
    let cat = strp(p, "category").map(str::to_ascii_lowercase);
    Ok(J::Array(
        materialcraft_nodes::catalog()
            .iter()
            .filter(|d| f.as_ref().is_none_or(|f| d.id.contains(f.as_str()) || d.label.to_ascii_lowercase().contains(f.as_str())))
            .filter(|d| cat.as_ref().is_none_or(|c| d.category.to_ascii_lowercase().contains(c.as_str())))
            .map(|d| {
                json!({
                    "id": d.id, "label": d.label, "category": d.category, "description": d.description,
                    "inputs": d.inputs.iter().map(|p| json!({"id": p.id, "label": p.label, "kind": format!("{:?}", p.kind)})).collect::<Vec<_>>(),
                    "outputs": d.outputs.iter().map(|p| json!({"id": p.id, "label": p.label, "kind": format!("{:?}", p.kind)})).collect::<Vec<_>>(),
                    "params": d.params.iter().map(|p| json!({"id": p.id, "label": p.label, "default": p.default, "min": p.min, "max": p.max, "options": p.options})).collect::<Vec<_>>(),
                })
            })
            .collect(),
    ))
}
