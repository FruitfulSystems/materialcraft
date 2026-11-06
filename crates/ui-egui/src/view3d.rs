//! The 3D view: the open graph's material on a built-in scene, rendered by
//! `materialcraft-render` (one ray per pixel at half resolution while the camera moves, 2×2
//! supersampled when it rests). Left-drag orbits, Alt+drag or middle-drag pans, the wheel zooms,
//! Shift+drag turns the light and a double-click resets the camera.
//!
//! The full-quality frame waits until the view's size and the camera have rested for [`SETTLE`]
//! (a panel, divider or window drag changes the size on every step), then renders on a worker
//! thread so the UI never waits for it; until it lands the last frame stays up, kept in proportion.
//! A frame that is no longer wanted is cancelled. The browser has no threads and renders here.
//!
//! The height output raises the mesh for real ([`Scene::with_relief`]), built on a worker thread and cached until its inputs change.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use egui::{Color32, ColorImage, Rect, RichText, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::view_cmds::{ENVIRONMENTS, SCENES};
use materialcraft_render::{Lighting, Maps, Relief, Scene, Settings};
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Seconds the view's size and the camera rest before the full-quality frame renders.
pub const SETTLE: f64 = 0.15;

/// Triangle budget of the displaced mesh; the browser builds it on the UI thread, so it gets a smaller one.
const RELIEF_TRIANGLES: usize = if cfg!(target_arch = "wasm32") { 120_000 } else { materialcraft_render::VIEW_TRIANGLES };

#[derive(Default)]
pub struct View3dState {
    scene: Option<(String, Arc<Scene>)>,
    /// The scene displaced by the height output, keyed by what it was built from (see [`relief_key`]).
    displaced: Option<(u64, Arc<Scene>)>,
    /// The displaced scene building on a worker thread.
    relief_job: Option<ReliefJob>,
    light: Option<Arc<Lighting>>,
    tex: Option<TextureHandle>,
    /// What the texture shows: (content key, full quality).
    key: Option<(u64, bool)>,
    /// The view size (points) the texture was rendered for.
    tex_size: Option<(u32, u32)>,
    /// The view size on the last frame.
    size: Option<(u32, u32)>,
    /// When the view's size or the camera last changed (app seconds).
    changed_at: Option<f64>,
    /// The full-quality frame rendering on a worker thread.
    job: Option<Job>,
    pub render_ms: f32,
    /// Frames rendered so far (previews and full quality).
    pub renders: u64,
    /// The last frame shown (RGBA8, for `view.screenshot`).
    pub shown: Option<(u32, u32, Vec<u8>)>,
    /// Show the environment behind the mesh.
    pub show_env: bool,
}

impl View3dState {
    /// A frame or its displaced geometry is still being built in the background.
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.relief_job.is_some()
    }
}

/// A rendered frame: RGBA8 pixels and how long they took.
struct Frame {
    w: u32,
    h: u32,
    px: Vec<u8>,
    ms: f32,
}

/// A full-quality frame rendering on a worker thread. Dropping it cancels the render.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Job {
    key: u64,
    size: (u32, u32),
    cancel: Arc<AtomicBool>,
    rx: Receiver<Option<Frame>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A displaced scene building on a worker thread (`None` arrives when nothing moves).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct ReliefJob {
    key: u64,
    rx: Receiver<Option<Arc<Scene>>>,
}

pub fn scene_label(app: &MaterialCraftApp) -> String {
    let id = &app.session.view3d.scene;
    if id == "custom"
        && let Some((name, _)) = &app.session.custom_mesh
    {
        return name.clone();
    }
    SCENES.iter().find(|(s, _)| s == id).map_or(id.as_str(), |(_, l)| l).to_string()
}

/// Which 3D view property page the Properties panel shows (Edit properties).
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

/// Open a 3D view property page in the Properties panel.
pub fn edit_properties(app: &mut MaterialCraftApp, page: Page3d) {
    app.props3d = Some((page, app.session.selection.clone()));
    app.ui.dock.focus(crate::docking::Panel::Properties);
}

fn menu_row(ui: &mut Ui, ic: &str, label: &str) -> bool {
    ui.add(egui::Button::new(format!("{ic}  {label}"))).clicked()
}

