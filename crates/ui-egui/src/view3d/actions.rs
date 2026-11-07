//! What the 3D view's menus do beyond one `view.3d` call: property pages, framing, resets,
//! state files, saving and copying a render

use egui::ColorImage;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Which 3D view property page the Properties panel shows (Edit properties)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page3d {
    Camera,
    Lights,
    Environment,
    Renderer,
    Material,
}

impl Page3d {
    pub fn title(self) -> &'static str {
        match self {
            Page3d::Camera => "Camera",
            Page3d::Lights => "Lights",
            Page3d::Environment => "Environment",
            Page3d::Renderer => "Renderer",
            Page3d::Material => "Material",
        }
    }
}

/// Open a 3D view property page in the Properties panel
pub fn edit_properties(app: &mut MaterialCraftApp, page: Page3d) {
    app.props3d = Some((page, app.session.selection.clone()));
    app.ui.dock.focus(crate::docking::Panel::Properties);
}

/// Camera ▸ Frame (F): fit the mesh in the view
pub fn frame(app: &mut MaterialCraftApp) {
    app.run("view.3d", json!({"distance": 3.2, "target": [0.0, 0.0, 0.0]}));
}

/// Camera ▸ Reset Position (R)
pub fn reset_position(app: &mut MaterialCraftApp) {
    app.run("view.3d", json!({"yaw": 35.0, "pitch": 25.0, "distance": 3.2, "target": [0.0, 0.0, 0.0]}));
}

/// Scene ▸ Reset Scene: the saved default state, else the built-in one
pub fn reset_all(app: &mut MaterialCraftApp) {
    match app.ui.view3d_default.clone() {
        Some(state) => {
            app.run("view.3d.loadState", json!({"state": state}));
        }
        None => {
            app.run("view.3d.reset", json!({}));
        }
    }
}

/// Scene ▸ Save Current State: the 3D view's state as a JSON file
pub(super) fn save_state(app: &mut MaterialCraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(state) = app.run("view.3d.state", json!({})) else { return };
        let Some(p) = crate::windows::dialog(app).set_file_name("3d-view-state.json").add_filter("3D view state", &["json"]).save_file() else { return };
        crate::windows::remember_dir(app, &p);
        match serde_json::to_string_pretty(&state).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string())) {
            Ok(()) => app.toast(format!("{} {}", tr("Saved"), p.display()), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Could not save the state")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    app.toast(tr("The desktop app saves state files"), true);
}

/// Scene ▸ Load State File: restore a saved state (`with_mesh` also switches the scene)
pub(super) fn load_state(app: &mut MaterialCraftApp, with_mesh: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(p) = crate::windows::dialog(app).add_filter("3D view state", &["json"]).pick_file() else { return };
        crate::windows::remember_dir(app, &p);
        const MAX: u64 = 4 << 20;
        if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MAX {
            app.toast(tr("That is not a 3D view state file"), true);
            return;
        }
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
            Ok(state) => {
                if app.run("view.3d.loadState", json!({"state": state, "keepScene": !with_mesh})).is_some() {
                    app.toast(format!("{} {}", tr("Loaded"), p.display()), false);
                }
            }
            Err(e) => app.toast(format!("{}: {e}", tr("Could not read the state file")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = with_mesh;
        app.toast(tr("The desktop app loads state files"), true);
    }
}

/// Camera ▸ Save Render (Alt+S): the view at the camera's render resolution, as a PNG
pub fn save_render(app: &mut MaterialCraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let name = format!("{}_render.png", materialcraft_engine::formats::safe_file_name(&app.session.graph().map(|g| g.name.clone()).unwrap_or_default()));
        let Some(p) = crate::windows::dialog(app).set_file_name(name).add_filter("PNG", &["png"]).save_file() else { return };
        crate::windows::remember_dir(app, &p);
        let Some((w, h, px)) = super::render_offscreen(app) else { return };
        match crate::control::encode_png(w, h, px)
            .ok_or_else(|| tr("could not encode the image").to_string())
            .and_then(|(_, _, png)| std::fs::write(&p, png).map_err(|e| e.to_string()))
        {
            Ok(()) => app.toast(format!("{} {}", tr("Render saved to"), p.display()), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Could not save the render")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let Some((w, h, px)) = super::render_offscreen(app) else { return };
        if let Some((_, _, png)) = crate::control::encode_png(w, h, px) {
            let _ = crate::web_files::download("render.png", &png);
        }
    }
}

/// Camera ▸ Copy Render to Clipboard
pub fn copy_render(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some((w, h, px)) = super::render_offscreen(app) else { return };
    let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
    ctx.copy_image(img);
    app.toast(tr("Render copied to the clipboard"), false);
}
