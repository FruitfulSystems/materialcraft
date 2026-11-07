use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use materialcraft_ui_egui::{MaterialCraftApp, UiState, control, docking::Panel, menus};
use serde_json::{Value, json};

fn app() -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.ui.prefs.preview_size = 64;
    app
}

fn maximized(app: &mut MaterialCraftApp, context: &egui::Context) -> Value {
    control::handle(app, context, "ui.inspect", &json!({}))["result"]["maximizedPanel"].clone()
}

fn maximize(app: &mut MaterialCraftApp, context: &egui::Context, panel: Panel) {
    let response = control::handle(app, context, "ui.panel.maximize", &json!({"panel": panel}));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(maximized(app, context), json!(panel));
}

#[test]
fn maximize_restores_each_panel_without_changing_the_workspace_or_document() {
    let mut app = app();
    let context = egui::Context::default();
    app.ui.dock.focus(Panel::Console);
    app.ui.dock.focus(Panel::History);
    let workspace = serde_json::to_value(&app.ui.dock).unwrap();
    let package = app.session.pkg.clone();
    let modified = app.session.modified;

    for panel in Panel::ALL {
        maximize(&mut app, &context, panel);
        assert!(menus::enabled(&app, "ui.unmaximize"));
        assert!(app.ui.dock.is_visible(panel));
        assert_eq!(serde_json::to_value(&app.ui.dock).unwrap(), workspace);
        let saved: UiState = serde_json::from_value(serde_json::to_value(&app.ui).unwrap()).unwrap();
        let mut reopened = MaterialCraftApp::headless(materialcraft_engine::new_session(), saved);
        assert_eq!(maximized(&mut reopened, &context), Value::Null);
        let response = control::handle(&mut app, &context, "ui.menu.invoke", &json!({"id": "ui.unmaximize"}));
        assert_eq!(response["ok"], true);
        assert_eq!(maximized(&mut app, &context), Value::Null);
        assert!(!menus::enabled(&app, "ui.unmaximize"));
        assert_eq!(serde_json::to_value(&app.ui.dock).unwrap(), workspace);
        assert_eq!(app.session.pkg, package);
        assert_eq!(app.session.modified, modified);
    }
}

#[test]
fn invalid_or_hidden_panels_leave_the_maximized_panel_and_workspace_alone() {
    let mut app = app();
    let context = egui::Context::default();
    maximize(&mut app, &context, Panel::View2d);
    let workspace = app.ui.dock.workspace().clone();

    for parameters in [json!({}), json!({"panel": "missing"}), json!({"panel": null}), json!({"panel": "Console"})] {
        let response = control::handle(&mut app, &context, "ui.panel.maximize", &parameters);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(maximized(&mut app, &context), json!("View2d"));
        assert_eq!(app.ui.dock.workspace(), &workspace);
    }
}

#[test]
fn opening_another_panel_or_closing_the_maximized_panel_restores_the_workspace() {
    let mut app = app();
    let context = egui::Context::default();
    maximize(&mut app, &context, Panel::View2d);
    menus::invoke(&mut app, &context, "ui.view.properties");
    assert_eq!(maximized(&mut app, &context), Value::Null);
    assert!(app.ui.dock.is_visible(Panel::Properties));

    maximize(&mut app, &context, Panel::View2d);
    app.ui.dock.close(Panel::View2d);
    assert_eq!(maximized(&mut app, &context), Value::Null);
    assert!(!app.ui.dock.is_open(Panel::View2d));

    maximize(&mut app, &context, Panel::Graph);
    app.ui.dock.close(Panel::Graph);
    assert!(app.ui.dock.is_open(Panel::Graph));
    assert_eq!(maximized(&mut app, &context), json!("Graph"));
    menus::invoke(&mut app, &context, "ui.resetLayout");
    assert_eq!(maximized(&mut app, &context), Value::Null);
    assert_eq!(app.ui.dock, materialcraft_ui_egui::docking::Dock::default());
}

fn harness() -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app(),
    );
    harness.run_steps(8);
    harness
}

fn press_shift_space(harness: &mut Harness<'static, MaterialCraftApp>) {
    harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
    harness.event(Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::SHIFT });
    harness.step();
    harness.event(Event::Key { key: Key::Space, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::SHIFT });
    harness.step();
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run_steps(3);
}

#[test]
fn shortcut_uses_the_current_pointer_and_restore_button_keeps_panel_geometry() {
    let mut harness = harness();
    let context = harness.ctx.clone();
    let graph = harness.state().graph.view;
    let workspace = harness.state().ui.dock.workspace().clone();
    harness.event(Event::PointerMoved(graph.center()));
    press_shift_space(&mut harness);
    assert_eq!(maximized(harness.state_mut(), &context), json!("Graph"));
    assert!(harness.state().graph.view.width() > graph.width());
    harness.get(By::new().role(egui::accesskit::Role::Button).label("Restore panel layout")).click();
    harness.run_steps(4);
    assert_eq!(maximized(harness.state_mut(), &context), Value::Null);
    assert_eq!(harness.state().graph.view, graph);
    assert_eq!(harness.state().ui.dock.workspace(), &workspace);

    harness.event(Event::PointerMoved(graph.center()));
    harness.step();
    harness.event(Event::PointerMoved(egui::pos2(1599.0, 1.0)));
    press_shift_space(&mut harness);
    assert_eq!(maximized(harness.state_mut(), &context), Value::Null);
}

#[test]
fn shortcut_leaves_text_input_and_popups_in_control() {
    let mut harness = harness();
    let context = harness.ctx.clone();
    harness.state_mut().ui.windows.new_graph = true;
    harness.run_steps(5);
    harness
        .get(By::new().role(egui::accesskit::Role::Window).label("New graph"))
        .get(By::new().role(egui::accesskit::Role::TextInput).label("Graph name"))
        .click();
    harness.run_steps(2);
    let graph = harness.state().graph.view;
    harness.event(Event::PointerMoved(graph.center()));
    press_shift_space(&mut harness);
    assert_eq!(maximized(harness.state_mut(), &context), Value::Null);

    harness.state_mut().ui.windows.new_graph = false;
    harness.run_steps(3);
    maximize(harness.state_mut(), &context, Panel::Graph);
    harness.state_mut().ui.windows.new_graph = true;
    harness.run_steps(5);
    harness
        .get(By::new().role(egui::accesskit::Role::Window).label("New graph"))
        .get(By::new().role(egui::accesskit::Role::TextInput).label("Graph name"))
        .click();
    harness.run_steps(2);
    press_shift_space(&mut harness);
    assert_eq!(maximized(harness.state_mut(), &context), json!("Graph"));

    harness.get_by_label("Cancel").click();
    harness.run_steps(3);
    harness.get_by_label("Windows").click();
    harness.run_steps(3);
    assert!(egui::Popup::is_any_open(&context));
    press_shift_space(&mut harness);
    assert_eq!(maximized(harness.state_mut(), &context), json!("Graph"));
}
