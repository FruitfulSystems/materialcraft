//! UI state: serde data, so the control channel can read and drive it and eframe persists it.

use serde::{Deserialize, Serialize};

use materialcraft_engine::CameraPrefs;

use crate::docking::Dock;
use crate::theme::{Accent, Brightness};

/// Preferences (Edit ▸ Preferences).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// UI language code (`i18n::LANGUAGES`).
    pub language: String,
    pub brightness: Brightness,
    pub accent: Accent,
    pub ui_scale: f32,
    /// Largest node output while editing (0 = full size).
    pub preview_size: u32,
    /// Minutes between autosaves of a modified package (0 = off).
    pub autosave_minutes: u32,
    /// Show node thumbnails in the graph.
    pub thumbnails: bool,
    /// Library tiles: large icons or a list.
    pub library_list: bool,
    /// Library tile size (icons view), 56..160 px.
    pub library_tile: f32,
    /// Open the home screen when the app starts.
    pub show_home: bool,
    /// 3D view camera: pan limit and speed.
    pub camera: CameraPrefs,
    /// Show the main toolbar under the menu bar (Windows ▸ Main toolbar).
    pub show_main_toolbar: bool,
    /// Worker threads the cook may use (Tools ▸ Switch engine; 0 = every core).
    pub cook_threads: usize,
    /// Graph toolbar: show each node's cook time.
    pub node_timings: bool,
    /// Graph toolbar: straight wires instead of curves.
    pub straight_wires: bool,
    /// Graph toolbar: dropped nodes land on the grid.
    pub snap_grid: bool,
    /// Graph toolbar: draw the background grid.
    pub show_grid: bool,
    /// General: how many recent packages the File menu keeps.
    pub recent_count: u32,
    /// General: undo steps kept.
    pub history_size: u32,
    /// General: wheel zooms the other way in every view.
    pub invert_zoom: bool,
    /// General: where file dialogs open: `last`, `package` or `custom` (`save_path`).
    pub save_path_mode: String,
    pub save_path: String,
    /// General: the auto-backup folder (empty = Documents/MaterialCraft/autosave).
    pub temp_dir: String,
    /// General: the cooked image cache's size in MB (0 = automatic).
    pub cache_mb: u32,
    /// General: auto-backup versions kept per package.
    pub backup_versions: u32,
    /// General: cap on every cook's larger side in px (0 = none).
    pub cook_limit: u32,
    /// Projects: lights on in a new session when no default 3D state is saved (ambient, point 1, point 2).
    pub default_lights: [bool; 3],
    /// Graph: Tab opens the node search.
    pub tab_key_menu: bool,
    /// Graph: dragging a wire into empty space opens the node search.
    pub drag_create: bool,
    /// Graph: show the 3D view when a graph opens.
    pub view_3d_on_open: bool,
    /// Graph: show the graph's output in the 2D view when it opens.
    pub view_2d_on_open: bool,
    /// Graph: show a newly added node in the 2D view.
    pub auto_display_new_node: bool,
    /// Graph: insert a conversion node when a color wire meets a grayscale input (or the reverse).
    pub auto_convert: bool,
    /// 3D view: the environment is hidden behind the mesh by default.
    pub env_hidden: bool,
    /// 3D view: default viewport render scale (0 = auto).
    pub viewport_scale: f32,
    /// 3D view: default samples per pixel of the resting frame (n × n).
    pub sample_count: u32,
    /// Bakers: worker threads (0 = every core).
    pub bake_threads: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            language: crate::i18n::os_language().into(),
            brightness: Brightness::default(),
            accent: Accent::default(),
            ui_scale: 1.0,
            preview_size: 512,
            autosave_minutes: 5,
            thumbnails: true,
            library_list: false,
            library_tile: 78.0,
            show_home: true,
            camera: CameraPrefs::default(),
            show_main_toolbar: true,
            cook_threads: 0,
            node_timings: false,
            straight_wires: false,
            snap_grid: false,
            show_grid: true,
            recent_count: 12,
            history_size: 200,
            invert_zoom: false,
            save_path_mode: "last".into(),
            save_path: String::new(),
            temp_dir: String::new(),
            cache_mb: 0,
            backup_versions: 6,
            cook_limit: 8192,
            default_lights: [true, false, false],
            tab_key_menu: true,
            drag_create: true,
            view_3d_on_open: true,
            view_2d_on_open: true,
            auto_display_new_node: true,
            auto_convert: false,
            env_hidden: true,
            viewport_scale: 0.0,
            sample_count: 2,
            bake_threads: 0,
        }
    }
}

/// Which dialogs are open.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Windows {
    pub new_graph: bool,
    pub export: bool,
    pub about: bool,
    pub preferences: bool,
    pub shortcuts: bool,
    pub home: bool,
    pub plugin_manager: bool,
    pub switch_engine: bool,
    /// The Dependency Manager window (Windows menu).
    pub dependencies: bool,
    /// The action waiting on the "unsaved changes" dialog (readable over the control channel;
    /// never restored from a saved layout).
    #[serde(skip_deserializing)]
    pub unsaved: Option<crate::unsaved::Guarded>,
}

/// The graph view's camera: the graph point at the view's top-left corner, and the zoom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphCamera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

impl GraphCamera {
    pub const ZOOM: (f32, f32) = (0.08, 4.0);

    /// The camera with a finite pan and a zoom inside [`Self::ZOOM`] (state set from outside can
    /// hold a zoom of 0 or NaN, which the view divides by).
    pub fn sanitized(self) -> GraphCamera {
        let d = GraphCamera::default();
        let pan = |v: f32, d: f32| if v.is_finite() { v.clamp(-1.0e9, 1.0e9) } else { d };
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(Self::ZOOM.0, Self::ZOOM.1) } else { d.zoom };
        GraphCamera { pan: [pan(self.pan[0], d.pan[0]), pan(self.pan[1], d.pan[1])], zoom }
    }
}

