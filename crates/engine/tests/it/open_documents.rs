use materialcraft_engine::{Session, core::Package, execute, file_cmds, formats, inspect};
use serde_json::json;

#[test]
fn opening_another_package_preserves_edits_selection_and_undo() {
    let mut session = Session::new(Package::new("First"));
    execute(&mut session, "file.new", &json!({"template": "empty", "name": "Source"})).unwrap();
    let added = execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let history = session.history();
    let selection = session.selection.clone();
    let original = file_cmds::save_bytes(&session).unwrap();
    let bytes = formats::sbs::write_sbs(&Package::new("Second")).unwrap();

    file_cmds::open_bytes(&mut session, &bytes, Some("second.sbs".into())).unwrap();

    assert_eq!(inspect::session(&session, None)["documents"].as_array().unwrap().len(), 2);
    execute(&mut session, "package.activate", &json!({"document": 0})).unwrap();
    // Other open packages extend the runtime dependency registry without editing this document
    assert_eq!(file_cmds::save_bytes(&session).unwrap(), original);
    assert_eq!(session.selection, selection);
    assert_eq!(session.history(), history);
    assert!(session.modified);

    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert!(session.graph().unwrap().nodes.iter().all(|node| node.id.0 != added["id"].as_u64().unwrap()));
}

#[test]
fn saving_a_new_file_preserves_its_identity_across_path_aliases() {
    let directory = std::env::temp_dir().join(format!("materialcraft-save-identity-{}", std::process::id()));
    let files = directory.join("files");
    std::fs::create_dir_all(&files).unwrap();
    // Unix aliases exercise the same mismatch as Windows short paths and canonical prefixes
    #[cfg(unix)]
    let requested = {
        let alias = directory.join("alias");
        std::os::unix::fs::symlink(&files, &alias).unwrap();
        alias.join("source.sbs")
    };
    #[cfg(not(unix))]
    let requested = files.join("source.sbs");

    let mut session = Session::new(Package::new("Source"));
    execute(&mut session, "file.new", &json!({"template": "empty", "name": "Source"})).unwrap();
    let saved = execute(&mut session, "file.save", &json!({"path": requested})).unwrap();
    let canonical = requested.canonicalize().unwrap();
    assert_eq!(saved["path"], json!(canonical));
    assert_eq!(session.path.as_deref(), canonical.to_str());

    let document = session.document_id;
    execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let edited = session.pkg.clone();
    let history = session.history();
    for path in [&requested, &canonical] {
        file_cmds::open_file(&mut session, path.to_str().unwrap()).unwrap();
        assert_eq!(session.document_id, document);
        assert_eq!(session.open_packages().len(), 1);
        assert_eq!(session.pkg, edited);
        assert_eq!(session.history(), history);
        assert!(session.modified);
        assert_eq!(session.path.as_deref(), canonical.to_str());
    }
    assert_eq!(execute(&mut session, "file.save", &json!({})).unwrap()["path"], saved["path"]);

    execute(&mut session, "file.newPackage", &json!({"name": "Other"})).unwrap();
    let bytes = std::fs::read(&canonical).unwrap();
    for path in [&requested, &canonical] {
        let error = execute(&mut session, "file.saveAs", &json!({"path": path})).unwrap_err();
        assert!(error.to_string().contains("another open package already uses this file"));
        assert_eq!(std::fs::read(&canonical).unwrap(), bytes);
    }
    std::fs::remove_dir_all(directory).unwrap();
}
