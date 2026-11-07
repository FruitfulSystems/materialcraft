use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Graph,
    Explorer,
    Library,
    View3d,
    View2d,
    Properties,
    Console,
    History,
}

/// Windows menu items and the panels they show or hide
pub const VIEWS: [(&str, Panel); 7] = [
    ("ui.view.explorer", Panel::Explorer),
    ("ui.view.properties", Panel::Properties),
    ("ui.view.console", Panel::Console),
    ("ui.view.3d", Panel::View3d),
    ("ui.view.2d", Panel::View2d),
    ("ui.view.library", Panel::Library),
    ("ui.view.history", Panel::History),
];

/// The panel a Windows menu item toggles
pub fn view_panel(id: &str) -> Option<Panel> {
    VIEWS.iter().find(|(v, _)| *v == id).map(|(_, p)| *p)
}

impl Panel {
    pub const ALL: [Panel; 8] =
        [Panel::Graph, Panel::Explorer, Panel::Library, Panel::View3d, Panel::View2d, Panel::Properties, Panel::Console, Panel::History];

    pub fn name(self) -> &'static str {
        match self {
            Panel::Graph => "Graph",
            Panel::Explorer => "Explorer",
            Panel::Library => "Library",
            Panel::View3d => "3D View",
            Panel::View2d => "2D View",
            Panel::Properties => "Properties",
            Panel::Console => "Console",
            Panel::History => "History",
        }
    }

    /// Initial floating rectangle in points: `[x, y, width, height]`
    pub(super) fn float_rect(self) -> Option<[f32; 4]> {
        let slot = match self {
            Panel::Console => 0.0,
            Panel::History => 1.0,
            _ => return None,
        };
        Some([900.0, 120.0 + 50.0 * slot, 420.0, 320.0])
    }
}
