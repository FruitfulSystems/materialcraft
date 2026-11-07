//! The tool strip down the left of the viewport: framing, property pages and display toggles

use egui::{Rect, Ui, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;

use super::actions::{Page3d, edit_properties, frame};
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

pub(super) const WIDTH: f32 = 28.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, strip: Rect) {
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
