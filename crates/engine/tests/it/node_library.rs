use materialcraft_engine::{
    Session,
    core::{FunctionGraph, Graph, Package},
    execute, new_session,
};
use serde_json::{Value, json};

fn entries(session: &mut Session, parameters: Value) -> Vec<Value> {
    execute(session, "library.tree", &parameters).unwrap()["entries"].as_array().unwrap().clone()
}

#[test]
fn the_library_tree_lists_categories_and_filters_entries() {
    let mut session = new_session();
    let tree = execute(&mut session, "library.tree", &json!({})).unwrap();
    let names: Vec<_> = tree["categories"].as_array().unwrap().iter().map(|category| category["name"].as_str().unwrap()).collect();
    assert_eq!(names[..4], ["Graph items", "Atomic nodes", "FxMap nodes", "Function nodes"]);
    assert!(names.contains(&"MCF functions") && !names.contains(&"SDF functions"));
    assert_eq!(tree["layouts"], json!(["sbs://all_nodes.sbs"]));

    let noises = entries(&mut session, json!({"category": "Texture generators/Noises"}));
    assert_eq!(noises.len(), 89);
    assert!(noises.iter().all(|entry| entry["missing"].is_string()));

    let functions = entries(&mut session, json!({"canvas": "function", "filter": "pi"}));
    assert!(functions.iter().any(|entry| entry["id"] == "sbs://functions.sbs#Functions/Math/Pi" && entry["type"] == "functionInstance"));
    assert!(functions.iter().all(|entry| entry["canvas"] == "function"));
    assert!(execute(&mut session, "library.tree", &json!({"canvas": "teapot"})).is_err());
}

#[test]
fn library_entries_add_to_the_graph_they_fit() {
    let mut session = new_session();
    let before = session.graph().unwrap().nodes.len();
    let added = execute(&mut session, "library.add", &json!({"entry": "node:blur", "at": [32.0, 64.0]})).unwrap();
    assert_eq!(added["entry"], "node:blur");
    let graph = session.graph().unwrap();
    assert_eq!(graph.nodes.len(), before + 1);
    assert_eq!(graph.nodes.last().unwrap().def, "blur");
    assert_eq!(graph.nodes.last().unwrap().pos, [32.0, 64.0]);

    for (entry, check) in [("item:comment", "comments"), ("item:frame", "frames"), ("item:pin", "pins")] {
        let count = |session: &Session| serde_json::to_value(session.graph().unwrap()).unwrap()[check].as_array().map_or(0, Vec::len);
        let previous = count(&session);
        execute(&mut session, "library.add", &json!({"entry": entry})).unwrap();
        assert_eq!(count(&session), previous + 1, "{entry}");
    }

    // Library nodes MaterialCraft lacks are listed but refuse to add, naming what they need
    let error = execute(&mut session, "library.add", &json!({"entry": "sbs://3d_perlin_noise.sbs#3D_perlin_noise"})).unwrap_err().to_string();
    assert!(error.contains("3d_perlin_noise.sbs"), "{error}");
    assert!(execute(&mut session, "library.add", &json!({"entry": "nothing"})).is_err());
    assert!(execute(&mut session, "library.add", &json!({})).is_err());
    // Function entries need a Function canvas
    assert!(execute(&mut session, "library.add", &json!({"entry": "function:add"})).is_err());
    assert!(execute(&mut session, "library.add", &json!({"entry": "pattern:paramset"})).is_err());

    let mut package = Package::new("Functions");
    let mut graph = Graph::new("Caller");
    graph.function = Some(FunctionGraph::default());
    package.graphs.push(graph);
    let mut session = Session::new(package);
    execute(&mut session, "library.add", &json!({"entry": "function:add", "at": [0.0, 0.0]})).unwrap();
    execute(&mut session, "library.add", &json!({"entry": "sbs://functions.sbs#Functions/Math/Pi", "at": [140.0, 0.0]})).unwrap();
    let function = session.graph().unwrap().function.as_ref().unwrap();
    assert_eq!(function.nodes.len(), 2);
    assert!(function.nodes.iter().any(|node| node.op == "add"));
    assert!(function.nodes.iter().any(|node| node.instance.as_ref().is_some_and(|instance| instance.graph == "Functions/Math/Pi")));
    assert!(execute(&mut session, "library.add", &json!({"entry": "node:blur"})).is_err());
    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert_eq!(session.graph().unwrap().function.as_ref().unwrap().nodes.len(), 1);
}

#[test]
fn reloading_the_library_keeps_open_work_unmodified() {
    let mut session = new_session();
    let reloaded = execute(&mut session, "library.reload", &json!({})).unwrap();
    assert!(reloaded["entries"].as_u64().unwrap() > 500);
    assert!(reloaded["available"].as_u64().unwrap() > 200);
    assert!(!session.modified);
    assert!(session.pkg.dependencies.contains_key("sbs://functions.sbs"));
}
