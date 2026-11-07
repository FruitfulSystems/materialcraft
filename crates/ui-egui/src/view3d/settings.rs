//! The renderer's `Settings` for a 3D view state, and the viewport render scale

use materialcraft_render::{Background, Settings};

use crate::MaterialCraftApp;

/// The renderer's settings for the view state
pub fn settings_of(v: &materialcraft_engine::View3d, show_env: bool, supersample: u32) -> Settings {
    let cam = &v.camera;
    let bg = [cam.background_color[0], cam.background_color[1], cam.background_color[2]];
    let background = match cam.background.as_str() {
        "color" => Background::Color(bg),
        "environment" => Background::Environment,
        _ => Background::Backdrop,
    };
    let lights = &v.lights;
    Settings {
        yaw: v.yaw,
        pitch: v.pitch,
        distance: v.distance,
        target: v.target,
        tiling: v.tiling * v.material.uv_scale[0].max(0.01),
        exposure: v.exposure,
        env_rotation: v.env_rotation,
        normal_y_flip: v.normal_y_flip,
        show_env,
        supersample: supersample.min(v.renderer.samples.max(1)),
        ao_rays: v.renderer.ao_rays,
        transparency: v.transparency,
        alpha_view: v.alpha_view,
        fov: cam.fov(),
        ortho: cam.projection == "orthographic",
        background,
        clip: if cam.auto_clip { None } else { Some([cam.near, cam.far]) },
        ambient: if lights.ambient_on { lights.ambient } else { [0.0; 3] },
        lights: lights.point,
        env_light: v.env_enabled,
        shader: v.material.shader,
        common: v.material.common,
        use_map: v.material.use_map,
        wireframe: v.display.wireframe,
        backface_culling: v.display.backface_culling,
        ..Settings::default()
    }
}

/// The viewport render scale: the session's renderer setting, else the preference (0 = auto = 1)
pub(super) fn view_scale(app: &MaterialCraftApp) -> f32 {
    let r = &app.session.view3d.renderer;
    if !r.use_viewport_resolution {
        return r.scale;
    }
    let p = app.ui.prefs.viewport_scale;
    if p > 0.0 { p.clamp(0.25, 2.0) } else { r.scale.clamp(0.25, 2.0) }
}

/// The full-quality frame's samples per pixel: the preference, within the renderer's limit
pub(super) fn samples(app: &MaterialCraftApp) -> u32 {
    app.ui.prefs.sample_count.clamp(1, 4)
}
