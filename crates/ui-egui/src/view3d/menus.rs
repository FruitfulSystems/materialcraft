//! The 3D view's menu bar: Scene, Materials, Lights, Camera, Environment, Display, Renderer

use egui::{RichText, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::view_cmds::{ENVIRONMENTS, SCENES};
use materialcraft_render::{Common, MapUse, Shader};
use serde_json::json;

use super::actions::{self, Page3d, edit_properties, frame};
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

fn menu_row(ui: &mut Ui, ic: &str, label: &str) -> bool {
    ui.add(egui::Button::new(format!("{ic}  {label}"))).clicked()
}

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tk.panel).inner_margin(egui::Margin::symmetric(4, 1)).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("Scene"), |ui| scene(app, ui));
            ui.menu_button(tr("Materials"), |ui| materials(app, ui));
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
            ui.menu_button(tr("Camera"), |ui| camera(app, ui));
            ui.menu_button(tr("Environment"), |ui| environment(app, ui));
            ui.menu_button(tr("Display"), |ui| display(app, ui));
            ui.menu_button(tr("Renderer"), |ui| {
                if menu_row(ui, icon::GEAR, tr("Edit settings")) {
                    edit_properties(app, Page3d::Renderer);
                    ui.close();
                }
                ui.separator();
                let _ = ui.selectable_label(true, tr("MC Ray caster (CPU)"));
                ui.add_enabled(false, egui::Button::new(tr("GPU path tracer (not supported yet)")));
                ui.separator();
                let mut depth_percent = app.session.view3d.height_scale * 100.0;
                if crate::widgets::slider(ui, tr("Height depth (% of the mesh size)"), &mut depth_percent, 0.0..=20.0, 1).changed() {
                    app.run("view.3d", json!({"heightScale": depth_percent / 100.0}));
                }
                let mut level = app.session.view3d.height_level;
                if crate::widgets::slider(ui, tr("Height level (0 = rises out of the surface)"), &mut level, 0.0..=1.0, 2).changed() {
                    app.run("view.3d", json!({"heightLevel": level}));
                }
                let mut t = app.session.view3d.tiling;
                if crate::widgets::slider(ui, tr("Tiling"), &mut t, 0.25..=8.0, 2).changed() {
                    app.run("view.3d", json!({"tiling": t}));
                }
            });
        });
    });
}

fn scene(app: &mut MaterialCraftApp, ui: &mut Ui) {
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
    if app.session.custom_mesh.is_some() && ui.selectable_label(app.session.view3d.scene == "custom", super::scene_label_custom(app)).clicked() {
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
        actions::load_state(app, false);
        ui.close();
    }
    if ui.button(tr("Load State File With Mesh...")).clicked() {
        actions::load_state(app, true);
        ui.close();
    }
    ui.separator();
    if ui.button(tr("Save Current State...")).clicked() {
        actions::save_state(app);
        ui.close();
    }
    if ui.button(tr("Save Current State As Default")).clicked() {
        app.ui.view3d_default = app.run("view.3d.state", json!({}));
        app.toast(tr("The 3D view opens with this state from now on (Reset Scene restores it)"), false);
        ui.close();
    }
    ui.separator();
    if ui.button(tr("Reset Scene")).clicked() {
        actions::reset_all(app);
        ui.close();
    }
}

/// Materials ▸ Default: the shader and its properties; below it, what each material channel
/// is assigned from (View in 3D view or the graph's output usages)
fn materials(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.menu_button(tr("Default"), |ui| {
        if menu_row(ui, icon::GEAR, tr("Edit properties")) {
            edit_properties(app, Page3d::Material);
            ui.close();
        }
        ui.separator();
        let current = app.session.view3d.material.shader;
        for (shader, label) in [
            (Shader::Blinn, "Blinn"),
            (Shader::Lambert, "Lambert"),
            (Shader::MeshInfo, "Mesh Info"),
            (Shader::OpenPbr, "OpenPBR [Default]"),
            (Shader::MetallicRoughness, "Metallic Roughness"),
            (Shader::Coated, "Metallic Roughness - Coated"),
            (Shader::Subsurface, "Metallic Roughness - SSS"),
            (Shader::SpecularGlossiness, "Specular Glossiness"),
            (Shader::Unlit, "Unlit"),
        ] {
            if ui.selectable_label(current == shader, tr(label)).clicked() {
                app.run("view.3d", json!({"material": {"shader": shader.id()}}));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Reset Common Parameters")).clicked() {
            app.run("view.3d", json!({"material": {"common": Common::default(), "useMap": MapUse::default()}}));
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
    ui.separator();
    ui.label(RichText::new(tr("Assigned channels")).italics().color(tk.text_dim));
    let sources = app.session.material_preview.sources(&app.session.pkg);
    for channel in materialcraft_engine::core::MaterialChannel::ALL {
        let label = sources
            .get(&channel)
            .and_then(|source| {
                let graph = app.session.pkg.graphs.get(source.graph)?;
                let node = graph.node(source.node).ok()?;
                Some(format!("{} / {}", graph.name, crate::graph::node_label(node)))
            })
            .unwrap_or_else(|| tr("Unassigned").to_string());
        ui.label(format!("{}: {label}", tr(channel.label())));
    }
    ui.separator();
    if ui.button(tr("Clear material assignments")).clicked() {
        app.run("view.3d.clear", json!({}));
        ui.close();
    }
}

fn camera(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
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
        actions::save_render(app);
        ui.close();
    }
    if menu_row(ui, icon::CLIPBOARD, tr("Copy Render to Clipboard")) {
        actions::copy_render(app, ui.ctx());
        ui.close();
    }
    ui.separator();
    if ui.add(egui::Button::new(tr("Reset Position")).shortcut_text("R")).clicked() {
        actions::reset_position(app);
        ui.close();
    }
    if menu_row(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset Camera")) {
        app.run("view.3d.reset", json!({"what": "camera"}));
        ui.close();
    }
}

fn environment(app: &mut MaterialCraftApp, ui: &mut Ui) {
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
    ui.separator();
    let mut rot = app.session.view3d.env_rotation;
    if crate::widgets::slider(ui, tr("Rotation"), &mut rot, 0.0..=360.0, 0).changed() {
        app.run("view.3d", json!({"envRotation": rot}));
    }
    let mut exp = app.session.view3d.exposure;
    if crate::widgets::slider(ui, tr("Exposure"), &mut exp, -4.0..=4.0, 2).changed() {
        app.run("view.3d", json!({"exposure": exp}));
    }
}

fn display(app: &mut MaterialCraftApp, ui: &mut Ui) {
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
    ui.checkbox(&mut app.view3d.show_env, tr("Environment background"));
    let mut t = app.session.view3d.transparency;
    if ui.checkbox(&mut t, tr("Transparency (alpha)")).changed() {
        app.run("view.3d", json!({"transparency": t}));
    }
    let mut a = app.session.view3d.alpha_view;
    if ui.checkbox(&mut a, tr("Show alpha channel")).changed() {
        app.run("view.3d", json!({"alphaView": a}));
    }
    ui.separator();
    // A submenu rather than a combo box: a popup inside a menu closes it
    let mut size = app.session.preview_size.unwrap_or(0);
    ui.menu_button(format!("{}: {}", tr("Preview size"), crate::preview_resolution::label(size)), |ui| {
        if crate::preview_resolution::choices(ui, &mut size) {
            app.run("view.previewSize", json!({"size": size}));
            ui.close();
        }
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
}
