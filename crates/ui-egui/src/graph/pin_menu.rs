//! Right-click on a navigation pin: rename it in place, jump to it, delete it

use egui::Ui;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, pin: u64) {
    ui.set_min_width(200.0);
    let Some(current) = app.session.graph().ok().and_then(|graph| graph.pins.iter().find(|p| p.id == pin).cloned()) else {
        ui.label(tr("The pin is gone"));
        return;
    };
    ui.label(egui::RichText::new(tr("Pin name")).small());
    let id = ui.make_persistent_id(("pin-rename", pin));
    let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| current.name.clone());
    let edit = ui.text_edit_singleline(&mut name);
    crate::widgets::name(&edit, egui::Role::TextInput, "Pin name");
    if edit.changed() {
        ui.data_mut(|d| d.insert_temp(id, name.clone()));
    }
    let apply = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if (apply || ui.button(tr("Rename")).clicked()) && name.trim() != current.name {
        app.run("graph.pin.edit", json!({"id": pin, "name": name.trim()}));
        ui.data_mut(|d| d.remove_temp::<String>(id));
        ui.close();
    }
    ui.separator();
    if ui.button(tr("Go to pin")).clicked() {
        let zoom = app.ui.graph_camera.zoom.max(0.01);
        app.ui.graph_camera.pan = [current.pos[0] - app.graph.view.width() * 0.5 / zoom, current.pos[1] - app.graph.view.height() * 0.5 / zoom];
        ui.close();
    }
    if ui.button(tr("Delete pin")).clicked() {
        app.run("graph.pin.delete", json!({"id": pin}));
        ui.close();
    }
}
