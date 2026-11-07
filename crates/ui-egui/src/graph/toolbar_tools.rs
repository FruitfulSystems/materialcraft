//! The graph toolbar's view tools: actual-size zoom, a screenshot of the canvas, the graph
//! information popup and the clean-up action

use egui::{RichText, Ui};
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Zoom the graph camera to `zoom` around the view's center
pub(super) fn zoom_to(app: &mut MaterialCraftApp, zoom: f32) {
    let c = super::view_center(app);
    let v = app.graph.view;
    let cam = &mut app.ui.graph_camera;
    cam.zoom = zoom;
    cam.pan = [c.x - v.width() * 0.5 / zoom, c.y - v.height() * 0.5 / zoom];
    *cam = cam.sanitized();
}

/// Save the graph panel as a PNG: a screenshot of the window, cropped to the canvas
pub(super) fn screenshot(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let name = format!("{}_graph.png", materialcraft_engine::formats::safe_file_name(&app.session.graph().map(|g| g.name.clone()).unwrap_or_default()));
        let Some(path) = crate::windows::dialog(app).set_file_name(name).add_filter("PNG", &["png"]).save_file() else { return };
        crate::windows::remember_dir(app, &path);
        let view = app.graph.view;
        let ppp = ctx.pixels_per_point();
        let repaint = ctx.clone();
        let callback = egui::ScreenshotCallback::new(move |img| {
            let x0 = ((view.left() * ppp).max(0.0) as usize).min(img.size[0]);
            let y0 = ((view.top() * ppp).max(0.0) as usize).min(img.size[1]);
            let x1 = ((view.right() * ppp).max(0.0) as usize).clamp(x0, img.size[0]);
            let y1 = ((view.bottom() * ppp).max(0.0) as usize).clamp(y0, img.size[1]);
            let (w, h) = (x1 - x0, y1 - y0);
            if w == 0 || h == 0 {
                return;
            }
            let mut rgba = Vec::with_capacity(w * h * 4);
            for y in y0..y1 {
                for x in x0..x1 {
                    if let Some(px) = img.pixels.get(y * img.size[0] + x) {
                        rgba.extend_from_slice(&px.to_array());
                    }
                }
            }
            if let Some((_, _, png)) = crate::control::encode_png(w as u32, h as u32, rgba)
                && let Err(e) = std::fs::write(&path, png)
            {
                log::warn!("could not save the graph screenshot {}: {e}", path.display());
            }
            repaint.request_repaint();
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(callback));
        app.toast(tr("Saving a screenshot of the graph"), false);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        app.toast(tr("Screenshots are saved from the desktop app"), true);
    }
}

/// The ⓘ popup: what the graph is, how big and how long it takes
pub(super) fn graph_info(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let Ok(g) = app.session.graph() else { return };
    let [w, h] = g.size_px();
    let outputs = g.nodes.iter().filter(|n| n.def == "output").count();
    let errors = app.session.cooked.nodes.values().filter(|r| r.error.is_some()).count();
    let ms: f32 = app.session.cooked.nodes.values().map(|r| r.ms).sum();
    ui.set_min_width(220.0);
    ui.label(RichText::new(&g.name).strong());
    ui.label(RichText::new(format!("{} ({})", app.session.pkg.name, app.session.path.as_deref().unwrap_or(tr("not saved yet")))).small().color(tk.text_dim));
    ui.separator();
    egui::Grid::new("graph-info").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        ui.label(tr("Size"));
        ui.label(format!("{w} x {h}"));
        ui.end_row();
        ui.label(tr("Format"));
        ui.label(format!("{} bits", g.format.bits()));
        ui.end_row();
        ui.label(tr("Nodes"));
        ui.label(format!("{} ({} {})", g.nodes.len(), outputs, tr("outputs")));
        ui.end_row();
        ui.label(tr("Wires"));
        ui.label(g.connections.len().to_string());
        ui.end_row();
        ui.label(tr("Last cook"));
        ui.label(if errors > 0 { format!("{ms:.0} ms, {errors} {}", tr("error(s)")) } else { format!("{ms:.0} ms") });
        ui.end_row();
        ui.label(tr("Exposed"));
        ui.label(g.exposed.len().to_string());
        ui.end_row();
    });
    ui.separator();
    if ui.button(tr("Graph attributes...")).clicked() {
        app.run("edit.selectNone", json!({}));
        app.ui.dock.focus(crate::docking::Panel::Properties);
        ui.close();
    }
}

/// Remove reroute dots nothing uses and disabled nodes with no wires
pub(super) fn clean_up(app: &mut MaterialCraftApp) {
    let Ok(g) = app.session.graph() else { return };
    let wired = |id: materialcraft_engine::core::NodeId| g.connections.iter().any(|c| c.from == id || c.to == id);
    let ids: Vec<u64> =
        g.nodes.iter().filter(|n| (n.def == "dot" && !g.connections.iter().any(|c| c.from == n.id)) || (n.disabled && !wired(n.id))).map(|n| n.id.0).collect();
    if ids.is_empty() {
        app.toast(tr("Nothing to clean up"), false);
        return;
    }
    let n = ids.len();
    if app.run("graph.select", json!({"ids": ids})).is_some() && app.run("edit.delete", json!({})).is_some() {
        app.toast(format!("{} {n}", tr("Removed nodes:")), false);
    }
}
