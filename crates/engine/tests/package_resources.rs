use materialcraft_engine::{Session, core::Package, execute, file_cmds};
use serde_json::json;

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"/></svg>"#;

#[test]
fn resources_is_reserved_for_assets() {
    let mut session = Session::new(Package::new("Resources test"));
    assert!(session.pkg.folder_paths().contains("Resources"));
    for (command, params) in [
        ("file.new", json!({"folder": "Resources"})),
        ("folder.create", json!({"name": "Resources"})),
        ("folder.create", json!({"parent": "Resources", "name": "Graphs"})),
        ("folder.rename", json!({"folder": "Resources", "name": "Other"})),
        ("folder.remove", json!({"folder": "Resources"})),
        ("explorer.move", json!({"folder": "Resources", "parent": ""})),
        ("explorer.copy", json!({"folder": "Resources"})),
    ] {
        assert!(execute(&mut session, command, &params).is_err(), "{command}: {params}");
    }
    execute(&mut session, "file.new", &json!({"template": "empty"})).unwrap();
    execute(&mut session, "folder.create", &json!({"name": "Other"})).unwrap();
    execute(&mut session, "explorer.copy", &json!({"graph": 0})).unwrap();
    for (command, params) in [
        ("folder.rename", json!({"folder": "Other", "name": "Resources"})),
        ("explorer.move", json!({"graph": 0, "parent": "Resources"})),
        ("explorer.paste", json!({"folder": "Resources"})),
    ] {
        assert!(execute(&mut session, command, &params).is_err());
    }
}

