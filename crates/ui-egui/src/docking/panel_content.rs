use craft_ui::docking::{DockContent, PanelLimits, Permissions};
use egui::{Margin, RichText, Ui, vec2};

use super::{Panel, permissions};
use crate::{MaterialCraftApp, i18n::tr};

/// Panel bodies, titles and sizing constraints
pub(super) struct PanelContent<'a> {
    pub(super) app: &'a mut MaterialCraftApp,
}

impl DockContent<Panel> for PanelContent<'_> {
    fn body(&mut self, ui: &mut Ui, panel: &Panel) {
        let app = &mut *self.app;
        if app.ui.dock.maximized().is_none() && ui.rect_contains_pointer(ui.max_rect()) && super::maximization::consume_shortcut(ui.ctx()) {
            if let Err(error) = app.ui.dock.maximize(*panel) {
                log::warn!("panel was not maximized: {error}");
            }
            ui.ctx().request_repaint();
        }

        match panel {
            Panel::Graph => crate::graph::show(app, ui),
            Panel::View3d => crate::view3d::show(app, ui),
            Panel::View2d => crate::view2d::show(app, ui),
            _ => {
                egui::Frame::new().inner_margin(Margin::symmetric(6, 4)).show(ui, |ui| match panel {
                    Panel::Explorer => crate::explorer::show(app, ui),
                    Panel::Library => crate::library::show(app, ui),
                    Panel::Properties => crate::properties::show(app, ui),
                    Panel::Console => crate::windows::console(app, ui),
                    Panel::History => crate::windows::history(app, ui),
                    _ => {}
                });
            }
        }
    }

    fn logical_label(&self, panel: &Panel) -> String {
        match panel {
            Panel::Properties => crate::properties::title(self.app),
            Panel::Graph => self.app.session.graph().map(|g| format!("{} - {}", g.name, tr("Graph"))).unwrap_or_else(|_| tr("Graph").into()),
            Panel::View3d => format!("{} - {}", crate::view3d::scene_label(self.app), tr("3D View")),
            Panel::View2d => crate::view2d::title(self.app),
            p => tr(p.name()).to_string(),
        }
    }

    fn label(&mut self, _ui: &Ui, _panel: &Panel, logical: &str, font: &egui::FontId, color: egui::Color32) -> egui::WidgetText {
        RichText::new(logical.to_uppercase()).font(font.clone()).strong().color(color).into()
    }

    fn permissions(&self, panel: &Panel) -> Permissions {
        permissions(panel)
    }

    fn limits(&self, panel: &Panel) -> PanelLimits {
        let min = if *panel == Panel::Graph { vec2(240.0, 160.0) } else { vec2(120.0, 60.0) };
        PanelLimits { min, ..PanelLimits::default() }
    }

    fn floating_size(&self, _panel: &Panel) -> Option<egui::Vec2> {
        Some(vec2(420.0, 320.0))
    }
}
