use egui::{Event, Pos2, Rect, epaint::Shape, vec2};
use egui_kittest::Harness;
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

pub fn harness(render: bool) -> Harness<'static, MaterialCraftApp> {
    let app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    let builder = Harness::builder().with_size(vec2(1600.0, 960.0)).with_step_dt(1.0 / 60.0);
    let builder = if render { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app,
    );
    h.run_steps(8);
    let pointer = h.state().graph.view.center();
    h.event(Event::PointerMoved(pointer));
    h.step();
    h.key_press(egui::Key::Tab);
    h.step();
    h.run_steps(3);
    assert!(h.ctx.memory(|m| m.area_rect(egui::Id::unique("node-search"))).is_some());
    assert!(label_position(&h, "Bitmap").is_some(), "fresh search must show its first result");
    h
}

pub fn label_position(h: &Harness<'static, MaterialCraftApp>, label: &str) -> Option<Pos2> {
    let popup = h.ctx.memory(|m| m.area_rect(egui::Id::unique("node-search"))).unwrap();
    fn find(shape: &Shape, clip: Rect, popup: Rect, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if clip.contains(t.pos) && popup.contains(t.pos) && t.galley.text().trim() == label => {
                Some(t.pos + vec2(0.0, t.galley.size().y * 0.5))
            }
            Shape::Vec(v) => v.iter().find_map(|s| find(s, clip, popup, label)),
            _ => None,
        }
    }
    h.output().shapes.iter().find_map(|s| find(&s.shape, s.clip_rect, popup, label))
}
