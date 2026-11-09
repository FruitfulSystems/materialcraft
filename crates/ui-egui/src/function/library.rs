pub(crate) use super::function_library_drag::FunctionLibraryDrag;
use crate::MaterialCraftApp;
use materialcraft_engine::FunctionEdit;

/// Add `entry` to the open Function canvas, after its last node
pub(crate) fn add(app: &mut MaterialCraftApp, entry: &FunctionLibraryDrag) {
    if let Some(mut editor) = app.function_editor.take() {
        let at = editor.current(app).map_or([0.0; 2], |graph| [graph.nodes.len() as f32 * 140.0, 0.0]);
        insert(app, &mut editor, entry, at);
        app.function_editor = Some(editor);
    }
}

/// The native operations the open Function canvas can take (processors that only compute a
/// value cannot sample images)
pub(crate) fn available_operations(app: &MaterialCraftApp) -> Vec<&'static str> {
    app.function_editor
        .as_ref()
        .and_then(|editor| editor.command_target())
        .map(|target| target.available_operations(&app.session).map(|definition| definition.id).collect())
        .unwrap_or_default()
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
