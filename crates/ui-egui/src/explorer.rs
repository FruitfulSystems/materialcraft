//! The package tree, its graphs, inputs, outputs and resources, each with a context menu

use egui::{RichText, Sense, Ui};

mod actions;
mod package_menu;
mod tree_row;
use egui_phosphor::regular as icon;
use serde_json::json;

pub use actions::{GraphClipboard, Rename};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if crate::widgets::icon_button(ui, icon::FILE_PLUS, tr("New graph..."), false, 22.0).clicked() {
            app.ui.windows.new_graph = true;
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
    let mut menu_pick: Option<&'static str> = None;
    let mut resource_action: Option<(&str, usize)> = None;
    let mut rename_done: Option<(usize, Option<String>)> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let name = format!("{}{}", app.session.pkg.name, if app.session.modified { "*" } else { "" });
        let package_row = tree_row::show(ui, &name, icon::PACKAGE, false);
        if let Some(path) = &app.session.path {
            package_row.clone().on_hover_text(path);
        }
        package_row.context_menu(|ui| {
            if let Some(id) = package_menu::show(app, ui) {
                menu_pick = Some(id);
            }
        });
        let mut open: Option<usize> = None;
        let mut action: Option<(&str, usize)> = None;
        let mut selected_node = None;
        let mut assignment = None;
        let renaming = app.explorer_rename.0.clone();
        let can_paste = app.explorer_clipboard.0.is_some();
        // Keyboard over the tree: F2 renames the open graph, Delete removes it
        if ui.ui_contains_pointer() && renaming.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            let gi = app.session.graph;
            ui.input(|i| {
                if i.key_pressed(egui::Key::F2) {
                    action = Some(("rename", gi));
                } else if i.key_pressed(egui::Key::Delete) {
                    action = Some(("delete", gi));
                }
            });
        }
        for (index, graph) in app.session.pkg.graphs.iter().enumerate() {
            let active = index == app.session.graph;
            let function = graph.function.is_some();
            // Rename in place (F2 / the context menu)
            if let Some((ri, mut text)) = renaming.clone()
                && ri == index
            {
                let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width() - 24.0));
                if !resp.has_focus() && !resp.lost_focus() {
                    resp.request_focus();
                }
                let enter = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                let escape = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
                if escape {
                    rename_done = Some((index, None));
                } else if resp.lost_focus() || enter {
                    rename_done = Some((index, Some(text.clone())));
                } else if resp.changed() {
                    app.explorer_rename.0 = Some((index, text));
                }
                continue;
            }
            let mut header = |ui: &mut Ui| {
                let symbol = if function { icon::FUNCTION } else { icon::TREE_STRUCTURE };
                let response = tree_row::show(ui, &graph.name, symbol, active);
                if !function {
                    response.interact(egui::Sense::drag()).dnd_set_drag_payload(crate::graph_drag_payload::GraphDragPayload { name: graph.name.clone() });
                }
                if response.clicked() {
                    open = Some(index);
                }
                response.context_menu(|ui| {
                    if !function {
                        for (label, command) in [("View outputs in 3D view", "view3d"), ("Reset and view outputs in 3D view", "reset3d")] {
                            if ui.button(tr(label)).clicked() {
                                action = Some((command, index));
                                ui.close();
                            }
                        }
                        ui.separator();
                    }
                    for (label, command, key, enabled) in [
                        ("Open", "open", "Return", true),
                        ("Copy", "copy", "", true),
                        ("Paste", "paste", "", can_paste),
                        ("Remove", "delete", "Del", true),
                        ("Rename", "rename", "F2", true),
                        ("Duplicate", "duplicate", "", true),
                        ("Compute Outputs", "compute", "", true),
                        ("Export outputs...", "export", "", true),
                    ] {
                        if crate::menus::menu_row(ui, tr(label), key, enabled).clicked() {
                            action = Some((command, index));
                            ui.close();
                        }
                    }
                });
            };

            if function {
                ui.horizontal(|ui| {
                    ui.add_space(ui.spacing().indent);
                    header(ui);
                });
                continue;
            }

            let id = ui.make_persistent_id(("graph", index, &graph.name));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true).show_header(ui, header).body(|ui| {
                ui.indent("graph-ports", |ui| {
                    for node in graph.nodes.iter().filter(|node| matches!(node.def.as_str(), "output" | "input_color" | "input_grayscale" | "input_value")) {
                        ui.push_id(node.id, |ui| {
                            let name = crate::graph::node_label(node);
                            let selected = active && app.session.selection.contains(&node.id);
                            let symbol = if node.def == "input_value" { icon::FUNCTION } else { icon::IMAGE };
                            let response = tree_row::show(ui, &name, symbol, selected);

                            response.context_menu(|ui| {
                                if let Some(parameters) = crate::material_preview_menu::show(ui, &app.session, index, node.id) {
                                    assignment = Some(parameters);
                                }
                            });
                            if response.clicked() || response.double_clicked() {
                                selected_node = Some((index, node.id, response.double_clicked()));
                            }
                        });
                    }
                });
            });
        }
        if let Some(i) = open {
            app.run("graph.open", json!({"graph": i}));
        }
        if let Some((index, node, preview)) = selected_node {
            if index != app.session.graph && app.run("graph.open", json!({"graph": index})).is_none() {
                return;
            }

            app.pattern_editor = None;
            app.function_editor = None;
            app.run("graph.annotation.select", json!({}));
            app.run("graph.select", json!({"ids": [node.0]}));
            app.graph.inspected_node = Some(node);
            app.ui.dock.focus(crate::docking::Panel::Properties);
            if preview {
                app.run("view.2d", json!({"node": node.0}));
                app.ui.dock.focus(crate::docking::Panel::View2d);
            }
        }

        if let Some(parameters) = assignment {
            app.run("view.3d.assign", parameters);
        }
        match action {
            Some((command @ ("view3d" | "reset3d"), graph)) => {
                app.run("view.3d.outputs", json!({"graph": graph, "reset": command == "reset3d"}));
            }
            Some(("open", i)) => {
                app.run("graph.open", json!({"graph": i}));
            }
            Some(("export", i)) => {
                // The export window works on the open graph
                if app.run("graph.open", json!({"graph": i})).is_some() {
                    app.ui.windows.export = true;
                }
            }
            Some(("duplicate", i)) => {
                app.run("graph.duplicate", json!({"graph": i}));
            }
            Some(("delete", i)) => {
                app.run("graph.delete", json!({"graph": i}));
            }
            Some(("copy", i)) => actions::copy_graph(app, ui.ctx(), i),
            Some(("paste", _)) => actions::paste_graph(app),
            Some(("rename", i)) => {
                if let Some(g) = app.session.pkg.graphs.get(i) {
                    app.explorer_rename.0 = Some((i, g.name.clone()));
                }
            }
            Some(("compute", i)) if app.run("graph.open", json!({"graph": i})).is_some() => actions::compute(app),
            _ => {}
        }
        if !app.session.pkg.resources.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Resources")).color(tk.text_dim).size(11.0));
            for (i, r) in app.session.pkg.resources.iter().enumerate() {
                let linked = r.link.is_some();
                let text = format!("{}  {}  ({} KB)", if linked { icon::LINK } else { icon::IMAGE }, r.name, r.bytes.len() / 1024);
                let hover = match &r.link {
                    Some(l) => format!("{} {l}", tr("Linked:")),
                    None => tr("Embedded in the package").to_string(),
                };
                let resp = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text(hover);
                resp.context_menu(|ui| {
                    if linked && ui.button(tr("Relink...")).clicked() {
                        resource_action = Some(("relink", i));
                        ui.close();
                    }
                    if ui.button(tr("Add a Bitmap node")).clicked() {
                        resource_action = Some(("node", i));
                        ui.close();
                    }
                    if ui.button(tr("Remove")).clicked() {
                        resource_action = Some(("delete", i));
                        ui.close();
                    }
                });
            }
        }
    });
    if let Some((i, text)) = rename_done {
        let old = app.session.pkg.graphs.get(i).map(|g| g.name.clone()).unwrap_or_default();
        app.explorer_rename.0 = None;
        if let Some(new) = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && *t != old) {
            app.run("graph.rename", json!({"graph": i, "name": new}));
        }
    }
    match menu_pick {
        Some("explorer.copy") => actions::copy_graph(app, ui.ctx(), app.session.graph),
        Some("explorer.paste") => actions::paste_graph(app),
        Some("explorer.compute") => actions::compute(app),
        Some("explorer.reveal") => actions::reveal(app),
        Some(id) => crate::menus::invoke(app, ui.ctx(), id),
        None => {}
    }
    match resource_action {
        Some(("delete", i)) => {
            if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone()) {
                app.run("file.deleteResource", json!({"name": name}));
            }
        }
        Some(("node", i)) => {
            if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone()) {
                app.run("graph.node.add", json!({"def": "bitmap", "params": {"resource": name}}));
            }
        }
        Some(("relink", i)) => actions::relink(app, i),
        _ => {}
    }
}
