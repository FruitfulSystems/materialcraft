use super::function_port::FunctionPort;
use super::{FunctionDrag, FunctionEditor};
use crate::{
    MaterialCraftApp,
    graph::{GraphCamera, canvas_geometry::CanvasGeometry},
    theme::Tokens,
};
use egui::{Pos2, Stroke, Ui};
use materialcraft_engine::FunctionEdit;

pub(super) fn handle(
    app: &mut MaterialCraftApp,
    editor: &mut FunctionEditor,
    ui: &mut Ui,
    response: &egui::Response,
    ports: &[FunctionPort],
    geometry: &CanvasGeometry,
    on_annotation: bool,
) {
    let canvas_bounds = response.rect;
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let painter = ui.painter_at(canvas_bounds);
    let tokens = Tokens::get(ui.ctx());
    let snap = app.ui.prefs.snap_to_grid;
    if crate::graph::gesture::cancelled(ui) {
        editor.drag = None;
        return;
    }
    let on_wire = editor.drag.is_none()
        && !on_annotation
        && !ui.input(|input| input.modifiers.alt)
        && crate::graph::wire_selection::handle(&mut editor.selected_wires, &mut editor.selection, ui, response, &geometry.wires);
    if on_wire
        && matches!(editor.target, super::FunctionTarget::Graph)
        && app.session.annotation_selection.is_some()
        && (ui.input(|input| input.pointer.primary_pressed()) || response.secondary_clicked())
    {
        app.run("graph.annotation.select", serde_json::json!({}));
    }

    // Gestures: wires from output dots, panning
    let near = |pointer: Pos2| crate::graph::port_hit::nearest_by(ports, pointer, |port| (port.position, port.radius)).map(|port| (port.node, port.slot));
    if editor.drag.is_none() && GraphCamera::pan_started(ui, canvas_bounds) {
        editor.drag = Some(FunctionDrag::Pan(crate::graph::gesture::pan_button(ui)));
    } else if ui.input(|input| input.pointer.primary_pressed())
        && editor.drag.is_none()
        && !app.graph.is_dragging()
        && let Some(pointer_position) = pointer
        && ui.rect_contains_pointer(canvas_bounds)
        && let Some((id, None)) = near(pointer_position)
    {
        editor.drag = Some(FunctionDrag::Wire(id));
    }

    if editor.drag.is_none()
        && !app.graph.is_dragging()
        && response.hovered()
        && ui.input(|input| input.pointer.primary_pressed())
        && let Some(start) = pointer
        && !on_annotation
        && !on_wire
    {
        if matches!(editor.target, super::FunctionTarget::Graph) && app.session.annotation_selection.is_some() {
            app.run("graph.annotation.select", serde_json::json!({}));
        }
        editor.drag = Some(FunctionDrag::Box { start });
        crate::graph::node_selection::select(&mut editor.selected_wires, None, ui.input(|input| input.modifiers));
    }

    let delta = ui.input(|input| input.pointer.delta());
    match &mut editor.drag {
        Some(FunctionDrag::Node(_, offset)) => *offset += delta / editor.zoom,
        Some(FunctionDrag::Pan(_)) => editor.pan -= delta / editor.zoom,
        Some(FunctionDrag::Wire(id)) => {
            if let (Some(pointer_position), Some(source_id)) = (pointer, ports.iter().find(|port| port.node == *id && port.slot.is_none())) {
                crate::graph::wire_geometry::bezier(&painter, source_id.position, pointer_position, Stroke::new(2.0, tokens.accent));
            }
        }
        Some(FunctionDrag::Box { start }) => {
            if let Some(pointer) = pointer {
                crate::graph::node_selection::paint_box(&painter, *start, pointer, tokens.accent);
            }
        }
        None => {}
    }

    let owner = match editor.drag {
        Some(FunctionDrag::Pan(button)) => button,
        _ => egui::PointerButton::Primary,
    };
    if crate::graph::gesture::finished(ui, owner) {
        match editor.drag.take() {
            Some(FunctionDrag::Node(id, offset)) => {
                if offset.length() > 0.5 {
                    let mut ids = editor.selection.clone();
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                    editor.edit(app, FunctionEdit::Move { ids, by: offset.into(), snap }, false);
                }
            }
            Some(FunctionDrag::Box { start }) => {
                if let Some(pointer) = pointer {
                    let nodes = geometry.rects.iter().map(|(id, bounds)| (id.0 as u32, *bounds));
                    crate::graph::node_selection::select_box(&mut editor.selection, nodes, start, pointer, ui.input(|input| input.modifiers));
                }
            }
            Some(FunctionDrag::Wire(from)) => {
                if let Some(pointer) = pointer
                    && let Some((to, Some(input))) = near(pointer)
                    && from != to
                {
                    editor.edit(app, FunctionEdit::Connect { from, to, input }, false);
                }
            }
            _ => {}
        }
    }

    if response.clicked() && !on_wire && !ui.input(|input| input.modifiers.alt) {
        crate::graph::node_selection::select(&mut editor.selection, None, ui.input(|input| input.modifiers));
        crate::graph::node_selection::select(&mut editor.selected_wires, None, ui.input(|input| input.modifiers));
    }

    let camera = GraphCamera { origin: canvas_bounds.min, pan: editor.pan, zoom: editor.zoom };

    if let Some(payload) = crate::widgets::dnd_release::<super::function_library_drag::FunctionLibraryDrag>(response)
        && let Some(pointer) = pointer
    {
        let position = camera.to_graph(pointer);
        super::library::insert(app, editor, &payload, [position.x - crate::graph::NODE_W * 0.5, position.y - 20.0]);
    }

    if let Some(payload) = crate::widgets::dnd_release::<crate::graph_drag_payload::GraphDragPayload>(response)
        && let Some(pointer) = pointer
        && let Some(target) = editor.command_target()
    {
        let at = camera.to_graph(pointer);
        if let Some(result) = app.run(
            "graph.instance.add",
            serde_json::json!({"document": payload.document, "graph": payload.name,
            "function": target, "at": [at.x - crate::graph::NODE_W * 0.5, at.y - 20.0], "snapToGrid": snap}),
        ) && let Some(id) = result.get("id").and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok())
        {
            editor.selection = vec![id];
            editor.selected_wires.clear();
            editor.inspected_node = Some(id);
        }
    }

    // Adding operations
    if response.secondary_clicked()
        && let Some(pointer_position) = pointer
    {
        editor.menu_at = camera.to_graph(pointer_position);
    }
    if response.hovered()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|input| input.key_pressed(egui::Key::Space))
        && let Some(pointer_position) = pointer
    {
        editor.menu_at = camera.to_graph(pointer_position);
        egui::Popup::open_id(ui.ctx(), response.id.with("menu"));
    }
    response.context_menu(|ui| {
        super::context_menu::show(app, editor, ui, None);
    });
}
