use crate::{
    Result, Session, cmd,
    commands::{CommandSpec, always, bad, strp},
};
use serde_json::Value;

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "file.importVector",
        "Import vector graphics...",
        ["File"],
        None,
        "{path, name?} imports an SVG into Resources, preserving its original bytes without adding a graph node",
        always,
        import_vector
    )]
}

fn import_vector(session: &mut Session, parameters: &Value) -> Result<Value> {
    let path = strp(parameters, "path").ok_or_else(|| bad("file.importVector", "`path` is required"))?;
    if !path.to_ascii_lowercase().ends_with(".svg") {
        return Err(bad("file.importVector", "Choose an SVG file"));
    }
    crate::file_cmds::import_resource_file(session, parameters, true)
}
