//! Preferences ▸ Graph ▸ "Automatically insert color/grayscale conversion node": a wire from a
//! color output to a grayscale input gets a Grayscale Conversion node in between, one from a
//! grayscale output to a color input a Gradient Map

use materialcraft_engine::core::NodeId;
use materialcraft_engine::nodes::PortKind;
use serde_json::json;

use crate::MaterialCraftApp;

/// The node that converts the kinds between an output and an input (`None` = the kinds agree
/// or the preference is off)
fn conversion_node(app: &MaterialCraftApp, from: &(NodeId, String), to: NodeId, to_port: &str) -> Option<&'static str> {
    if !app.ui.prefs.auto_convert {
        return None;
    }
    let g = app.session.graph().ok()?;
    let out_kind = materialcraft_engine::eval::ports(&app.session.pkg, g, g.node(from.0).ok()?).1.into_iter().find(|p| p.id == from.1)?.kind;
    let in_kind = materialcraft_engine::eval::ports(&app.session.pkg, g, g.node(to).ok()?).0.into_iter().find(|p| p.id == to_port)?.kind;
    match (out_kind, in_kind) {
        (PortKind::Color, PortKind::Gray) => Some("grayscale_conversion"),
        (PortKind::Gray, PortKind::Color) => Some("gradient_map"),
        _ => None,
    }
}

/// Connect `from` to `to`'s input (detaching `detach` first), inserting a conversion node when
/// the preference asks for one. One undo step either way
pub(super) fn connect(app: &mut MaterialCraftApp, from: &(NodeId, String), to: NodeId, to_port: &str, detach: Option<&(NodeId, String)>) {
    let Some(converter) = conversion_node(app, from, to, to_port) else {
        let mut parameters = json!({"from": from.0.0, "fromPort": from.1, "to": to.0, "toPort": to_port});
        if let Some((node, port)) = detach {
            parameters["detach"] = json!({"to": node.0, "toPort": port});
        }
        app.run("graph.connect", parameters);
        return;
    };
    let g = app.session.graph().ok();
    let pos = |id: NodeId| g.and_then(|g| g.node(id).ok()).map_or([0.0, 0.0], |n| n.pos);
    let (pa, pb) = (pos(from.0), pos(to));
    let at = [(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5];
    let snap = app.ui.prefs.snap_to_grid;
    let detach = detach.cloned();
    let from = from.clone();
    let to_port = to_port.to_string();
    let result = app.session.group("Connect", |session| {
        if let Some((node, port)) = &detach {
            materialcraft_engine::execute(session, "graph.disconnect", &json!({"to": node.0, "toPort": port}))?;
        }
        let added = materialcraft_engine::execute(
            session,
            "graph.node.add",
            &json!({"def": converter, "at": at, "snapToGrid": snap, "connectFrom": {"node": from.0.0, "port": from.1}}),
        )?;
        let id = added
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| materialcraft_engine::EngineError::Other("the conversion node was not added".into()))?;
        materialcraft_engine::execute(session, "graph.connect", &json!({"from": id, "to": to.0, "toPort": to_port}))?;
        Ok(())
    });
    if let Err(error) = result {
        app.toast(error.to_string(), true);
    }
}