fn menus(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tk.panel).inner_margin(egui::Margin::symmetric(4, 1)).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("Scene"), |ui| {
                for (i, (id, label)) in SCENES.iter().enumerate() {
                    if i == 5 {
                        ui.separator();
                    }
                    if ui.selectable_label(app.session.view3d.scene == *id, tr(label)).clicked() {
                        app.run("view.3d", json!({"scene": id}));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("Load mesh...")).clicked() {
                    crate::web_files::load_mesh(app, ui.ctx());
                    ui.close();
                }
                if app.session.custom_mesh.is_some() && ui.selectable_label(app.session.view3d.scene == "custom", scene_label_custom(app)).clicked() {
                    app.run("view.3d", json!({"scene": "custom"}));
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Display UVs in 2D View")).clicked() {
                    app.view2d.show_uvs = !app.view2d.show_uvs;
                    app.ui.dock.focus(crate::docking::Panel::View2d);
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Load State File...")).clicked() {
                    load_state(app, false);
                    ui.close();
                }
                if ui.button(tr("Load State File With Mesh...")).clicked() {
                    load_state(app, true);
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Save Current State...")).clicked() {
                    save_state(app);
                    ui.close();
                }
                if ui.button(tr("Save Current State As Default")).clicked() {
                    app.ui.view3d_default = app.run("view.3d.state", json!({}));
                    app.toast(tr("The 3D view opens with this state from now on (Reset Scene restores it)"), false);
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Reset Scene")).clicked() {
                    reset_all(app);
                    ui.close();
                }
            });
            ui.menu_button(tr("Materials"), |ui| {
                ui.menu_button(tr("Default"), |ui| {
                    if menu_row(ui, icon::GEAR, tr("Edit properties")) {
                        edit_properties(app, Page3d::Material);
                        ui.close();
                    }
                    ui.separator();
                    let current = app.session.view3d.material.shader;
                    for (shader, label) in [
                        (materialcraft_render::Shader::Blinn, "Blinn"),
                        (materialcraft_render::Shader::Lambert, "Lambert"),
                        (materialcraft_render::Shader::MeshInfo, "Mesh Info"),
                        (materialcraft_render::Shader::OpenPbr, "OpenPBR [Default]"),
                        (materialcraft_render::Shader::MetallicRoughness, "Metallic Roughness"),
                        (materialcraft_render::Shader::Coated, "Metallic Roughness - Coated"),
                        (materialcraft_render::Shader::Subsurface, "Metallic Roughness - SSS"),
                        (materialcraft_render::Shader::SpecularGlossiness, "Specular Glossiness"),
                        (materialcraft_render::Shader::Unlit, "Unlit"),
                    ] {
                        if ui.selectable_label(current == shader, tr(label)).clicked() {
                            app.run("view.3d", json!({"material": {"shader": shader.id()}}));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Reset Common Parameters")).clicked() {
                        let d = materialcraft_render::Common::default();
                        app.run("view.3d", json!({"material": {"common": d, "useMap": materialcraft_render::MapUse::default()}}));
                        ui.close();
                    }
                    if menu_row(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset Material")) {
                        app.run("view.3d.reset", json!({"what": "material"}));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr("Create graph from texture inputs")).clicked() {
                        crate::menus::invoke(app, ui.ctx(), "ui.newGraph");
                        ui.close();
                    }
                });
            });
            ui.menu_button(tr("Lights"), |ui| {
                if menu_row(ui, icon::GEAR, tr("Edit properties")) {
                    edit_properties(app, Page3d::Lights);
                    ui.close();
                }
                if menu_row(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset Lights")) {
                    app.run("view.3d.reset", json!({"what": "lights"}));
                    ui.close();
                }
            });
            ui.menu_button(tr("Camera"), |ui| {
                if menu_row(ui, icon::GEAR, tr("Edit properties")) {
                    edit_properties(app, Page3d::Camera);
                    ui.close();
                }
                if ui.add(egui::Button::new(tr("Frame")).shortcut_text("F")).clicked() {
                    frame(app);
                    ui.close();
                }
                ui.label(RichText::new(tr("Viewpoint")).italics().color(tk.text_dim));
                for (label, yaw, pitch) in [
                    ("Default", 35.0, 25.0),
                    ("Front", 0.0, 0.0),
                    ("Right", 90.0, 0.0),
                    ("Back", 180.0, 0.0),
                    ("Left", 270.0, 0.0),
                    ("Top", 0.0, 89.0),
                    ("Bottom", 0.0, -89.0),
                ] {
                    if ui.button(format!("    {}", tr(label))).clicked() {
                        app.run("view.3d", json!({"yaw": yaw, "pitch": pitch, "distance": 3.2, "target": [0.0, 0.0, 0.0]}));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.add(egui::Button::new(format!("{}  {}", icon::FLOPPY_DISK, tr("Save Render..."))).shortcut_text("Alt+S")).clicked() {
                    save_render(app, ui.ctx());
                    ui.close();
                }
                if menu_row(ui, icon::CLIPBOARD, tr("Copy Render to Clipboard")) {
                    copy_render(app, ui.ctx());
                    ui.close();
                }
                ui.separator();
                if ui.add(egui::Button::new(tr("Reset Position")).shortcut_text("R")).clicked() {
                    app.run("view.3d", json!({"yaw": 35.0, "pitch": 25.0, "distance": 3.2, "target": [0.0, 0.0, 0.0]}));
                    ui.close();
                }
                if menu_row(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset Camera")) {
                    app.run("view.3d.reset", json!({"what": "camera"}));
                    ui.close();
                }
            });
            ui.menu_button(tr("Environment"), |ui| {
                if menu_row(ui, icon::GEAR, tr("Edit properties")) {
                    edit_properties(app, Page3d::Environment);
                    ui.close();
                }
                if menu_row(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset Environment")) {
                    app.run("view.3d.reset", json!({"what": "environment"}));
                    ui.close();
                }
                ui.separator();
                for (id, label) in ENVIRONMENTS {
                    if ui.selectable_label(app.session.view3d.environment == *id, tr(label)).clicked() {
                        app.run("view.3d", json!({"environment": id}));
                        ui.close();
                    }
                }
            });
            ui.menu_button(tr("Display"), |ui| {
                let d = app.session.view3d.display.clone();
                let mut toggles: Vec<(&str, bool)> = Vec::new();
                for (key, label, on) in [
                    ("wireframe", "Wireframe", d.wireframe),
                    ("axis", "Axis", app.session.view3d.show_axis),
                    ("backfaceCulling", "Backface culling", d.backface_culling),
                    ("boundingBox", "Bounding box", d.bounding_box),
                    ("grid", "Grid", app.session.view3d.show_grid),
                    ("lights", "Light", d.lights),
                ] {
                    if ui.selectable_label(on, tr(label)).clicked() {
                        toggles.push((key, !on));
                    }
                }
                ui.separator();
                for (key, label, on) in [("resolution", "Resolution", d.resolution), ("stats", "Scene stats", d.stats), ("fps", "FPS", d.fps)] {
                    if ui.selectable_label(on, tr(label)).clicked() {
                        toggles.push((key, !on));
                    }
                }
                ui.separator();
                let mut flip = app.session.view3d.normal_y_flip;
                if ui.checkbox(&mut flip, tr("Flip normal map Y (DirectX)")).changed() {
                    app.run("view.3d", json!({"normalYFlip": flip}));
                }
                let mut t = app.session.view3d.transparency;
                if ui.checkbox(&mut t, tr("Transparency (alpha)")).changed() {
                    app.run("view.3d", json!({"transparency": t}));
                }
                let mut a = app.session.view3d.alpha_view;
                if ui.checkbox(&mut a, tr("Show alpha channel")).changed() {
                    app.run("view.3d", json!({"alphaView": a}));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Preview size"));
                    crate::view2d::resolution_menu(app, ui);
                });
                for (key, on) in toggles {
                    match key {
                        "axis" => {
                            app.run("view.3d", json!({"axis": on}));
                        }
                        "grid" => {
                            app.run("view.3d", json!({"grid": on}));
                        }
                        _ => {
                            app.run("view.3d", json!({"display": {key: on}}));
                        }
                    }
                }
            });
            ui.menu_button(tr("Renderer"), |ui| {
                if menu_row(ui, icon::GEAR, tr("Edit settings")) {
                    edit_properties(app, Page3d::Renderer);
                    ui.close();
                }
                ui.separator();
                let _ = ui.selectable_label(true, tr("MC Ray caster (CPU)"));
                ui.add_enabled(false, egui::Button::new(tr("GPU path tracer (not supported yet)")));
            });
        });
    });
}

