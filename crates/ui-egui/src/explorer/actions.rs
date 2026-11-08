use super::ExplorerTarget;
use crate::{MaterialCraftApp, i18n::tr};
use serde_json::json;

pub(super) fn activate(app: &mut MaterialCraftApp, document: u64) -> bool {
    document == app.session.document_id || app.run("package.activate", json!({"document": document})).is_some()
}

pub(super) fn invoke(app: &mut MaterialCraftApp, ctx: &egui::Context, target: ExplorerTarget, command: &str) {
    if !activate(app, target.document()) {
        return;
    }
    let Some(folder) = target.folder(&app.session.pkg) else { return };
    match command {
        "ui.newGraph" | "ui.newFunctionGraph" => {
            let (kind, name) = if command == "ui.newFunctionGraph" { ("function", "Untitled_Function_Graph_1") } else { ("material", "Untitled_Graph_1") };
            if app.run("file.new", json!({"folder": folder, "kind": kind, "name": name, "template": "empty"})).is_some() {
                app.ui.explorer.selected = Some(ExplorerTarget::Graph { document: target.document(), index: app.session.graph });
            }
        }
        "folder.create" => {
            if let Some(path) = app
                .run(command, json!({"parent": folder, "name": "Untitled_Folder_1"}))
                .and_then(|result| result.get("folder").and_then(|path| path.as_str()).map(str::to_string))
            {
                app.ui.explorer.selected = Some(ExplorerTarget::Folder { document: target.document(), path });
            }
        }
        "graph.rename" | "folder.rename" => {
            if let Some(name) = target.name(&app.session.pkg) {
                app.ui.explorer.rename = Some((target, name));
            }
        }
        "explorer.paste" => {
            app.run(command, json!({"folder": folder}));
        }
        "package.close" | "file.reload" => {
            crate::unsaved::guard(app, crate::unsaved::Guarded::Package { document: target.document(), reload: command == "file.reload" })
        }
        "file.save" | "file.saveAs" | "file.saveCopyAs" => {
            crate::windows::file_action(app, ctx, command);
        }
        _ => {
            app.run(command, target.parameters());
        }
    }
}

pub(super) fn rename_dialog(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some((target, mut name)) = app.ui.explorer.rename.clone() else { return };
    let mut open = true;
    let mut accept = false;
    let mut cancel = false;
    egui::Window::new(tr("Rename"))
        .id(egui::Id::unique("explorer-rename"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let response = ui.text_edit_singleline(&mut name);
            crate::widgets::name(&response, egui::Role::TextInput, "Name");
            if !ctx.memory(|memory| memory.has_focus(response.id)) && !ctx.egui_wants_keyboard_input() {
                response.request_focus();
            }
            accept |= response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                accept |= ui.button(tr("Rename")).clicked();
                cancel |= ui.button(tr("Cancel")).clicked();
            });
        });
    cancel |= ctx.input(|input| input.key_pressed(egui::Key::Escape));
    app.ui.explorer.rename = if open && !cancel { Some((target.clone(), name.clone())) } else { None };
    if accept && activate(app, target.document()) {
        let mut parameters = target.parameters();
        parameters["name"] = json!(name);
        let command = if matches!(target, ExplorerTarget::Folder { .. }) { "folder.rename" } else { "graph.rename" };
        if let Some(result) = app.run(command, parameters) {
            app.ui.explorer.rename = None;
            app.ui.explorer.selected = if let Some(path) = result.get("folder").and_then(|path| path.as_str()) {
                Some(ExplorerTarget::Folder { document: target.document(), path: path.into() })
            } else {
                Some(target)
            };
        }
    }
}
