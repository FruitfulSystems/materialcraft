use crate::{i18n::tr, theme::Tokens};
use egui::Ui;
use materialcraft_engine::core::FunctionGraph;

pub(super) fn show(ui: &mut Ui, graph: &FunctionGraph) {
    let messages = materialcraft_engine::nodes::function::warnings(graph);
    if messages.is_empty() {
        return;
    }

    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for message in messages.iter().take(3) {
            ui.colored_label(tokens.warning, format!("{}  {message}", egui_phosphor::regular::WARNING))
                .on_hover_text(tr("Connect the missing inputs and right-click a node to set it as the output"));
        }
        if messages.len() > 3 {
            ui.collapsing(format!("{} ({})", tr("More warnings"), messages.len() - 3), |ui| {
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    for message in messages.iter().skip(3) {
                        ui.colored_label(tokens.warning, message);
                    }
                });
            });
        }
    });
}
