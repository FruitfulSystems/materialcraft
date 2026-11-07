//! Panel hosting, workspace validation and transient maximization

mod dock;
mod maximization;
mod panel;
mod panel_content;
mod workspace;

pub use dock::Dock;
pub use panel::{Panel, VIEWS, view_panel};
pub use workspace::{Workspace, default_workspace, permissions, validate};

use craft_ui::docking::{DockArea, DockStyle};
use egui::{Id, Ui};

use crate::{MaterialCraftApp, theme::Tokens};
use panel_content::PanelContent;

fn area_id() -> Id {
    Id::unique("materialcraft-docking")
}

/// Shared appearance for docked and floating panels
fn style(ui: &Ui, tokens: &Tokens) -> DockStyle {
    let mut style = DockStyle::from_ui(ui);
    style.gap = 6.0;
    // Smallest panel extent a divider drag leaves
    style.min_pane = 60.0;
    style.background = tokens.panel;
    style.tab_background = tokens.panel;
    style.active_background = tokens.button;
    style.text = tokens.text_strong;
    style.inactive_text = tokens.text_dim;
    style.border = egui::Stroke::NONE;
    style.accent = tokens.accent;
    style.floating_background = tokens.panel;
    style.floating_stroke = egui::Stroke::new(1.0, tokens.button_border);
    style.font = egui::FontId::proportional(10.5);
    style.min_tab_width = 54.0;
    style.tab_height = 30.0;
    // Floating title bars move windows without creating new docks
    style.new_dock_zones = false;
    style.title_bar_docks = false;
    style
}

/// Draw the saved workspace or its maximized panel
pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let context = ui.ctx().clone();
    if app.ui.dock.maximized().is_some() && maximization::consume_shortcut(&context) {
        app.ui.dock.restore();
    }
    if let Some(panel) = app.ui.dock.maximized() {
        maximization::show(app, ui, panel);
        return;
    }

    let tokens = Tokens::get(&context);
    let style = style(ui, &tokens);
    let layout = app.ui.dock.workspace.layout.clone();
    let mut area = DockArea::new(area_id()).input_blocked(context.memory(|m| m.top_modal_layer()).is_some());
    if let Some(panel) = app.ui.dock.raise.take() {
        area = area.raise_panel(panel);
    }
    let output = area.show_content(ui, &layout, &style, &mut PanelContent { app });
    if let Some(error) = &output.error {
        log::warn!("docking: {error}");
    }
    // Panel actions take priority over gestures against the previous layout
    if app.ui.dock.workspace.layout != layout || app.ui.dock.maximized().is_some() {
        craft_ui::docking::cancel_drag::<Panel>(&context, area_id());
        return;
    }
    for action in output.actions {
        app.ui.dock.apply(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_has_every_panel_and_is_valid() {
        let w = default_workspace();
        assert!(validate(&w).is_ok());
        for panel in Panel::ALL {
            assert!(w.layout.contains(&panel) || w.hidden.iter().any(|(p, _)| *p == panel), "{panel:?} is neither shown nor hidden");
        }
    }

    #[test]
    fn default_dock_round_trips() {
        let dock = Dock::default();
        let json = serde_json::to_string(&dock).unwrap();
        let back: Dock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn the_graph_stays_put_and_panels_come_back() {
        let mut dock = Dock::default();
        dock.close(Panel::Graph);
        assert!(dock.is_open(Panel::Graph));
        let before = dock.clone();
        for p in [Panel::Properties, Panel::Library, Panel::View3d] {
            dock.close(p);
            assert!(!dock.is_open(p));
            dock.focus(p);
            assert!(dock.is_visible(p), "{p:?}");
            assert_eq!(dock, before, "{p:?} came back elsewhere");
        }
        dock.focus(Panel::Console);
        assert!(dock.is_visible(Panel::Console));
        assert_eq!(serde_json::from_value::<Dock>(serde_json::json!([1, 2])).unwrap(), Dock::default());
    }
}
