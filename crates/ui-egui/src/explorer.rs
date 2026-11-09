//! The package tree, its graphs, inputs, outputs and resources

use egui::Ui;

mod actions;
mod context_menu;
mod explorer_state;
mod explorer_target;
mod explorer_tree;
mod tree_row;
use egui_phosphor::regular as icon;
pub use explorer_state::ExplorerState;
pub use explorer_target::ExplorerTarget;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if crate::widgets::icon_button(ui, icon::FILE_PLUS, tr("New graph..."), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "ui.newGraph");
        }
        if crate::widgets::icon_button(ui, icon::FOLDER_OPEN, tr("Open package..."), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.open");
        }
        if crate::widgets::icon_button(ui, icon::FLOPPY_DISK, tr("Save package"), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.save");
        }
        if crate::widgets::icon_button(ui, icon::EXPORT, tr("Export outputs..."), false, 22.0).clicked() {
            app.ui.windows.export = true;
        }
        if crate::widgets::icon_button(ui, icon::IMAGE, tr("Import bitmap..."), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.importBitmap");
        }
    });
    ui.add_space(2.0);
    let context = ui.ctx().clone();
    let mut tree = explorer_tree::ExplorerTree {
        session: &app.session,
        state: &mut app.ui.explorer,
        action: None,
        opened_node: None,
        assignment: None,
        movement: None,
        inspected_graph: None,
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (document, package, path, modified) in tree.session.open_packages() {
            ui.push_id(document, |ui| tree.package(ui, document, package, path, modified));
        }
    });
    let action = tree.action;
    let opened_node = tree.opened_node;
    let assignment = tree.assignment;
    let movement = tree.movement;
    if let Some(target) = tree.inspected_graph {
        crate::properties::GraphPropertyDraft::commit(app);
        app.inspected_graph = Some(target);
        app.ui.dock.focus(crate::docking::Panel::Properties);
    }
    if let Some((document, index, node)) = opened_node {
        if !actions::activate(app, document) {
            return;
        }
        if index != app.session.graph && app.run("graph.open", json!({"graph": index})).is_none() {
            return;
        }

        app.pattern_editor = None;
        crate::properties::GraphPropertyDraft::commit(app);
        app.inspected_graph = None;
        app.function_editor = None;
        app.run("graph.annotation.select", json!({}));
        app.run("graph.select", json!({"ids": [node.0]}));
        app.graph.inspected_node = Some(node);
        app.ui.dock.focus(crate::docking::Panel::Properties);
        app.run("view.2d", json!({"node": node.0}));
        app.ui.dock.focus(crate::docking::Panel::View2d);
    }
    if let Some(parameters) = assignment {
        app.run("view.3d.assign", parameters);
    }
    if let Some((target, parent)) = movement
        && actions::activate(app, target.document())
    {
        let mut parameters = target.parameters();
        parameters["parent"] = json!(parent);
        app.run("explorer.move", parameters);
    }
    if let Some((target, command)) = action {
        if command == "activate" {
            actions::activate(app, target.document());
        } else {
            actions::invoke(app, &context, target, command);
        }
    }
}

pub(crate) fn dialogs(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    actions::rename_dialog(app, ctx);
}