/// Camera ▸ Frame (F): fit the mesh in the view.
pub fn frame(app: &mut MaterialCraftApp) {
    app.run("view.3d", json!({"distance": 3.2, "target": [0.0, 0.0, 0.0]}));
}

/// Scene ▸ Reset Scene: the saved default state, else the built-in one.
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

/// Scene ▸ Save Current State: the 3D view's state as a JSON file.
fn save_state(app: &mut MaterialCraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(state) = app.run("view.3d.state", json!({})) else { return };
        let Some(p) = rfd::FileDialog::new().set_file_name("3d-view-state.json").add_filter("3D view state", &["json"]).save_file() else { return };
        match serde_json::to_string_pretty(&state).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string())) {
            Ok(()) => app.toast(format!("{} {}", tr("Saved"), p.display()), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Could not save the state")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    app.toast(tr("The desktop app saves state files"), true);
}

/// Scene ▸ Load State File: restore a saved state (`with_mesh` also switches the scene).
fn load_state(app: &mut MaterialCraftApp, with_mesh: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(p) = rfd::FileDialog::new().add_filter("3D view state", &["json"]).pick_file() else { return };
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

/// Camera ▸ Save Render (Alt+S): the view at the camera's render resolution, as a PNG.
pub fn save_render(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let name = format!("{}_render.png", materialcraft_engine::formats::safe_file_name(&app.session.graph().map(|g| g.name.clone()).unwrap_or_default()));
        let Some(p) = rfd::FileDialog::new().set_file_name(name).add_filter("PNG", &["png"]).save_file() else { return };
        let Some((w, h, px)) = render_offscreen(app) else { return };
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
        let _ = ctx;
        let Some((w, h, px)) = render_offscreen(app) else { return };
        if let Some((_, _, png)) = crate::control::encode_png(w, h, px) {
            let _ = crate::web_files::download("render.png", &png);
        }
    }
}

/// Camera ▸ Copy Render to Clipboard.
pub fn copy_render(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some((w, h, px)) = render_offscreen(app) else { return };
    let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
    ctx.copy_image(img);
    app.toast(tr("Render copied to the clipboard"), false);
}

/// Render the view at the camera's resolution (or the last shown size) with full quality.
fn render_offscreen(app: &mut MaterialCraftApp) -> Option<(u32, u32, Vec<u8>)> {
    let v = app.session.view3d.clone();
    let (w, h) = if v.camera.override_resolution {
        let k = v.camera.resolution_scale;
        (((v.camera.resolution[0] as f32 * k) as u32).clamp(16, 4096), ((v.camera.resolution[1] as f32 * k) as u32).clamp(16, 4096))
    } else {
        app.view3d.shown.as_ref().map_or((1280, 720), |(w, h, _)| (*w, *h))
    };
    let (scene, light, maps, settings) = inputs(app, 2)?;
    let px = materialcraft_render::render(&scene, &maps, &light, &settings, w, h);
    Some((w, h, px))
}

/// The tool strip down the left of the viewport: property pages and display toggles.
fn side_strip(app: &mut MaterialCraftApp, ui: &mut Ui, strip: Rect) {
    let tk = Tokens::get(ui.ctx());
    ui.painter().rect_filled(strip, 0.0, tk.panel);
    ui.scope_builder(egui::UiBuilder::new().max_rect(strip.shrink2(vec2(2.0, 4.0))), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let b = |ui: &mut Ui, ic: &str, tip: &str, on: bool| crate::widgets::icon_button(ui, ic, tip, on, 22.0).clicked();
            if b(ui, icon::CORNERS_OUT, tr("Frame (F)"), false) {
                frame(app);
            }
            let page = app.props3d.as_ref().map(|(p, _)| *p);
            for (ic, tip, pg) in [
                (icon::VIDEO_CAMERA, "Camera properties", Page3d::Camera),
                (icon::LIGHTBULB, "Light properties", Page3d::Lights),
                (icon::SUN_HORIZON, "Environment properties", Page3d::Environment),
                (icon::SPHERE, "Material properties", Page3d::Material),
                (icon::GEAR, "Renderer settings", Page3d::Renderer),
            ] {
                if b(ui, ic, tr(tip), page == Some(pg)) {
                    edit_properties(app, pg);
                }
            }
            ui.add_space(6.0);
            let v = app.session.view3d.clone();
            let mut set: Option<serde_json::Value> = None;
            if b(ui, icon::POLYGON, tr("Wireframe"), v.display.wireframe) {
                set = Some(json!({"display": {"wireframe": !v.display.wireframe}}));
            }
            if b(ui, icon::GRID_FOUR, tr("Grid"), v.show_grid) {
                set = Some(json!({"grid": !v.show_grid}));
            }
            if b(ui, icon::ARROWS_OUT_CARDINAL, tr("Axis"), v.show_axis) {
                set = Some(json!({"axis": !v.show_axis}));
            }
            if b(ui, icon::BOUNDING_BOX, tr("Bounding box"), v.display.bounding_box) {
                set = Some(json!({"display": {"boundingBox": !v.display.bounding_box}}));
            }
            if b(ui, icon::CUBE_TRANSPARENT, tr("Transparency (alpha)"), v.transparency) {
                set = Some(json!({"transparency": !v.transparency}));
            }
            if b(ui, icon::CHECKERBOARD, tr("Show alpha channel"), v.alpha_view) {
                set = Some(json!({"alphaView": !v.alpha_view}));
            }
            if b(ui, icon::IMAGE, tr("Environment background"), app.view3d.show_env) {
                app.view3d.show_env = !app.view3d.show_env;
            }
            if let Some(p) = set {
                app.run("view.3d", p);
            }
        });
    });
}

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    menus(app, ui);
    let avail = ui.available_size();
    let strip_w = 28.0;
    let (whole, _) = ui.allocate_exact_size(vec2(avail.x, (avail.y - 22.0).max(40.0)), Sense::hover());
    let strip = Rect::from_min_size(whole.min, vec2(strip_w, whole.height()));
    let rect = Rect::from_min_max(pos2(whole.left() + strip_w, whole.top()), whole.max);
    let resp = ui.interact(rect, ui.scope_id().with("view3d-canvas"), Sense::click_and_drag());
    crate::widgets::name(&resp, egui::Role::Image, "3D view");
    // Camera gestures.
    let mut moving = false;
    if resp.dragged() {
        let d = resp.drag_delta();
        let v = &app.session.view3d;
        if ui.input(|i| i.modifiers.alt) || resp.dragged_by(egui::PointerButton::Middle) {
            if let Some((_, scene)) = &app.view3d.scene {
                let s = Settings { yaw: v.yaw, pitch: v.pitch, distance: v.distance, target: v.target, ..Settings::default() };
                let cam = app.session.camera;
                let k = cam.pan_speed / rect.height();
                app.session.view3d.target = cam.clamp_target(materialcraft_render::pan(scene, &s, d.x * k, d.y * k));
            }
        } else if ui.input(|i| i.modifiers.shift) {
            let rot = (v.env_rotation + d.x * 0.5).rem_euclid(360.0);
            app.session.view3d.env_rotation = rot;
        } else {
            let (yaw, pitch) = (v.yaw - d.x * 0.4, (v.pitch + d.y * 0.4).clamp(-89.0, 89.0));
            app.session.view3d.yaw = yaw.rem_euclid(360.0);
            app.session.view3d.pitch = pitch;
        }
        moving = true;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y) * if app.ui.prefs.invert_zoom { -1.0 } else { 1.0 };
        if scroll.abs() > 0.0 {
            app.session.view3d.distance = (app.session.view3d.distance * (-scroll * 0.002).exp()).clamp(1.2, 20.0);
            moving = true;
        }
    }
    if resp.double_clicked() {
        app.run("view.3d", json!({"yaw": 35.0, "pitch": 25.0, "distance": 3.2, "target": [0.0, 0.0, 0.0]}));
    }
    // Keys while the pointer is over the view: F frames, R resets the position, Alt+S saves a render.
    if resp.hovered() && !ui.ctx().egui_wants_keyboard_input() {
        let (f, r, alt_s) = ui.input(|i| (i.key_pressed(egui::Key::F), i.key_pressed(egui::Key::R), i.modifiers.alt && i.key_pressed(egui::Key::S)));
        if f {
            frame(app);
        }
        if r {
            app.run("view.3d", json!({"yaw": 35.0, "pitch": 25.0, "distance": 3.2, "target": [0.0, 0.0, 0.0]}));
        }
        if alt_s {
            save_render(app, ui.ctx());
        }
    }
    render(app, ui.ctx(), rect, moving || resp.is_pointer_button_down_on());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_rgb(0x1b, 0x1c, 0x1e));
    if let Some(t) = &app.view3d.tex {
        p.image(t.id(), frame_rect(rect, app.view3d.tex_size), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    if app.session.view3d.alpha_view {
        p.text(rect.left_top() + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, tr("Alpha channel"), egui::FontId::proportional(11.0), tk.viewport_text);
    }
    if app.session.material_maps().is_empty() {
        p.text(
            rect.center_top() + vec2(0.0, 18.0),
            egui::Align2::CENTER_TOP,
            tr("Add Output nodes with usages (base color, normal, ...) to see the material"),
            egui::FontId::proportional(11.0),
            tk.viewport_text,
        );
    }
    overlays(app, &p, rect);
    p.text(
        rect.right_bottom() - vec2(8.0, 6.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{}: {:.0} ms", tr("Render time"), app.view3d.render_ms),
        egui::FontId::proportional(10.5),
        tk.viewport_text,
    );
    side_strip(app, ui, strip);
    // Bottom bar.
    let bar = Rect::from_min_size(pos2(whole.left(), whole.bottom()), vec2(whole.width(), 22.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
        ui.painter().rect_filled(bar, 0.0, tk.panel);
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(icon::SPHERE).color(tk.text_dim));
            let r = egui::ComboBox::from_id_salt("scene3d").width(110.0).selected_text(scene_label(app)).show_ui(ui, |ui| {
                for (id, label) in SCENES {
                    if ui.selectable_label(app.session.view3d.scene == *id, tr(label)).clicked() {
                        app.run("view.3d", json!({"scene": id}));
                    }
                }
            });
            crate::widgets::name(&r.response, egui::Role::ComboBox, "Scene");
            ui.label(egui::RichText::new(format!("{}  sRGB / ACES", icon::SUN)).size(11.0).color(tk.text_dim));
            let a = app.session.view3d.alpha_view;
            if crate::widgets::icon_button(ui, icon::CHECKERBOARD, tr("Show alpha channel"), a, 20.0).clicked() {
                app.run("view.3d", json!({"alphaView": !a}));
            }
        });
    });
}