impl Default for GraphCamera {
    fn default() -> Self {
        GraphCamera { pan: [-60.0, -60.0], zoom: 0.8 }
    }
}

/// The 2D view's camera: image zoom (1 = one image pixel per point) and center offset.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageCamera {
    pub zoom: f32,
    pub offset: [f32; 2],
    /// Fit the image to the view (until the user zooms).
    pub fit: bool,
}

impl ImageCamera {
    pub const ZOOM: (f32, f32) = (0.02, 64.0);

    /// The camera with a finite offset and a zoom inside [`Self::ZOOM`].
    pub fn sanitized(self) -> ImageCamera {
        let off = |v: f32| if v.is_finite() { v.clamp(-1.0e9, 1.0e9) } else { 0.0 };
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(Self::ZOOM.0, Self::ZOOM.1) } else { 1.0 };
        ImageCamera { zoom, offset: [off(self.offset[0]), off(self.offset[1])], fit: self.fit }
    }
}

impl Default for ImageCamera {
    fn default() -> Self {
        ImageCamera { zoom: 1.0, offset: [0.0, 0.0], fit: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub dock: Dock,
    pub prefs: Prefs,
    pub windows: Windows,
    pub graph_camera: GraphCamera,
    pub image_camera: ImageCamera,
    /// Library: the selected category path ("" = all) and the search text.
    pub library_category: String,
    pub library_search: String,
    /// Library entries starred by the user (node ids, `@graph`, `fn:op`).
    pub library_favorites: Vec<String>,
    /// Library tree groups that are folded shut.
    pub library_collapsed: Vec<String>,
    /// Recently opened or saved packages, newest first.
    pub recent: Vec<String>,
    /// Plugins (file or folder paths in the plugins folder) whose graphs the library offers.
    pub plugins_loaded: Vec<String>,
    /// 3D view ▸ Scene ▸ Save Current State As Default: the state new sessions and Reset Scene use.
    pub view3d_default: Option<serde_json::Value>,
    /// The folder the last file dialog ended in (Preferences ▸ Save/Export path: Last path).
    pub last_dir: String,
    /// Preferences ▸ Shortcuts: node creation shortcuts, def id → shortcut ("Ctrl+Shift+B").
    pub node_shortcuts: Vec<(String, String)>,
    /// New graph dialog fields.
    pub new_graph_template: String,
    pub new_graph_name: String,
    pub new_graph_size: u32,
    /// New graph height when the two sizes are unlinked (0 = same as the width).
    pub new_graph_height: u32,
    /// Keep width and height the same in the new graph dialog.
    pub new_graph_link: bool,
    /// Template category shown in the new graph dialog (empty = all).
    pub new_graph_category: String,
    /// Graph toolbar: the parent size's width and height change together.
    pub parent_link: bool,
    /// Graph toolbar: the align buttons also stack the nodes (no gaps).
    pub stack_align: bool,
    /// Export dialog fields.
    pub export_dir: String,
    pub export_format: String,
    pub export_depth: u32,
    /// Export size in pixels; 0 = the graph's own size.
    pub export_size: u32,
    pub export_pattern: String,
    /// Frame the whole graph on the next frame.
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
    /// Remember `path` at the top of the recent list.
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(self.prefs.recent_count.clamp(1, 50) as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cameras_bound_huge_finite_offsets_before_screen_math() {
        let g = GraphCamera { pan: [f32::MAX, f32::MIN], zoom: 4.0 }.sanitized();
        assert!(g.pan.iter().all(|v| (v * g.zoom).is_finite() && v.abs() <= 1.0e9));
        let i = ImageCamera { offset: [f32::MAX, f32::MIN], zoom: 64.0, fit: false }.sanitized();
        assert!(i.offset.iter().all(|v| (v * i.zoom).is_finite() && v.abs() <= 1.0e9));
    }

    #[test]
    fn cameras_sanitize_zero_and_non_finite_zoom() {
        let g = GraphCamera { pan: [f32::NAN, 5.0], zoom: 0.0 }.sanitized();
        assert_eq!(g.zoom, GraphCamera::ZOOM.0);
        assert!(g.pan[0].is_finite() && g.pan[1] == 5.0);
        assert_eq!(GraphCamera { pan: [0.0; 2], zoom: f32::NAN }.sanitized().zoom, GraphCamera::default().zoom);
        assert_eq!(ImageCamera { zoom: -3.0, offset: [f32::INFINITY, 1.0], fit: false }.sanitized().zoom, ImageCamera::ZOOM.0);
        assert_eq!(ImageCamera { zoom: 1e9, offset: [0.0; 2], fit: false }.sanitized().zoom, ImageCamera::ZOOM.1);
    }

    #[test]
    fn camera_prefs_persist_and_default_when_missing() {
        // Preferences saved before the camera settings existed still load, with the defaults.
        let old: Prefs = serde_json::from_value(serde_json::json!({"uiScale": 1.25})).unwrap();
        assert_eq!(old.camera, CameraPrefs::default());
        assert_eq!(old.ui_scale, 1.25);
        let p = Prefs { camera: CameraPrefs { pan_limit: 8.0, pan_speed: 0.5 }, ..Prefs::default() };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["camera"], serde_json::json!({"panLimit": 8.0, "panSpeed": 0.5}));
        assert_eq!(serde_json::from_value::<Prefs>(v).unwrap(), p);
    }
}
