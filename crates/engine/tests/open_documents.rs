use materialcraft_engine::{Session, core::Package, execute, file_cmds, formats, inspect};
use serde_json::json;

#[test]
fn opening_another_package_preserves_edits_selection_and_undo() {
    let mut session = Session::new(Package::new("First"));
    execute(&mut session, "file.new", &json!({"template": "empty", "name": "Source"})).unwrap();
    let added = execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let history = session.history();
    let selection = session.selection.clone();
    let original = session.pkg.clone();
    let bytes = formats::sbs::write_sbs(&Package::new("Second")).unwrap();
    file_cmds::open_bytes(&mut session, &bytes, Some("second.sbs".into())).unwrap();
    assert_eq!(inspect::session(&session, None)["documents"].as_array().unwrap().len(), 2);
    execute(&mut session, "package.activate", &json!({"document": 0})).unwrap();
    assert_eq!(session.pkg, original);
    assert_eq!(session.selection, selection);
    assert_eq!(session.history(), history);
    assert!(session.modified);
    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert!(session.graph().unwrap().nodes.iter().all(|node| node.id.0 != added["id"].as_u64().unwrap()));
}
