use super::{GraphCamera, GraphDrag, LibraryDrag, NODE_W, PortHit, WireRef};
use super::{navigation::frame, port_hit::nearest as nearest_port, search::open_search, wire_actions::wire_menu};
use crate::MaterialCraftApp;
use egui::{Rect, Ui, Vec2};
use materialcraft_engine::core::NodeId;
use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle(
    app: &mut MaterialCraftApp,
    ui: &mut Ui,
    canvas: &egui::Response,
    rect: Rect,
    camera: &GraphCamera,
    nodes: &[(NodeId, egui::Response)],
    frames: &[super::frame_hit::FrameHit],
    comments: &[(u64, Rect)],
    port_hover: Option<PortHit>,
    wire_hover: Option<WireRef>,
) {
    let context = ui.ctx().clone();
    if super::gesture::cancelled(ui) {
        app.graph.drag = None;
        return;
    }

    let (pointer, modifiers, primary_pressed) = context.input(|input| (input.pointer.interact_pos(), input.modifiers, input.pointer.primary_pressed()));
    let zoom = camera.zoom;
    let motion = context.input(|input| input.pointer.delta()) / zoom;

    // Starting gestures (only on a press inside the canvas, with no gesture running)
    if app.graph.drag.is_none()
        && let Some(position) = pointer
        && rect.contains(position)
        && ui.rect_contains_pointer(rect)
        && app.graph.search.is_none()
    {
        if GraphCamera::pan_started(ui, rect) {
            app.graph.drag = Some(GraphDrag::Pan(super::gesture::pan_button(ui)));
        } else if primary_pressed {
            if app.session.annotation_selection.is_some() {
                app.run("graph.annotation.select", json!({}));
            }

            let on_card = nodes.iter().any(|(_, response)| response.rect.contains(position)) || comments.iter().any(|(_, bounds)| bounds.contains(position));
            let wire = wire_hover.as_ref().filter(|_| port_hover.is_none() && !on_card);
            if wire.is_none() {
                super::node_selection::select(&mut app.graph.selected_wires, None, modifiers);
            }

            if let Some(wire) = wire {
                app.graph.drag = Some(GraphDrag::WirePress { wire: wire.clone(), start: position });
            } else if let Some(port) = &port_hover {
                if port.output {
                    app.graph.drag = Some(GraphDrag::Wire { from: (port.node, port.port.clone()), detached: None });
                } else if let Some(connection) = app.session.graph().ok().and_then(|graph| graph.input_of(port.node, &port.port).cloned()) {
                    app.graph.drag =
                        Some(GraphDrag::Wire { from: (connection.from, connection.from_port.clone()), detached: Some((port.node, port.port.clone())) });
                }
            } else if let Some((id, _)) = comments.iter().rev().find(|(_, bounds)| bounds.contains(position)) {
                super::annotation_input::begin_comment(app, *id);
            } else if let Some((id, _)) = nodes.iter().rev().find(|(_, response)| response.rect.contains(position)) {
                super::node_selection::select(&mut app.session.selection, Some(*id), modifiers);
                app.graph.drag = Some(GraphDrag::Nodes { delta: Vec2::ZERO });
            } else if let Some(hit) = frames.iter().rev().find(|frame| frame.contains(position)) {
                super::annotation_input::begin_frame(app, hit.id, hit.grip.contains(position));
            } else {
                app.graph.drag = Some(GraphDrag::Box { start: position });
            }
        }
    }

    // Continuing gestures
    match &mut app.graph.drag {
        Some(GraphDrag::Pan(_)) => {
            let delta = context.input(|input| input.pointer.delta()) / zoom;
            app.ui.graph_camera.pan = [app.ui.graph_camera.pan[0] - delta.x, app.ui.graph_camera.pan[1] - delta.y];
        }
        Some(GraphDrag::Nodes { delta }) | Some(GraphDrag::Frame { delta, .. }) | Some(GraphDrag::Comment { delta, .. }) => *delta += motion,
        Some(GraphDrag::WirePress { wire, start }) => {
            // Dragged far enough: a reroute dot where the press was, carried by the pointer
            if let Some(position) = pointer
                && position.distance(*start) > 4.0
            {
                let (wire, start) = (wire.clone(), *start);
                let at = camera.to_graph(start);
                app.graph.drag = None;
                if app.run("graph.wire.insertDot", json!({"to": wire.0.0, "toPort": wire.1, "at": [at.x, at.y]})).is_some() {
                    app.graph.drag = Some(GraphDrag::Nodes { delta: (position - start) / zoom });
                }
            }
        }
        _ => {}
    }

    // Ending gestures
    let owner = match app.graph.drag {
        Some(GraphDrag::Pan(button)) => button,
        _ => egui::PointerButton::Primary,
    };
    let ended = super::gesture::finished(ui, owner);
    if ended && let Some(drag) = app.graph.drag.take() {
        match drag {
            GraphDrag::Pan(_) => {}
            GraphDrag::WirePress { wire, .. } => {
                super::wire_selection::select(&mut app.graph.selected_wires, &mut app.session.selection, wire, modifiers, false);
            }
            GraphDrag::Nodes { delta } => {
                if delta.length() > 0.5 {
                    app.run("graph.node.move", json!({"by": [delta.x, delta.y]}));
                }
            }
            GraphDrag::Box { start } => {
                if let Some(position) = pointer {
                    super::node_selection::select_box(&mut app.session.selection, app.graph.rects.iter().copied(), start, position, modifiers);
                }
            }
            GraphDrag::Wire { from, detached } => {
                let target = pointer.and_then(|position| nearest_port(&app.graph.ports, position, 12.0)).filter(|port| !port.output).cloned();
                match target {
                    Some(target) => {
                        let mut parameters = json!({"from": from.0.0, "fromPort": from.1, "to": target.node.0, "toPort": target.port});
                        if let Some((node, port)) = detached {
                            parameters["detach"] = json!({"to": node.0, "toPort": port});
                        }

                        app.run("graph.connect", parameters);
                    }
                    None => {
                        if let Some((node, port)) = &detached {
                            app.run("graph.disconnect", json!({"to": node.0, "toPort": port}));
                        } else if let Some(position) = pointer
                            && rect.contains(position)
                        {
                            open_search(app, camera.to_graph(position), Some(from));
                        }
                    }
                }
            }
            GraphDrag::Frame { .. } | GraphDrag::Comment { .. } => super::annotation_input::finish(app, drag),
        }
    }

    // Clicks
    for (id, response) in nodes {
        if response.double_clicked() {
            // Double-click previews outputs; embedded graphs require an explicit edit action
            // Linked instances open their sources through Open reference
            if app.session.graph().ok().and_then(|graph| graph.node(*id).ok()).is_some_and(|node| node.def != "graph_instance") {
                app.run("view.2d", json!({"node": id.0, "port": 0}));
            }
        } else if response.clicked() && app.session.selection.contains(id) && app.session.view2d.node != Some(*id) {
            // Keep the current output when deselecting or clicking the node already shown
            app.run("view.2d", json!({"node": id.0}));
        }
        if response.secondary_clicked()
            && let Some(pointer) = pointer
        {
            app.graph.menu_at = camera.to_graph(pointer);
        }

        let at = app.graph.menu_at;
        response.context_menu(|ui| super::context_menu::show(app, ui, at, Some(*id)));
    }

    if let Some(port) = &port_hover
        && port.output
        && ui.rect_contains_pointer(rect)
        && context.input(|input| input.pointer.primary_clicked())
    {
        let port_index = app.session.output_ports(port.node).iter().position(|identifier| *identifier == port.port).unwrap_or(0);
        app.run("view.2d", json!({"node": port.node.0, "port": port_index}));
    }

    let on_node = pointer.is_some_and(|position| {
        nodes.iter().any(|(_, response)| response.rect.contains(position))
            || comments.iter().any(|(_, bounds)| bounds.contains(position))
            || frames.iter().any(|frame| frame.contains(position))
    });
    if canvas.double_clicked()
        && !on_node
        && let Some(position) = pointer
    {
        match &wire_hover {
            // Double-click a wire: a reroute dot there
            Some(wire) => {
                let at = camera.to_graph(position);
                app.run("graph.wire.insertDot", json!({"to": wire.0.0, "toPort": wire.1, "at": [at.x, at.y]}));
                app.graph.selected_wires.clear();
            }
            None => open_search(app, camera.to_graph(position), None),
        }
    }

    if canvas.secondary_clicked()
        && let Some(position) = pointer
    {
        app.graph.menu_at = camera.to_graph(position);
        if let Some((id, _)) = comments.iter().rev().find(|(_, rect)| rect.contains(position)) {
            app.run("graph.annotation.select", json!({"kind": "comment", "id": id}));
        } else if let Some(hit) = frames.iter().rev().find(|frame| frame.contains(position)) {
            app.run("graph.annotation.select", json!({"kind": "frame", "id": hit.id}));
        }
        app.graph.menu_wire = wire_hover.clone().filter(|_| !on_node);
        if let Some(wire) = app.graph.menu_wire.clone() {
            super::wire_selection::select(&mut app.graph.selected_wires, &mut app.session.selection, wire, egui::Modifiers::NONE, true);
        }
    }

    let at = app.graph.menu_at;
    canvas.context_menu(|ui| match app.graph.menu_wire.clone() {
        Some(wire) => wire_menu(app, ui, wire, at),
        None => super::context_menu::show(app, ui, at, None),
    });

    if let Some(payload) = crate::widgets::dnd_release::<crate::graph_drag_payload::GraphDragPayload>(canvas)
        && let Some(pointer) = pointer
    {
        let at = camera.to_graph(pointer);
        app.run("graph.instance.add", json!({"document": payload.document, "graph": payload.name, "at": [at.x - NODE_W * 0.5, at.y - 20.0]}));
    }

    // Library drops
    if let Some(payload) = crate::widgets::dnd_release::<LibraryDrag>(canvas)
        && let Some(position) = pointer
    {
        let graph_position = camera.to_graph(position);
        let at = [graph_position.x - NODE_W * 0.5, graph_position.y - 20.0];
        app.run("graph.node.add", json!({"def": payload.0, "at": at}));
    }

    // Keys over the canvas
    let hovered = pointer.is_some_and(|position| rect.contains(position));
    #[cfg(not(target_arch = "wasm32"))]
    let update_dialog_open = crate::updates_ui::blocks_input(app);
    #[cfg(target_arch = "wasm32")]
    let update_dialog_open = false;
    if hovered && app.graph.search.is_none() && !update_dialog_open {
        let space = !context.egui_wants_keyboard_input() && context.input(|input| input.key_pressed(egui::Key::Space));
        let tab = !app.graph.keyboard_was_focused && !context.text_edit_focused() && context.input(|input| input.key_pressed(egui::Key::Tab));
        if (space || tab)
            && let Some(position) = pointer
        {
            open_search(app, camera.to_graph(position), None);
        }
        if GraphCamera::frame_key(ui, rect) {
            let selected = !app.session.selection.is_empty();
            frame(app, rect, selected);
        }
    }
}
