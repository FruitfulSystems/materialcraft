use super::{By, MaterialCraftApp, NodeT, Queryable, UiState, harness, json, save, settle};
use egui::accesskit::Role;

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn explorer_selection_does_not_open_documents_or_graphs() {
    let directory = std::path::PathBuf::from(std::env::var_os("MC_SNAPSHOT_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.session.pkg.name = "Source".into();
    app.run("graph.rename", json!({"name": "Material"})).unwrap();
    let output = app.run("graph.node.add", json!({"def": "output", "params": {"identifier": "result"}})).unwrap()["id"].as_u64().unwrap();
    app.run("graph.node.add", json!({"def": "input_color", "params": {"identifier": "input"}})).unwrap();
    app.run("folder.create", json!({"name": "Tools"})).unwrap();
    app.run("file.new", json!({"folder": "Tools", "kind": "function", "name": "Helper"})).unwrap();
    let source = app.session.document_id;
    app.run("file.newPackage", json!({"name": "Active"})).unwrap();
    let active = (app.session.document_id, app.session.graph);
    let selection = app.session.selection.clone();
    let view = app.session.view2d.node;
    let mut harness = harness(app, 1600.0, 960.0);
    settle(&mut harness, 5);

    for label in ["Source*", "Tools", "Material", "Helper", "Result", "Input"] {
        let position = harness.get(By::new().role(Role::TreeItem).label(label)).rect().center();
        super::click_canvas(&mut harness, position);

        assert_eq!((harness.state().session.document_id, harness.state().session.graph), active, "single click on {label}");
        assert_eq!(harness.state().session.selection, selection, "single click on {label}");
        assert_eq!(harness.state().session.view2d.node, view, "single click on {label}");
    }
    save(&mut harness, &directory, "explorer-select-without-opening");

    let position = harness.get(By::new().role(Role::TreeItem).label("Helper")).rect().center();
    super::double_click(&mut harness, position);
    assert_eq!(harness.state().session.document_id, source);
    assert_eq!(harness.state().session.graph().unwrap().name, "Helper");

    let position = harness.get(By::new().role(Role::TreeItem).label("Material")).rect().center();
    super::click_canvas(&mut harness, position);
    assert_eq!(harness.state().session.graph().unwrap().name, "Helper");
    super::double_click(&mut harness, position);
    assert_eq!(harness.state().session.graph().unwrap().name, "Material");

    let position = harness.get(By::new().role(Role::TreeItem).label("Result")).rect().center();
    super::double_click(&mut harness, position);
    assert_eq!(harness.state().session.selection, vec![materialcraft_engine::core::NodeId(output)]);
    assert_eq!(harness.state().session.view2d.node, Some(materialcraft_engine::core::NodeId(output)));
    save(&mut harness, &directory, "explorer-double-click-open");
}

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn explorer_folder_tree_and_context_menus() {
    let directory = std::path::PathBuf::from(std::env::var_os("MC_SNAPSHOT_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.session.pkg.name = "Explorer menus".into();
    app.run("folder.create", json!({"name": "Tools"})).unwrap();
    app.run("folder.create", json!({"parent": "Tools", "name": "Math"})).unwrap();
    app.run("file.new", json!({"folder": "Tools/Math", "kind": "function", "name": "Helper"})).unwrap();
    app.run("file.new", json!({"folder": "Tools", "template": "empty", "name": "Surface"})).unwrap();
    let mut harness = harness(app, 1600.0, 960.0);
    settle(&mut harness, 6);
    fn tree(label: &str) -> By<'_> {
        By::new().role(Role::TreeItem).label(label)
    }
    fn button(label: &str) -> By<'_> {
        By::new().role(Role::Button).label(label)
    }
    let tools = harness.get(tree("Tools")).rect();
    let math = harness.get(tree("Math")).rect();
    let helper = harness.get(tree("Helper")).rect();
    assert!(tools.left() < math.left() && math.left() < helper.left());
    save(&mut harness, &directory, "explorer-folders");

    for (label, expected, screenshot) in [
        ("Tools", vec!["New", "Copy", "Paste", "Rename", "Remove"], "explorer-folder-menu"),
        ("Explorer menus*", vec!["New", "Reload", "Save", "Save As", "Save A Copy As", "Copy", "Paste", "Close"], "explorer-package-menu"),
        ("Helper", vec!["Open", "Copy", "Paste", "Remove", "Rename"], "explorer-function-menu"),
        ("Surface", vec!["Open", "Copy", "Paste", "Remove", "Rename"], "explorer-graph-menu"),
    ] {
        harness.get(tree(label)).click_secondary();
        settle(&mut harness, 3);
        for label in expected {
            assert!(harness.query(By::new().label(label)).is_some(), "{screenshot}: {label}");
        }
        for label in ["Duplicate", "Delete", "Import", "Link"] {
            assert!(harness.query(button(label)).is_none(), "{screenshot}: unexpected {label}");
        }
        assert!(harness.get(button("Paste")).accesskit_node().is_disabled());
        save(&mut harness, &directory, screenshot);
        harness.key_press(egui::Key::Escape);
        settle(&mut harness, 2);
    }

    harness.get(tree("Helper")).click_secondary();
    settle(&mut harness, 2);
    harness.get(button("Copy")).click();
    settle(&mut harness, 2);
    harness.get(tree("Tools")).click_secondary();
    settle(&mut harness, 2);
    harness.get(button("Paste")).click();
    settle(&mut harness, 3);
    assert!(harness.state().session.pkg.graph_by_name("Tools/Helper").is_some());

    harness.get(tree("Math")).click_secondary();
    settle(&mut harness, 2);
    harness.get(By::new().label("New")).click();
    settle(&mut harness, 3);
    for label in ["Graph", "Function Graph", "Folder"] {
        assert!(harness.query(button(label)).is_some());
    }
    save(&mut harness, &directory, "explorer-new-submenu");
    harness.get(button("Function Graph")).click();
    settle(&mut harness, 3);
    assert!(!harness.state().ui.windows.new_graph);
    assert!(harness.state().ui.explorer.rename.is_none());
    assert_eq!(harness.state().session.graph().unwrap().folder, "Tools/Math");
    assert_eq!(harness.state().session.graph().unwrap().name, "Untitled_Function_Graph_1");
    assert!(harness.state().session.graph().unwrap().function.is_some());

    for (label, expected) in [
        ("Function Graph", "Untitled_Function_Graph_2"),
        ("Graph", "Untitled_Graph_1"),
        ("Graph", "Untitled_Graph_2"),
        ("Folder", "Untitled_Folder_1"),
        ("Folder", "Untitled_Folder_2"),
    ] {
        harness.get(tree("Math")).click_secondary();
        settle(&mut harness, 2);
        harness.get(By::new().label("New")).click();
        settle(&mut harness, 2);
        harness.get(button(label)).click();
        settle(&mut harness, 3);

        assert!(!harness.state().ui.windows.new_graph);
        assert!(harness.state().ui.explorer.rename.is_none());
        let path = format!("Tools/Math/{expected}");
        if label == "Folder" {
            assert!(harness.state().session.pkg.folder_paths().contains(&path));
        } else {
            assert!(harness.state().session.pkg.graph_by_name(&path).is_some());
        }
    }
    save(&mut harness, &directory, "explorer-create-without-prompts");
}
