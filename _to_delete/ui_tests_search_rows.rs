//! Clicking anywhere across a search result's row should insert that result.
#[path = "common/search.rs"]
mod search;
use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use materialcraft_ui_egui::MaterialCraftApp;

fn right_side(h: &Harness<'static, MaterialCraftApp>) -> Pos2 {
    let y = search::label_position(h, "Bitmap").unwrap().y;
    let popup = h.ctx.memory(|m| m.area_rect(egui::Id::unique("node-search"))).unwrap();
    pos2(popup.right() - 24.0, y)
}
fn click(h: &mut Harness<'static, MaterialCraftApp>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
}
#[test]
fn clicking_label_or_right_side_inserts_the_same_result() {
    for right in [false, true] {
        let mut h = search::harness(false);
        let pos = if right { right_side(&h) } else { search::label_position(&h, "Bitmap").unwrap() + vec2(8.0, 0.0) };
        click(&mut h, pos);
        let graph = h.state().session.graph().unwrap();
        assert_eq!(graph.nodes.len(), 1, "row click did not insert a node (right_side={right})");
        assert_eq!(graph.nodes[0].def, "bitmap");
    }
}
#[test]
#[ignore = "requires an offscreen GPU"]
fn full_width_result_screenshot() {
    let mut h = search::harness(true);
    let pos = right_side(&h);
    h.event(Event::PointerMoved(pos));
    h.run_steps(3);
    let dir = std::path::PathBuf::from(std::env::var_os("MC_SNAPSHOT_DIR").expect("set MC_SNAPSHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    h.render().unwrap().save(dir.join("full-width-search-result.png")).unwrap();
}
