use materialcraft_engine::{execute, new_session};
use serde_json::json;

#[test]
fn rectangular_templates_are_created_and_undone_as_one_edit() {
    for template in ["empty", "pbr", "atomics"] {
        let mut session = new_session();
        let original = session.pkg.clone();
        let original_graph = session.graph;
        execute(&mut session, "file.new", &json!({"template": template, "name": "Wide", "width": 512, "height": 128})).unwrap();
        assert_eq!(session.graph().unwrap().size_px(), [512, 128]);
        assert_eq!(session.graph().unwrap().name, "Wide");
        assert_eq!(session.pkg.graphs.len(), original.graphs.len() + 1);
        execute(&mut session, "edit.undo", &json!({})).unwrap();
        assert_eq!(session.pkg, original);
        assert_eq!(session.graph, original_graph);
        assert!(!session.modified);
        execute(&mut session, "edit.redo", &json!({})).unwrap();
        assert_eq!(session.graph().unwrap().size_px(), [512, 128]);
    }
}

#[test]
fn dimension_overrides_keep_square_defaults_and_bound_each_axis() {
    for (parameters, expected) in [
        (json!({"size": 256}), [256, 256]),
        (json!({"size": 256, "height": 64}), [256, 64]),
        (json!({"size": 256, "width": 64}), [64, 256]),
        (json!({"width": 1000, "height": 127}), [512, 64]),
        (json!({"width": 0, "height": u64::MAX}), [16, 8192]),
    ] {
        let mut session = new_session();
        execute(&mut session, "file.new", &parameters).unwrap();
        assert_eq!(session.graph().unwrap().size_px(), expected, "{parameters}");
    }
}

#[test]
fn invalid_dimensions_do_not_create_a_partial_graph_or_undo_entry() {
    for parameters in [json!({"width": -1}), json!({"height": "128"}), json!({"width": 32.5}), json!({"height": null})] {
        let mut session = new_session();
        let original = session.pkg.clone();
        assert!(execute(&mut session, "file.new", &parameters).is_err(), "{parameters}");
        assert_eq!(session.pkg, original);
        assert!(!session.modified);
        assert!(execute(&mut session, "edit.undo", &json!({})).is_err());
    }
}
