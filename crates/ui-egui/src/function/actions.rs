use super::{FunctionEditor, FunctionTarget};
use crate::MaterialCraftApp;

pub(super) fn editor_enabled(app: &MaterialCraftApp, editor: &FunctionEditor, command: &str) -> Option<bool> {
    if let Some(enabled) = crate::graph::wire_actions::enabled(command, editor.selected_wires.len()) {
        return Some(enabled && editor.current(app).is_some());
    }
    let standalone = matches!(editor.target, FunctionTarget::Graph);
    if standalone && app.session.annotation_selection.is_some() && matches!(command, "edit.delete" | "edit.deleteRelink") {
        return None;
    }
    if let Some(target) = editor.command_target()
        && let Some(enabled) = materialcraft_engine::selection_commands::enabled_for(
            &app.session,
            &materialcraft_engine::SelectionTarget::Function(target),
            &editor.selection,
            command,
        )
    {
        return Some(enabled && editor.current(app).is_some());
    }
    match command {
        "edit.undo" | "edit.redo" | "edit.finish" | "graph.cook" | "ui.graph.snapToGrid" => None,
        "ui.graph.previousPin" | "ui.graph.nextPin" | "graph.comment.add" | "graph.frame.add" | "graph.pin.add" if standalone => None,
        command if command.starts_with("graph.") || command.starts_with("ui.graph.") || command.starts_with("edit.") => Some(false),
        _ => None,
    }
}

pub(crate) fn enabled(app: &MaterialCraftApp, command: &str) -> Option<bool> {
    app.function_editor.as_ref().and_then(|editor| editor_enabled(app, editor, command))
}

pub(super) fn invoke_editor(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, command: &str) -> bool {
    let Some(allowed) = editor_enabled(app, editor, command) else { return false };

    if !allowed {
        return true;
    }

    if command.starts_with("graph.wire.") || (command == "edit.delete" && !editor.selected_wires.is_empty()) {
        let command = if command == "edit.delete" { "graph.wire.delete" } else { command };
        let target = editor.command_target().map(materialcraft_engine::SelectionTarget::Function);
        crate::graph::wire_actions::edit(app, &mut editor.selected_wires, target, command);
        return true;
    }

    let parameters = editor.selection_parameters(command);
    crate::graph::selection_commands::invoke(app, &mut editor.selection, command, parameters);
    if matches!(command, "edit.selectAll" | "edit.selectNone" | "edit.paste" | "edit.pasteWithoutLinks" | "edit.duplicate" | "edit.duplicateWithoutLinks") {
        editor.selected_wires.clear();
    }
    true
}

pub(crate) fn invoke(app: &mut MaterialCraftApp, command: &str) -> bool {
    let Some(mut editor) = app.function_editor.take() else { return false };

    let handled = invoke_editor(app, &mut editor, command);
    app.function_editor = Some(editor);
    handled
}
