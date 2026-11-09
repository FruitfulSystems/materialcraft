use super::ExplorerTarget;
use crate::{i18n::tr, menus};

pub(super) fn show(ui: &mut egui::Ui, target: &ExplorerTarget, paste: bool, reload: bool) -> Option<&'static str> {
    ui.set_min_width(190.0);
    let mut action = None;
    let resources = target.is_resources();
    if matches!(target, ExplorerTarget::Package { .. } | ExplorerTarget::Folder { .. }) {
        ui.menu_button(tr("New"), |ui| {
            for (label, command) in menus::EXPLORER_NEW {
                if resources && *command != "folder.create" {
                    continue;
                }
                if ui.button(tr(label)).clicked() {
                    action = Some(*command);
                    ui.close();
                }
            }
        });
        if matches!(target, ExplorerTarget::Package { .. }) || resources {
            ui.menu_button(tr("Import"), |ui| {
                for (label, command) in menus::EXPLORER_IMPORT {
                    if ui.button(tr(label)).clicked() {
                        action = Some(*command);
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
    }
    if resources {
        if !target.is_resources_root() {
            for (label, command) in [("Rename", "folder.rename"), ("Remove", "folder.remove")] {
                if ui.button(tr(label)).clicked() {
                    action = Some(command);
                    ui.close();
                }
            }
        }
        return action;
    }
    let entries = match target {
        ExplorerTarget::Package { .. } => menus::EXPLORER_PACKAGE,
        ExplorerTarget::Folder { .. } => menus::EXPLORER_FOLDER,
        ExplorerTarget::Graph { .. } => menus::EXPLORER_GRAPH,
        ExplorerTarget::Resource { .. } => menus::EXPLORER_RESOURCE,
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