/// Display menu overlays: bounding box, light gizmos, resolution, scene statistics and FPS.
fn overlays(app: &mut MaterialCraftApp, p: &egui::Painter, rect: Rect) {
    let tk = Tokens::get(p.ctx());
    let v = app.session.view3d.clone();
    let d = &v.display;
    let (bbox, lights) = (d.bounding_box, d.lights);
    if bbox || lights {
        let (_, scene) = base_scene(app);
        let settings = settings_of(&v, app.view3d.show_env, 1);
        let cam = materialcraft_render::camera(&scene, &settings);
        let aspect = rect.width() / rect.height().max(1.0);
        let to_screen = |w: materialcraft_engine::geom::glam::Vec3| {
            cam.project(w, aspect).map(|ndc| pos2(rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - ndc.y * 0.5) * rect.height()))
        };
        if bbox {
            let (lo, hi) = scene.mesh.bounds();
            let corners: Vec<_> = (0..8)
                .map(|i| {
                    materialcraft_engine::geom::glam::Vec3::new(
                        if i & 1 == 0 { lo.x } else { hi.x },
                        if i & 2 == 0 { lo.y } else { hi.y },
                        if i & 4 == 0 { lo.z } else { hi.z },
                    )
                })
                .collect();
            let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xff, 0xc0, 0x40, 200));
            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                if let (Some(pa), Some(pb)) = (to_screen(corners[a]), to_screen(corners[b])) {
                    p.line_segment([pa, pb], stroke);
                }
            }
        }
        if lights {
            for (i, l) in v.lights.point.iter().enumerate() {
                let w = scene.center + materialcraft_engine::geom::glam::Vec3::from(l.position) * scene.radius;
                if let Some(q) = to_screen(w) {
                    let c = if l.on { Color32::from_rgb(0xff, 0xe0, 0x60) } else { Color32::from_gray(0x80) };
                    p.circle(q, 6.0, c, egui::Stroke::new(1.0, Color32::BLACK));
                    p.text(q + vec2(9.0, 0.0), egui::Align2::LEFT_CENTER, format!("{} {}", tr("Light"), i + 1), egui::FontId::proportional(10.5), c);
                    if let Some(center) = to_screen(scene.center) {
                        p.line_segment([q, center], egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xff, 0xe0, 0x60, 90)));
                    }
                }
            }
        }
    }
    let mut lines: Vec<String> = Vec::new();
    if d.resolution {
        let (w, h) = app.view3d.shown.as_ref().map_or((0, 0), |(w, h, _)| (*w, *h));
        lines.push(format!("{}: {w} x {h}", tr("Resolution")));
    }
    if d.stats {
        let (_, scene) = base_scene(app);
        let tris = app.view3d.displaced.as_ref().map_or(scene.mesh.triangle_count(), |(_, s)| s.mesh.triangle_count());
        let maps = app.session.material_maps().len();
        lines.push(format!("{}: {tris} {}, {maps} {}", tr("Scene"), tr("triangles"), tr("maps")));
    }
    if d.fps {
        lines.push(format!("{:.0} FPS ({} {})", app.fps, app.view3d.renders, tr("frames")));
    }
    for (i, line) in lines.iter().enumerate() {
        p.text(
            rect.left_bottom() + vec2(8.0, -8.0 - 14.0 * (lines.len() - 1 - i) as f32),
            egui::Align2::LEFT_BOTTOM,
            line,
            egui::FontId::proportional(10.5),
            tk.viewport_text,
        );
    }
}

