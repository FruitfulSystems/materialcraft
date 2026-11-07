use craft_ui::docking::DockContent;
use egui::{Context, Key, Modifiers, RichText, Ui};

use super::{Panel, panel_content::PanelContent};
use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};

pub(super) fn consume_shortcut(context: &Context) -> bool {
    if context.egui_wants_keyboard_input()
        || egui::Popup::is_any_open(context)
        || context.memory(|memory| memory.top_modal_layer()).is_some()
        || context.input(|input| input.pointer.any_down())
    {
        return false;
    }
    context.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Space))
}

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, panel: Panel) {
    let tokens = Tokens::get(ui.ctx());
    let title = PanelContent { app }.logical_label(&panel);
    egui::Frame::NONE.fill(tokens.panel).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.label(RichText::new(title.to_uppercase()).size(10.5).strong().color(tokens.text_strong));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(tr("Restore panel layout")).on_hover_text("Shift+Space").clicked() {
                    crate::menus::invoke(app, ui.ctx(), "ui.unmaximize");
                }
            });
        });
        let rectangle = ui.available_rect_before_wrap();
        // Match the dock host's scope so widget state survives maximizing and restoring
        let mut body = ui.new_child(
            egui::UiBuilder::new().scope_id(super::area_id().with(("body", &panel))).max_rect(rectangle).layout(egui::Layout::top_down(egui::Align::Min)),
        );
        body.set_clip_rect(rectangle.intersect(ui.clip_rect()));
        PanelContent { app }.body(&mut body, &panel);
    });
}
