use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use materialcraft_engine::{
    Session,
    core::{FunctionGraph, Graph, Package},
};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

fn harness(render: bool) -> Harness<'static, MaterialCraftApp> {
    let mut package = Package::new("Source");
    let mut function = Graph::new("Helper");
    function.function = Some(FunctionGraph::default());
    function.attributes.description = "A reusable function.\nAnother line.".into();
    function.attributes.label = "Readable helper".into();
    function.attributes.author_url = "https://example.org".into();
    package.graphs = vec![Graph::new("Material"), function];
    let mut app = MaterialCraftApp::headless(Session::new(package), UiState::default());
    app.run("file.newPackage", json!({"name": "Canvas"})).unwrap();
    app.run("file.new", json!({"template": "empty", "name": "Open_graph"})).unwrap();
    let builder = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_step_dt(1.0 / 60.0);
    let builder = if render { builder.wgpu() } else { builder };
    let mut harness = builder.build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app,
    );
    harness.run_steps(8);
    harness
}

fn select(harness: &mut Harness<'static, MaterialCraftApp>, name: &str) {
    harness.get(By::new().role(egui::Role::TreeItem).label(name)).click();
    harness.run_steps(5);
}

fn field(label: &str) -> By<'_> {
    By::new().role(egui::Role::TextInput).label(label)
}

fn type_field(harness: &mut Harness<'static, MaterialCraftApp>, label: &str, text: &str) {
    harness.get(field(label)).click();
    harness.step();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.step();
    harness.event(Event::Text(text.into()));
    harness.run_steps(3);
}

#[test]
fn explorer_inspects_unopened_graphs_and_edits_the_selected_package() {
    let mut harness = harness(false);
    let active = (harness.state().session.document_id, harness.state().session.graph);

    select(&mut harness, "Helper");

    for label in ["Identifier", "Description", "Category", "Label", "Author", "Author URL", "Tags", "Group", "User Data"] {
        assert!(harness.query(field(label)).is_some(), "missing {label}");
    }
    assert_eq!(harness.get(field("Identifier")).value().as_deref(), Some("Helper"));
    assert_eq!(harness.get(field("Label")).value().as_deref(), Some("Readable helper"));
    assert_eq!(harness.get(field("Author URL")).value().as_deref(), Some("https://example.org"));
    assert_eq!((harness.state().session.document_id, harness.state().session.graph), active);

    type_field(&mut harness, "Description", "First edited line\nSecond edited line");
    select(&mut harness, "Material");

    let source = materialcraft_engine::GraphTarget { document: 0, graph: 0 }.package(&harness.state().session).unwrap();
    assert_eq!(source.graphs[1].attributes.description, "First edited line\nSecond edited line");
    assert_eq!(harness.get(field("Identifier")).value().as_deref(), Some("Material"));
    assert!(harness.query(field("Label")).is_some());
    for label in ["Description", "Category", "Author", "Author URL", "Tags", "Group", "User Data"] {
        assert!(harness.query(field(label)).is_none(), "unexpected regular graph attribute {label}");
    }
    assert!(harness.state().session.graph().unwrap().attributes.description.is_empty());

    type_field(&mut harness, "Label", "Surface label");
    harness.key_press(Key::Enter);
    harness.run_steps(4);
    assert_eq!(materialcraft_engine::GraphTarget { document: 0, graph: 0 }.get(&harness.state().session).unwrap().attributes.label, "Surface label");
    assert_eq!(materialcraft_engine::GraphTarget { document: 0, graph: 0 }.get(&harness.state().session).unwrap().name, "Material");
    assert_eq!((harness.state().session.document_id, harness.state().session.graph), active);
}

#[test]
fn identifier_edit_commits_before_switching_the_inspector() {
    let mut harness = harness(false);
    select(&mut harness, "Helper");
    type_field(&mut harness, "Identifier", "Renamed_helper");
    select(&mut harness, "Material");

    let source = materialcraft_engine::GraphTarget { document: 0, graph: 0 }.package(&harness.state().session).unwrap();
    assert_eq!(source.graphs[1].name, "Renamed_helper");
    assert_eq!(source.graphs[1].attributes.label, "Readable helper");
    assert_eq!(harness.state().session.graph().unwrap().name, "Open_graph");
}

#[test]
#[ignore = "requires an offscreen GPU"]
fn graph_properties_screenshot() {
    let mut harness = harness(true);
    select(&mut harness, "Helper");
    let directory = std::path::PathBuf::from(std::env::var_os("MC_SNAPSHOT_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    harness.render().unwrap().save(directory.join("function-graph-properties.png")).unwrap();

    select(&mut harness, "Material");
    harness.render().unwrap().save(directory.join("material-graph-properties.png")).unwrap();
}