fn scene_label_custom(app: &MaterialCraftApp) -> String {
    app.session.custom_mesh.as_ref().map_or_else(String::new, |(n, _)| n.clone())
}

/// The view's size in whole points: part of the content key and of `tex_size`.
fn view_size(rect: Rect) -> (u32, u32) {
    (rect.width() as u32, rect.height() as u32)
}

/// Where the last frame goes in `rect`: all of it when it was rendered for this size, otherwise
/// at the view's height and centred. The camera's field of view is vertical, so that is what a
/// new render will show (cropped, or with bars at the sides) while a resize is in progress,
/// instead of a stretched picture.
fn frame_rect(rect: Rect, rendered: Option<(u32, u32)>) -> Rect {
    match rendered {
        Some((w, h)) if h > 0 && (w, h) != view_size(rect) => Rect::from_center_size(rect.center(), vec2(rect.height() * w as f32 / h as f32, rect.height())),
        _ => rect,
    }
}

fn content_key(app: &MaterialCraftApp, w: u32, h: u32, show_env: bool, relief_shown: Option<u64>) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hs = DefaultHasher::new();
    let v = &app.session.view3d;
    (w, h, show_env, &v.scene, &v.environment, app.session.custom_mesh.as_ref().map(|(n, _)| n), v.transparency, v.alpha_view, relief_shown).hash(&mut hs);
    v.normal_y_flip.hash(&mut hs);
    for f in [v.yaw, v.pitch, v.distance, v.target[0], v.target[1], v.target[2], v.tiling, v.exposure, v.env_rotation, v.height_scale, v.height_level] {
        f.to_bits().hash(&mut hs);
    }
    // Camera, lights, material, renderer and display settings: hashed through their JSON.
    serde_json::to_string(&(&v.camera, &v.lights, &v.material, &v.renderer, &v.display, v.env_enabled)).unwrap_or_default().hash(&mut hs);
    if let Ok(g) = app.session.graph() {
        for (id, usage, _, _) in materialcraft_engine::eval::outputs(g, &app.session.cooked) {
            (usage, app.session.cooked.nodes.get(&id).map(|r| r.key)).hash(&mut hs);
        }
    }
    hs.finish()
}