#[test]
fn unsaved_resources_save_independently_as_sbs_and_mcraft() {
    let root = std::env::temp_dir().join(format!("materialcraft-resources-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut session = Session::new(Package::new("Wood"));
    file_cmds::add_resource(&mut session, "mask.svg", SVG.to_vec(), Some("Mask")).unwrap();
    assert!(session.path.is_none());
    assert!(session.modified);
    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert!(session.pkg.resources.is_empty());
    execute(&mut session, "edit.redo", &json!({})).unwrap();

    for extension in ["sbs", "mcraft"] {
        let directory = root.join(extension);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("wood.{extension}"));
        execute(&mut session, "file.saveAs", &json!({"path": path})).unwrap();
        assert!(path.is_file(), "save must honor the requested format");
        if extension == "sbs" {
            let xml = std::fs::read_to_string(&path).unwrap();
            assert!(xml.contains("resources/"));
            assert!(!xml.contains("binembedded"));
            let files: Vec<_> = std::fs::read_dir(directory.join("resources")).unwrap().collect();
            assert_eq!(files.len(), 1);
            assert_eq!(std::fs::read(files[0].as_ref().unwrap().path()).unwrap(), SVG);
        } else {
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1, "an archive needs no external SBS or resources");
        }
        let mut reopened = Session::new(Package::new("Other"));
        file_cmds::open_file(&mut reopened, path.to_str().unwrap()).unwrap();
        assert_eq!(reopened.pkg.resources.len(), 1);
        assert_eq!(reopened.pkg.resources[0].bytes.as_slice(), SVG);
        execute(&mut reopened, "file.save", &json!({})).unwrap();
        if extension == "sbs" {
            assert_eq!(std::fs::read_dir(directory.join("resources")).unwrap().count(), 1, "repeat saves reuse resource names");
        }
        execute(&mut reopened, "folder.create", &json!({"name": "Unsaved"})).unwrap();
        let copy = directory.join(format!("copy.{extension}"));
        let saved_path = reopened.path.clone();
        execute(&mut reopened, "file.saveCopyAs", &json!({"path": copy})).unwrap();
        assert_eq!(reopened.path, saved_path);
        assert!(reopened.modified);
        execute(&mut reopened, "file.reload", &json!({"discard": true})).unwrap();
        assert!(!reopened.pkg.folder_paths().contains("Unsaved"));
        assert_eq!(reopened.pkg.resources[0].bytes.as_slice(), SVG);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn vector_import_is_undoable_and_does_not_require_a_graph_or_a_saved_package() {
    let root = std::env::temp_dir().join(format!("materialcraft-vector-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("shape.SVG");
    std::fs::write(&path, SVG).unwrap();
    let mut session = Session::new(Package::new("Unsaved"));
    assert!(execute(&mut session, "file.importBitmap", &json!({"path": path})).is_err());
    execute(&mut session, "file.importVector", &json!({"path": path})).unwrap();
    assert!(session.pkg.graphs.is_empty());
    assert!(session.path.is_none());
    assert_eq!(session.pkg.resources[0].folder, "Resources");
    assert_eq!(session.pkg.resources[0].bytes.as_slice(), SVG);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1, "unsaved import creates no sidecars");
    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert!(session.pkg.resources.is_empty());
    for extension in ["psd", "psb", "surface", "raw"] {
        assert!(file_cmds::add_resource(&mut session, &format!("shape.{extension}"), SVG.to_vec(), None).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_save_keeps_the_document_and_existing_sbs_intact() {
    let root = std::env::temp_dir().join(format!("materialcraft-failed-save-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("safe.sbs");
    let mut session = Session::new(Package::new("Safe"));
    execute(&mut session, "file.save", &json!({"path": path})).unwrap();
    let original = std::fs::read(&path).unwrap();
    file_cmds::add_resource(&mut session, "shape.svg", SVG.to_vec(), None).unwrap();
    let files = file_cmds::save_files_at(&session, path.to_str().unwrap()).unwrap();
    let resource = files.files.keys().find(|name| name.starts_with("resources/")).unwrap();
    std::fs::write(root.join(resource), b"different existing file").unwrap();
    assert!(execute(&mut session, "file.save", &json!({})).is_err());
    assert!(session.modified);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(root.join(resource)).unwrap(), b"different existing file");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reload_refreshes_assets_in_archived_dependencies() {
    use materialcraft_engine::{
        core::{Graph, GraphInstance, Resource, ResourceBytes},
        formats::project::ProjectFiles,
    };
    let root = std::env::temp_dir().join(format!("materialcraft-reload-archive-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("linked.mcraft");
    let mut source = Package::new("Source");
    source.graphs.push(Graph::new("Material"));
    source.resources.push(Resource::new("Mask", "mask.svg", ResourceBytes::new(SVG.to_vec())));
    let mut package = Package::new("Consumer");
    let mut graph = Graph::new("Graph");
    let id = graph.add_node("graph_instance", [0.0; 2]);
    graph.node_mut(id).unwrap().instance = Some(GraphInstance {
        label: String::new(),
        package: "/source/material.sbs".into(),
        graph: "Material".into(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        defaults: Default::default(),
    });
    package.graphs.push(graph);
    package.dependencies.insert("/source/material.sbs".into(), std::sync::Arc::new(source));
    let mut files = ProjectFiles::build(&package, "linked.mcraft", true).unwrap();
    std::fs::write(&path, files.zip().unwrap()).unwrap();
    let mut session = Session::new(Package::new("Empty"));
    file_cmds::open_file(&mut session, path.to_str().unwrap()).unwrap();
    let reference = session.pkg.graphs[0].nodes[0].instance.as_ref().unwrap().package.clone();
    assert_eq!(session.pkg.dependencies[&reference].resources[0].bytes.as_slice(), SVG);

    // External edits can replace an asset without changing its path inside the archive.
    let changed = std::str::from_utf8(SVG).unwrap().replace("red", "blue").into_bytes();
    let resource = files.files.values_mut().find(|bytes| bytes.as_slice() == SVG).unwrap();
    *resource = ResourceBytes::new(changed.clone());
    std::fs::write(&path, files.zip().unwrap()).unwrap();
    execute(&mut session, "file.reload", &json!({})).unwrap();
    assert_eq!(session.pkg.dependencies[&reference].resources[0].bytes.as_slice(), changed);
    assert!(!session.modified);
    std::fs::remove_dir_all(root).unwrap();
}
