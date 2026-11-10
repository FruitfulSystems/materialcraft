use super::*;
use materialcraft_engine::core::NodeId;
use serde_json::json;

fn app() -> MaterialCraftApp {
    MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default())
}

fn labels(app: &MaterialCraftApp) -> Vec<String> {
    let entries = app.session.library_entries();
    visible(app, &entries, &Here::of(app)).into_iter().map(|entry| entry.label.clone()).collect()
}

#[test]
fn open_package_graphs_are_explorer_resources_not_library_entries() {
    let mut app = app();
    let mut graph = materialcraft_engine::core::Graph::new("Reusable source");
    graph.add_node("output", [0.0; 2]);
    app.session.pkg.graphs.push(graph);
    app.ui.library_search = "Reusable source".into();
    assert!(labels(&app).is_empty());
    app.ui.library_search.clear();
    app.ui.library_category = "@package".into();
    assert!(labels(&app).is_empty(), "an unknown category lists nothing");
    app.ui.library_category.clear();
    assert!(labels(&app).iter().any(|label| label == "Uniform color"));
}

#[test]
fn categories_include_their_sub_categories_and_favorites_follow_the_star() {
    let mut app = app();
    app.ui.library_category = "Functions".into();
    let functions = labels(&app);
    app.ui.library_category = "Functions/Constant".into();
    let constants = labels(&app);
    assert!(!constants.is_empty() && constants.len() < functions.len());
    assert!(constants.iter().all(|label| functions.contains(label)));

    app.ui.library_category = FAVORITES.into();
    assert!(labels(&app).is_empty());
    toggle_favorite(&mut app, "node:blur");
    toggle_favorite(&mut app, "sbs://functions.sbs#Functions/Math/Pi");
    assert_eq!(labels(&app), ["Blur", "Pi"]);
    toggle_favorite(&mut app, "node:blur");
    assert_eq!(labels(&app), ["Pi"]);
}

#[test]
fn search_puts_what_the_open_graph_takes_first_and_filters_hide_the_rest() {
    let mut app = app();
    // "Uniform" names a native node and random functions; a material graph takes only the node
    app.ui.library_search = "uniform".into();
    let found = labels(&app);
    assert_eq!(found.first().map(String::as_str), Some("Uniform color"), "{found:?}");
    assert!(found.len() > 1, "{found:?}");
    let entries = app.session.library_entries();
    let here = Here::of(&app);
    let shown = visible(&app, &entries, &here);
    let first_refused = shown.iter().position(|entry| !here.takes(entry)).unwrap();
    assert!(shown[first_refused..].iter().all(|entry| !here.takes(entry)), "addable entries come first");

    app.ui.library_show_missing = false;
    app.ui.library_show_other = false;
    let entries = app.session.library_entries();
    assert!(visible(&app, &entries, &here).iter().all(|entry| here.takes(entry)));
}

#[test]
fn processor_functions_take_operations_their_canvas_allows() {
    let mut app = app();
    let owner = NodeId(app.run("graph.node.add", json!({"def": "value_processor"})).unwrap()["id"].as_u64().unwrap());
    app.function_editor = Some(crate::function::FunctionEditor::new(app.session.graph, owner, crate::function::FunctionTarget::Parameter("function".into())));
    let here = Here::of(&app);
    let entries = app.session.library_entries();
    let find = |id: &str| entries.iter().find(|entry| entry.id == id).unwrap();
    assert!(here.takes(find("function:add")));
    assert!(here.takes(find("sbs://functions.sbs#Functions/Math/Pi")));
    assert!(!here.takes(find("function:samplecol")), "value processors can't sample images");
    app.ui.library_search = "sample".into();
    assert!(!labels(&app).iter().any(|label| label == "Sample color"), "and the library hides what the canvas can't take");
    assert!(!here.takes(find("node:blur")));
    assert!(here.refusal(find("node:blur")).is_some());
}

#[test]
fn every_tree_category_has_an_icon_and_double_click_adds() {
    for (top, subs) in CATEGORIES {
        assert_ne!(category_icon(top), icon::FOLDER_SIMPLE, "{top}");
        for sub in *subs {
            assert_ne!(category_icon(&format!("{top}/{sub}")), icon::FOLDER_SIMPLE, "{top}/{sub}");
        }
    }

    let mut app = app();
    let before = app.session.graph().unwrap().nodes.len();
    let entries = app.session.library_entries();
    // A material graph takes the Blur node, not a function
    for id in ["node:blur", "sbs://functions.sbs#Functions/Math/Pi"] {
        let entry = entries.iter().find(|entry| entry.id == id).unwrap();
        if Here::of(&app).takes(entry) {
            add_entry(&mut app, entry);
        }
    }
    assert_eq!(app.session.graph().unwrap().nodes.len(), before + 1, "only the entry the graph takes is added");
    let comments = app.session.graph().unwrap().comments.len();
    add_entry(&mut app, entries.iter().find(|entry| entry.id == "item:comment").unwrap());
    assert_eq!(app.session.graph().unwrap().comments.len(), comments + 1);
}
