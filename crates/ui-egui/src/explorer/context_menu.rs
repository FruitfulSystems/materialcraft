use super::ExplorerTarget;
use crate::{i18n::tr, menus};

pub(super) fn show(ui: &mut egui::Ui, target: &ExplorerTarget, paste: bool, reload: bool) -> Option<&'static str> {
    ui.set_min_width(190.0);
    let mut action = None;
    if matches!(target, ExplorerTarget::Package { .. } | ExplorerTarget::Folder { .. }) {
        ui.menu_button(tr("New"), |ui| {
            for (label, command) in menus::EXPLORER_NEW {
                if ui.button(tr(label)).clicked() {
                    action = Some(*command);
                    ui.close();
                }
            }
        });
        ui.separator();
    }
    let entries = match target {
        ExplorerTarget::Package { .. } => menus::EXPLORER_PACKAGE,
        ExplorerTarget::Folder { .. } => menus::EXPLORER_FOLDER,
        ExplorerTarget::Graph { .. } => menus::EXPLORER_GRAPH,
        ExplorerTarget::Node { .. } => return None,
    };
    for (label, command) in entries {
        let enabled = match *command {
            "explorer.paste" => paste,
            "file.reload" => reload,
            _ => true,
        };
        if ui.add_enabled(enabled, egui::Button::new(tr(label))).clicked() {
            action = Some(*command);
            ui.close();
        }
    }
    action
}
