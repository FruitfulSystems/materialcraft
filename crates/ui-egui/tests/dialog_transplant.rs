use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

fn harness(preferences: bool) -> Harness<'static, MaterialCraftApp> {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.ui.prefs.preview_size = 64;
    app.ui.windows.preferences = preferences;
    app.ui.windows.new_graph = !preferences;
    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app,
    );
    harness.run_steps(8);
    harness
}

fn choose_size(harness: &mut Harness<'static, MaterialCraftApp>, axis: &str, size: &str) {
    harness.get(By::new().role(egui::accesskit::Role::ComboBox).label(axis)).click();
    harness.run_steps(2);
    harness.get_by_label(size).click();
    harness.run_steps(2);
}

#[test]
fn template_selection_preserves_the_name_and_rectangular_creation_is_one_undo() {
    let mut harness = harness(false);
    let original = harness.state().session.pkg.clone();
    harness.state_mut().ui.new_graph_name = "My material".into();
    harness.get(By::new().role(egui::accesskit::Role::ListItem).label("Atomic sample")).click();
    harness.run_steps(3);
    assert_eq!(harness.state().ui.new_graph_name, "My material");
    choose_size(&mut harness, "Width", "512 px");
    harness.get(By::new().role(egui::accesskit::Role::Button).label("Link width and height")).click();
    harness.run_steps(2);
    choose_size(&mut harness, "Height", "128 px");
    harness.get_by_label("Create").click();
    harness.run_steps(3);
    assert!(!harness.state().ui.windows.new_graph);
    assert_eq!(harness.state().session.graph().unwrap().size_px(), [512, 128]);
    assert_eq!(harness.state().session.graph().unwrap().name, "My material");
    assert!(harness.state().session.graph().unwrap().nodes.iter().any(|node| node.def == "fx_map"));
    harness.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(harness.state().session.pkg, original);
}

#[test]
fn relinking_dimensions_creates_a_square_and_function_creation_keeps_main_behavior() {
    let mut harness = harness(false);
    choose_size(&mut harness, "Width", "256 px");
    harness.get_by_label("Link width and height").click();
    harness.run_steps(2);
    choose_size(&mut harness, "Height", "64 px");
    harness.get_by_label("Link width and height").click();
    harness.run_steps(2);
    harness.get_by_label("Create").click();
    harness.run_steps(3);
    assert_eq!(harness.state().session.graph().unwrap().size_px(), [256, 256]);

    harness.state_mut().ui.windows.new_graph = true;
    harness.run_steps(3);
    harness.get(By::new().role(egui::accesskit::Role::Button).label("Function Graph")).click();
    harness.run_steps(3);
    assert!(harness.query_all(By::new().role(egui::accesskit::Role::ComboBox).label("Width")).next().is_none());
    harness.get_by_label("Create").click();
    harness.run_steps(3);
    assert!(harness.state().session.graph().unwrap().function.is_some());
    assert!(harness.state().function_editor.is_some());
}

#[test]
fn preferences_tabs_edit_supported_settings_without_changing_the_document() {
    let mut harness = harness(true);
    let original = harness.state().session.pkg.clone();
    assert!(harness.query_all(By::new().role(egui::accesskit::Role::Slider).label("Interface scale")).next().is_some());
    harness.get(By::new().role(egui::accesskit::Role::Tab).label("Graph")).click();
    harness.run_steps(3);
    let thumbnails = harness.state().ui.prefs.thumbnails;
    harness.get(By::new().role(egui::accesskit::Role::CheckBox).label("Show node thumbnails")).click();
    harness.run_steps(2);
    assert_eq!(harness.state().ui.prefs.thumbnails, !thumbnails);
    harness.get(By::new().role(egui::accesskit::Role::Tab).label("3D View")).click();
    harness.run_steps(3);
    assert!(harness.query_all(By::new().role(egui::accesskit::Role::Slider).label("3D pan speed")).next().is_some());
    harness.get(By::new().role(egui::accesskit::Role::Tab).label("General")).click();
    harness.run_steps(3);
    harness.get(By::new().role(egui::accesskit::Role::ComboBox).label("Language")).click();
    harness.run_steps(2);
    assert!(harness.query_all(By::new().label("العربية")).next().is_some());
    assert_eq!(harness.state().session.pkg, original);
    let saved: UiState = serde_json::from_value(serde_json::to_value(&harness.state().ui).unwrap()).unwrap();
    assert_eq!(saved.prefs.thumbnails, !thumbnails);
}