/// Keep the 3D view's texture up to date: a quick frame right away while the camera moves, the
/// full-quality one once the view's size and the camera have rested for [`SETTLE`].
fn render(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, moving: bool) {
    let now = ctx.input(|i| i.time);
    let size = view_size(rect);
    if moving || app.view3d.size.is_some_and(|s| s != size) {
        app.view3d.changed_at = Some(now);
    }
    app.view3d.size = Some(size);
    let relief_shown = update_relief(app, ctx);
    let key = content_key(app, size.0, size.1, app.view3d.show_env, relief_shown);
    collect(app, ctx, key);
    if app.view3d.key == Some((key, true)) || (moving && app.view3d.key == Some((key, false))) {
        return;
    }
    let ppp = ctx.pixels_per_point();
    if moving {
        // One ray per pixel at half resolution, here: the camera follows the pointer.
        if let Some(frame) = render_now(app, rect, ppp * 0.5, 1) {
            show_frame(app, ctx, frame, key, false, size);
        }
        ctx.request_repaint_after(Duration::from_secs_f64(SETTLE));
        return;
    }
    let wait = app.view3d.changed_at.map_or(0.0, |t| SETTLE - (now - t));
    if wait > 0.0 {
        // Mid-resize or the camera just stopped: the last frame stays up (see `frame_rect`).
        ctx.request_repaint_after(Duration::from_secs_f64(wait.min(SETTLE)));
        return;
    }
    if app.view3d.job.as_ref().is_some_and(|j| j.key == key) {
        return;
    }
    start_full(app, ctx, rect, key, size);
}

/// Show a finished full-quality frame if it is still the one wanted; drop (and so cancel) one
/// that is not.
fn collect(app: &mut MaterialCraftApp, ctx: &egui::Context, key: u64) {
    let Some(job) = app.view3d.job.take() else { return };
    if job.key != key {
        return;
    }
    match job.rx.try_recv() {
        Ok(Some(frame)) => show_frame(app, ctx, frame, key, true, job.size),
        Ok(None) | Err(TryRecvError::Disconnected) => {}
        Err(TryRecvError::Empty) => app.view3d.job = Some(job),
    }
}

/// The plain scene of the current view and its cache key, building it when the scene changed.
fn base_scene(app: &mut MaterialCraftApp) -> (String, Arc<Scene>) {
    let v = &app.session.view3d;
    let scene_key = match (&*v.scene, &app.session.custom_mesh) {
        ("custom", Some((name, _))) => format!("custom:{name}"),
        _ => v.scene.clone(),
    };

    match &app.view3d.scene {
        Some((key, scene)) if *key == scene_key => (scene_key, scene.clone()),
        _ => {
            let scene = match (&*v.scene, &app.session.custom_mesh) {
                ("custom", Some((_, mesh))) => Scene::new((**mesh).clone()),
                _ => Scene::builtin(&v.scene),
            };
            let scene = Arc::new(scene);
            app.view3d.scene = Some((scene_key.clone(), scene.clone()));
            (scene_key, scene)
        }
    }
}

