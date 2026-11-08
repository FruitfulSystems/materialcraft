use super::function_library_drag::FunctionLibraryDrag;
use crate::{MaterialCraftApp, i18n::tr};
use egui::{Sense, Ui};
use materialcraft_engine::FunctionEdit;

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Some(target) = app.function_editor.as_ref().and_then(|editor| editor.command_target()) else { return };

    let operations = target.available_operations(&app.session);
    ui.add(egui::TextEdit::singleline(&mut app.ui.library_search).hint_text(tr("Search functions")));
    let search = app.ui.library_search.to_ascii_lowercase();
    let functions: Vec<_> = app
        .session
        .pkg
        .dependencies
        .iter()
        .filter(|(path, _)| path.starts_with("sbs://"))
        .flat_map(|(path, package)| {
            package.graphs.iter().filter(|graph| graph.function.is_some()).map(move |graph| {
                (
                    path.clone(),
                    graph.sbs_path(),
                    graph.display_name().to_string(),
                    graph.attributes.description.clone(),
                    graph.function.as_ref().is_some_and(|function| function.output.is_none() || function.nodes.iter().any(|node| node.missing.is_some())),
                )
            })
        })
        .filter(|(_, path, label, _, _)| path.to_ascii_lowercase().contains(&search) || label.to_ascii_lowercase().contains(&search))
        .collect();
    let mut add = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.strong(tr("SBS functions"));
        if !app.session.pkg.dependencies.keys().any(|path| path.starts_with("sbs://")) {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 180), tr("Missing SBS function library"));
        }

        for (package, graph, label, description, missing) in functions {
            let text = if missing {
                egui::RichText::new(format!("{}: {label}", tr("Missing"))).color(egui::Color32::from_rgb(255, 90, 180))
            } else {
                egui::RichText::new(&label)
            };
            let response = ui.add(egui::Label::new(text).selectable(false).sense(Sense::click_and_drag()));
            crate::widgets::name(&response, egui::Role::ListItem, &label);
            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), FunctionLibraryDrag::Instance { package: package.clone(), graph: graph.clone() });
            }
            if response.double_clicked() {
                add = Some(FunctionLibraryDrag::Instance { package: package.clone(), graph: graph.clone() });
            }
            response.on_hover_text(format!("{description}\n{package}/{graph}"));
        }
        ui.separator();
        ui.strong(tr("Native operations"));
        for definition in operations.filter(|definition| definition.label.to_ascii_lowercase().contains(&search)) {
            let response = ui.add(egui::Label::new(tr(definition.label)).selectable(false).sense(Sense::click_and_drag()));
            crate::widgets::name(&response, egui::Role::ListItem, definition.label);
            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), FunctionLibraryDrag::Operation(definition.id.into()));
            }
            if response.double_clicked() {
                add = Some(FunctionLibraryDrag::Operation(definition.id.into()));
            }
        }
    });
    if let Some(add) = add
        && let Some(mut editor) = app.function_editor.take()
    {
        let at = editor.current(app).map_or([0.0; 2], |graph| [graph.nodes.len() as f32 * 140.0, 0.0]);
        insert(app, &mut editor, &add, at);
        app.function_editor = Some(editor);
    }
}

pub(super) fn insert(app: &mut MaterialCraftApp, editor: &mut super::FunctionEditor, entry: &FunctionLibraryDrag, at: [f32; 2]) {
    match entry {
        FunctionLibraryDrag::Operation(operation) => {
            editor.edit(app, FunctionEdit::Add { operation: operation.clone(), at, snap: app.ui.prefs.snap_to_grid }, false)
        }
        FunctionLibraryDrag::Instance { package, graph } => {
            let Some(target) = editor.command_target() else { return };
            if let Some(result) = app.run(
                "library.function.add",
                serde_json::json!({
                    "package": package, "graph": graph, "function": target, "at": at, "snapToGrid": app.ui.prefs.snap_to_grid,
                }),
            ) && let Some(id) = result.get("id").and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok())
            {
                editor.selection = vec![id];
                editor.selected_wires.clear();
                editor.inspected_node = Some(id);
            }
        }
    }
}
