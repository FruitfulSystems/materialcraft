use serde::{Deserialize, Serialize};

use super::{GraphCamera, ImageCamera, Prefs, Windows};
use crate::docking::Dock;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub dock: Dock,
    pub prefs: Prefs,
    pub windows: Windows,
    pub graph_camera: GraphCamera,
    pub image_camera: ImageCamera,
    /// Library: the selected category path ("" = all) and the search text
    pub library_category: String,
    pub library_search: String,
    /// Library entries starred by the user (node ids, `@graph`, `fn:op`)
    pub library_favorites: Vec<String>,
    /// Library tree groups that are folded shut
    pub library_collapsed: Vec<String>,
    /// Recently opened or saved packages, newest first
    pub recent: Vec<String>,
    /// Plugins (file or folder paths in the plugins folder) whose graphs the library offers
    pub plugins_loaded: Vec<String>,
    /// 3D view ▸ Scene ▸ Save Current State As Default: the state new sessions and Reset Scene use
    pub view3d_default: Option<serde_json::Value>,
    /// The folder the last file dialog ended in (Preferences ▸ Save/Export path: Last path)
    pub last_dir: String,
    /// Preferences ▸ Shortcuts: node creation shortcuts, def id → shortcut ("Ctrl+Shift+B")
    pub node_shortcuts: Vec<(String, String)>,
    /// New graph dialog fields
    pub new_graph_kind: materialcraft_engine::core::GraphKind,
    pub new_graph_template: String,
    pub new_graph_name: String,
    pub new_graph_size: u32,
    /// New graph height when the two sizes are unlinked (0 = same as the width)
    pub new_graph_height: u32,
    /// Keep width and height the same in the new graph dialog
    pub new_graph_link: bool,
    /// Template category shown in the new graph dialog (empty = all)
    pub new_graph_category: String,
    /// Graph toolbar: the parent size's width and height change together
    pub parent_link: bool,
    /// Graph toolbar: the align buttons also stack the nodes (no gaps)
    pub stack_align: bool,
    /// Export dialog fields
    pub export_dir: String,
    pub export_format: String,
    pub export_depth: u32,
    /// Export size in pixels; 0 = the graph's own size
    pub export_size: u32,
    pub export_pattern: String,
    /// Frame the whole graph on the next frame
    #[serde(skip)]
    pub frame_all: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            dock: Dock::default(),
            prefs: Prefs::default(),
            windows: Windows::default(),
            graph_camera: GraphCamera::default(),
            image_camera: ImageCamera::default(),
            library_category: String::new(),
            library_search: String::new(),
            library_favorites: Vec::new(),
            library_collapsed: Vec::new(),
            recent: Vec::new(),
            plugins_loaded: Vec::new(),
            view3d_default: None,
            last_dir: String::new(),
            node_shortcuts: Vec::new(),
            new_graph_kind: materialcraft_engine::core::GraphKind::Material,
            new_graph_template: "pbr".into(),
            new_graph_name: "Material_Graph".into(),
            new_graph_size: 1024,
            new_graph_height: 0,
            new_graph_link: true,
            new_graph_category: String::new(),
            parent_link: true,
            stack_align: true,
            export_dir: String::new(),
            export_format: "png".into(),
            export_depth: 0,
            export_size: 0,
            export_pattern: "$(graph)_$(identifier)".into(),
            frame_all: true,
        }
    }
}

impl UiState {
    /// Remember `path` at the top of the recent list
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(self.prefs.recent_count.clamp(1, 50) as usize);
    }
}