/// What the displaced scene depends on (scene, height output cook, relief settings, normal map); `None` when nothing is raised.
fn relief_key(app: &MaterialCraftApp, scene_key: &str, maps: &Maps) -> Option<u64> {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let v = &app.session.view3d;
    if maps.height.is_none() || v.height_scale <= 0.0 || !v.material.use_map.height {
        return None;
    }

    let graph = app.session.graph().ok()?;
    let height_output = materialcraft_engine::eval::outputs(graph, &app.session.cooked).into_iter().find(|(_, usage, _, _)| *usage == "height");
    let height_cook = height_output.and_then(|(id, _, _, _)| app.session.cooked.nodes.get(&id).map(|r| r.key));

    let mut hasher = DefaultHasher::new();
    (scene_key, height_cook, maps.normal.is_some() && v.material.use_map.normal, v.material.use_map.height).hash(&mut hasher);
    for f in [v.height_scale, v.height_level, v.tiling, v.material.tessellation, v.material.uv_scale[0]] {
        f.to_bits().hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// Collect a finished relief build or start a missing one; returns the key of the displaced scene shown (`None` = plain mesh).
fn update_relief(app: &mut MaterialCraftApp, ctx: &egui::Context) -> Option<u64> {
    let (scene_key, base) = base_scene(app);
    let maps = Maps::from_usages(app.session.material_maps());
    let wanted = relief_key(app, &scene_key, &maps);

    if let Some(job) = app.view3d.relief_job.take() {
        match job.rx.try_recv() {
            Ok(Some(scene)) if Some(job.key) == wanted => app.view3d.displaced = Some((job.key, scene)),
            Ok(_) | Err(TryRecvError::Disconnected) => {}
            // Finish the active build before starting a newer one. Dropping its receiver
            // cannot cancel subdivision, so rapid edits must not accumulate workers.
            Err(TryRecvError::Empty) => app.view3d.relief_job = Some(job),
        }
    }

    let wanted = wanted?;

    if app.view3d.displaced.as_ref().is_some_and(|(key, _)| *key == wanted) {
        return Some(wanted);
    }
    if app.view3d.relief_job.is_none() {
        let v = &app.session.view3d;
        let relief = Relief { depth: v.height_scale, level: v.height_level };
        let budget = ((RELIEF_TRIANGLES as f32) * v.material.tessellation.clamp(0.1, 4.0)) as usize;
        let tiling = v.tiling * v.material.uv_scale[0].max(0.01);
        start_relief(app, ctx, wanted, base, maps, relief, tiling, budget);
    }
    None
}

/// Build the displaced scene on a worker thread.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn start_relief(app: &mut MaterialCraftApp, ctx: &egui::Context, key: u64, base: Arc<Scene>, maps: Maps, relief: Relief, tiling: f32, budget: usize) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (repaint, viewport) = (ctx.clone(), ctx.viewport_id());
    let (thread_base, thread_maps) = (base.clone(), maps.clone());
    let spawned = std::thread::Builder::new().name("materialcraft-relief".into()).spawn(move || {
        let _ = tx.send(thread_base.with_relief(&thread_maps, relief, tiling, budget).map(Arc::new));
        repaint.request_repaint_of(viewport);
    });

    match spawned {
        Ok(_) => app.view3d.relief_job = Some(ReliefJob { key, rx }),
        Err(e) => {
            log::warn!("could not start a 3D view relief thread ({e}); building it here");
            app.view3d.displaced = base.with_relief(&maps, relief, tiling, budget).map(|scene| (key, Arc::new(scene)));
        }
    }
}

/// The browser has no threads: the displaced scene builds here (the page pauses).
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn start_relief(app: &mut MaterialCraftApp, _ctx: &egui::Context, key: u64, base: Arc<Scene>, maps: Maps, relief: Relief, tiling: f32, budget: usize) {
    app.view3d.displaced = base.with_relief(&maps, relief, tiling, budget).map(|scene| (key, Arc::new(scene)));
}

/// The scene, lighting, maps and settings of the current view: the displaced scene when it is current, else the plain one.
fn inputs(app: &mut MaterialCraftApp, supersample: u32) -> Option<(Arc<Scene>, Arc<Lighting>, Maps, Settings)> {
    let (scene_key, base) = base_scene(app);
    let v = app.session.view3d.clone();
    if app.view3d.light.as_ref().is_none_or(|l| l.name != v.environment) {
        app.view3d.light = Some(Arc::new(Lighting::new(&v.environment)));
    }

    let settings = settings_of(&v, app.view3d.show_env, supersample);
    let maps = Maps::from_usages(app.session.material_maps());
    let wanted = relief_key(app, &scene_key, &maps);
    let scene = match &app.view3d.displaced {
        Some((key, displaced)) if Some(*key) == wanted => displaced.clone(),
        _ => base,
    };
    let light = app.view3d.light.clone()?;
    Some((scene, light, maps, settings))
}

/// The renderer's settings for the view state.
pub fn settings_of(v: &materialcraft_engine::View3d, show_env: bool, supersample: u32) -> Settings {
    let cam = &v.camera;
    let bg = [cam.background_color[0], cam.background_color[1], cam.background_color[2]];
    let background = match cam.background.as_str() {
        "color" => materialcraft_render::Background::Color(bg),
        "environment" => materialcraft_render::Background::Environment,
        _ => materialcraft_render::Background::Backdrop,
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

/// The viewport render scale: the session's renderer setting, else the preference (0 = auto = 1).
fn view_scale(app: &MaterialCraftApp) -> f32 {
    let r = &app.session.view3d.renderer;
    if !r.use_viewport_resolution {
        return r.scale;
    }
    let p = app.ui.prefs.viewport_scale;
    if p > 0.0 { p.clamp(0.25, 2.0) } else { r.scale.clamp(0.25, 2.0) }
}

/// Texture pixels for `rect` at `scale` pixels per point.
fn pixels(rect: Rect, scale: f32) -> (u32, u32) {
    (((rect.width() * scale) as u32).clamp(16, 4096), ((rect.height() * scale) as u32).clamp(16, 4096))
}

/// Render a frame on this thread.
fn render_now(app: &mut MaterialCraftApp, rect: Rect, scale: f32, supersample: u32) -> Option<Frame> {
    let (scene, light, maps, settings) = inputs(app, supersample)?;
    let (w, h) = pixels(rect, scale);
    let t0 = crate::now_ms();
    let px = materialcraft_render::render(&scene, &maps, &light, &settings, w, h);
    Some(Frame { w, h, px, ms: (crate::now_ms() - t0) as f32 })
}

/// Start the full-quality frame (2×2 supersampled, full resolution) on a worker thread.
#[cfg(not(target_arch = "wasm32"))]
fn start_full(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, key: u64, size: (u32, u32)) {
    let samples = app.ui.prefs.sample_count.clamp(1, 4);
    let Some((scene, light, maps, settings)) = inputs(app, samples) else { return };
    let (w, h) = pixels(rect, ctx.pixels_per_point() * view_scale(app));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, repaint, viewport) = (cancel.clone(), ctx.clone(), ctx.viewport_id());
    let spawned = std::thread::Builder::new().name("materialcraft-view3d".into()).spawn(move || {
        let t0 = crate::now_ms();
        let px = materialcraft_render::render_cancellable(&scene, &maps, &light, &settings, w, h, &stop);
        let _ = tx.send(px.map(|px| Frame { w, h, px, ms: (crate::now_ms() - t0) as f32 }));
        repaint.request_repaint_of(viewport);
    });
    match spawned {
        Ok(_) => app.view3d.job = Some(Job { key, size, cancel, rx }),
        Err(e) => {
            log::warn!("could not start a 3D view render thread ({e}); rendering here");
            let scale = ctx.pixels_per_point() * view_scale(app);
            if let Some(frame) = render_now(app, rect, scale, samples) {
                show_frame(app, ctx, frame, key, true, size);
            }
        }
    }
}

/// The browser has no threads: the full-quality frame renders here (the page pauses).
#[cfg(target_arch = "wasm32")]
fn start_full(app: &mut MaterialCraftApp, ctx: &egui::Context, rect: Rect, key: u64, size: (u32, u32)) {
    let samples = app.ui.prefs.sample_count.clamp(1, 4);
    let scale = ctx.pixels_per_point() * view_scale(app);
    if let Some(frame) = render_now(app, rect, scale, samples) {
        show_frame(app, ctx, frame, key, true, size);
    }
}

/// Upload a frame and remember what it shows.
fn show_frame(app: &mut MaterialCraftApp, ctx: &egui::Context, frame: Frame, key: u64, full: bool, size: (u32, u32)) {
    let st = &mut app.view3d;
    let img = ColorImage::from_rgba_unmultiplied([frame.w as usize, frame.h as usize], &frame.px);
    match &mut st.tex {
        Some(t) => t.set(img, TextureOptions::LINEAR),
        None => st.tex = Some(ctx.load_texture("view3d", img, TextureOptions::LINEAR)),
    }
    st.render_ms = frame.ms;
    st.renders += 1;
    st.shown = Some((frame.w, frame.h, frame.px));
    st.key = Some((key, full));
    st.tex_size = Some(size);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_relief_keeps_one_worker_and_discards_stale_geometry() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.session.cook();
        let (scene_key, base) = base_scene(&mut app);
        let maps = Maps::from_usages(app.session.material_maps());
        let old_key = relief_key(&app, &scene_key, &maps).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        app.view3d.relief_job = Some(ReliefJob { key: old_key, rx });
        let ctx = egui::Context::default();
        for index in 1..=20 {
            app.session.view3d.height_scale = index as f32 * 0.001;
            assert_eq!(update_relief(&mut app, &ctx), None);
            assert_eq!(app.view3d.relief_job.as_ref().unwrap().key, old_key, "a second subdivision worker started");
            assert!(app.view3d.busy(), "geometry work must count as a busy view");
        }
        let wanted = relief_key(&app, &scene_key, &maps).unwrap();
        assert_ne!(old_key, wanted);
        app.view3d.displaced = Some((wanted, base.clone()));
        tx.send(Some(Arc::new(Scene::builtin("sphere")))).unwrap();
        assert_eq!(update_relief(&mut app, &ctx), Some(wanted));
        assert!(app.view3d.relief_job.is_none());
        assert!(!app.view3d.busy());
        assert!(Arc::ptr_eq(&app.view3d.displaced.as_ref().unwrap().1, &base), "outdated geometry replaced the current scene");
    }

    #[test]
    fn a_frame_for_another_size_keeps_its_proportions() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 200.0));
        assert_eq!(frame_rect(rect, None), rect);
        assert_eq!(frame_rect(rect, Some((400, 200))), rect);
        // Rendered narrower (the view was widened): full height, centred, bars at the sides.
        let r = frame_rect(rect, Some((300, 200)));
        assert_eq!((r.width(), r.height(), r.center()), (300.0, 200.0, rect.center()));
        // A degenerate size never divides by zero.
        assert_eq!(frame_rect(rect, Some((300, 0))), rect);
    }
}
