//! Editing a navigation pin after it was added: its name and where it sits

use crate::commands::{CommandSpec, bad, has_graph, point, strp, u64p};
use crate::{Result, Session, cmd};
use serde_json::{Value as J, json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![cmd!("graph.pin.edit", "Edit pin", [], None, "{id, name?, at? [x, y]}", has_graph, pin_edit)]
}

fn pin_edit(session: &mut Session, parameters: &J) -> Result<J> {
    let id = u64p(parameters, "id").ok_or_else(|| bad("graph.pin.edit", "`id` is required"))?;
    let name = strp(parameters, "name").map(str::trim).filter(|name| !name.is_empty()).map(str::to_string);
    let at = point(parameters, "at");
    if name.is_none() && at.is_none() {
        return Err(bad("graph.pin.edit", "`name` or `at` is required"));
    }
    let label = if at.is_some() { "Move pin" } else { "Rename pin" };
    session.edit(label, |session| {
        let pin = session.graph_mut()?.pins.iter_mut().find(|pin| pin.id == id).ok_or_else(|| bad("graph.pin.edit", format!("no pin {id}")))?;
        if let Some(name) = name {
            pin.name = name;
        }
        if let Some(at) = at {
            pin.pos = at;
        }
        Ok(json!({"id": pin.id, "name": pin.name, "at": pin.pos}))
    })
}
