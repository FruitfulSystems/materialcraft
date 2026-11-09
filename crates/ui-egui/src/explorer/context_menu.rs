use super::ExplorerTarget;
use crate::menus::{self, ExplorerRow};
use crate::{i18n::tr, menus::shortcut_label};

/// What the right-clicked item allows right now
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MenuState {
    /// The Explorer clipboard holds something to paste
    pub paste: bool,
    /// The package has a file on disk (reload, show in folder)
    pub saved: bool,
    /// The package links other package files
    pub dependencies: bool,
    /// The graph (or, for a package, one of its graphs) has image outputs
    pub outputs: bool,
}

impl MenuState {
    fn enabled(&self, command: &str) -> bool {
        match command {
            "" => false,
            "explorer.paste" => self.paste,
            "file.reload" | menus::EXPLORER_SHOW_IN_FOLDER => self.saved,
            menus::EXPLORER_OPEN_DEPENDENCIES => self.dependencies,
            menus::EXPLORER_COMPUTE | menus::EXPLORER_VIEW_OUTPUTS | menus::EXPLORER_EXPORT_OUTPUTS => self.outputs,
            _ => true,
        }
    }
}

/// Rows the browser can't offer: it has no folders to show or package files to open
fn hidden(command: &str) -> bool {
    cfg!(target_arch = "wasm32") && matches!(command, menus::EXPLORER_SHOW_IN_FOLDER | menus::EXPLORER_OPEN_DEPENDENCIES)
}

fn item(ui: &mut egui::Ui, label: &str, command: &'static str, shortcut: Option<&str>, enabled: bool, action: &mut Option<&'static str>) {
    let mut button = egui::Button::new(tr(label));
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(shortcut_label(ui.ctx(), shortcut));
    }
    let response = ui.add_enabled(enabled, button);
    let response = if command.is_empty() { response.on_disabled_hover_text(tr("Not available yet")) } else { response };
    if response.clicked() {
        *action = Some(command);
        ui.close();
    }
}

fn rows(ui: &mut egui::Ui, rows: &[ExplorerRow], state: &MenuState, resources: bool, action: &mut Option<&'static str>) {
    for row in rows {
        match *row {
            ExplorerRow::Separator => {
                ui.separator();
            }
            ExplorerRow::Item(label, command, shortcut) if !hidden(command) => item(ui, label, command, shortcut, state.enabled(command), action),
            ExplorerRow::Item(..) => {}
            ExplorerRow::Sub(label, entries) => {
                let available = entries.iter().any(|(_, command)| !command.is_empty());
                ui.add_enabled_ui(available, |ui| {
                    let submenu = ui.menu_button(tr(label), |ui| {
                        for (entry, command) in entries {
                            // Resource folders hold folders and resources, not graphs
                            if resources && label == "New" && *command != "folder.create" {
                                continue;
                            }
                            item(ui, entry, command, None, state.enabled(command), action);
                        }
                    });
                    submenu.response.on_disabled_hover_text(tr("Not available yet"));
                });
            }
        }
    }
}

pub(super) fn show(ui: &mut egui::Ui, target: &ExplorerTarget, state: &MenuState) -> Option<&'static str> {
    ui.set_min_width(220.0);
    let mut action = None;
    let resources = target.is_resources();
    match target {
        ExplorerTarget::Package { .. } => rows(ui, menus::EXPLORER_PACKAGE, state, false, &mut action),
        ExplorerTarget::Folder { .. } if resources => {
            rows(ui, &[ExplorerRow::Sub("New", menus::EXPLORER_NEW), ExplorerRow::Sub("Import", menus::EXPLORER_IMPORT)], state, true, &mut action);
            if !target.is_resources_root() {
                ui.separator();
                rows(
                    ui,
                    &[ExplorerRow::Item("Rename", "folder.rename", Some("F2")), ExplorerRow::Item("Remove", "folder.remove", Some("Delete"))],
                    state,
                    true,
                    &mut action,
                );
            }
        }
        ExplorerTarget::Folder { .. } => {
            rows(ui, &[ExplorerRow::Sub("New", menus::EXPLORER_NEW), ExplorerRow::Separator], state, false, &mut action);
            rows(ui, menus::EXPLORER_FOLDER, state, false, &mut action);
        }
        ExplorerTarget::Graph { .. } => rows(ui, menus::EXPLORER_GRAPH, state, false, &mut action),
        ExplorerTarget::Resource { .. } => {
            for (label, command) in menus::EXPLORER_RESOURCE {
                item(ui, label, command, None, true, &mut action);
            }
        }
        ExplorerTarget::Node { .. } => return None,
    }
    action
}
