use serde::{Deserialize, Serialize};

use crate::theme::{Accent, Brightness};
use materialcraft_engine::CameraPrefs;

/// Preferences (Edit ▸ Preferences)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// UI language code (`i18n::LANGUAGES`)
    pub language: String,
    pub brightness: Brightness,
    pub accent: Accent,
    pub ui_scale: f32,
    /// Largest node output while editing (0 = full size)
    pub preview_size: u32,
    /// Minutes between autosaves of a modified package (0 = off)
    pub autosave_minutes: u32,
    /// Show node thumbnails in the graph
    pub thumbnails: bool,
    pub snap_to_grid: bool,
    /// Library tiles: large icons or a list
    pub library_list: bool,
    /// Open the home screen when the app starts
    pub show_home: bool,
    /// 3D view camera: pan limit and speed
    pub camera: CameraPrefs,
    /// Show the main toolbar under the menu bar (Windows ▸ Main toolbar)
    pub show_main_toolbar: bool,
    /// Worker threads the cook may use (Tools ▸ Switch engine; 0 = every core)
    pub cook_threads: usize,
    /// Library tile size (icons view), 56..160 px
    pub library_tile: f32,
    /// Graph toolbar: show each node's cook time
    pub node_timings: bool,
    /// Graph toolbar: straight wires instead of curves
    pub straight_wires: bool,
    /// Graph toolbar: draw the background grid
    pub show_grid: bool,
    /// General: how many recent packages the File menu keeps
    pub recent_count: u32,
    /// General: undo steps kept
    pub history_size: u32,
    /// General: wheel zooms the other way in every view
    pub invert_zoom: bool,
    /// General: where file dialogs open: `last`, `package` or `custom` (`save_path`)
    pub save_path_mode: String,
    pub save_path: String,
    /// General: the auto-backup folder (empty = Documents/MaterialCraft/autosave)
    pub temp_dir: String,
    /// General: the cooked image cache's size in MB (0 = automatic)
    pub cache_mb: u32,
    /// General: auto-backup versions kept per package
    pub backup_versions: u32,
    /// General: cap on every cook's larger side in px (0 = none)
    pub cook_limit: u32,
    /// Projects: lights on in a new session when no default 3D state is saved (ambient, point 1, point 2)
    pub default_lights: [bool; 3],
    /// Graph: Tab opens the node search
    pub tab_key_menu: bool,
    /// Graph: dragging a wire into empty space opens the node search
    pub drag_create: bool,
    /// Graph: show the 3D view when a graph opens
    pub view_3d_on_open: bool,
    /// Graph: show the graph's output in the 2D view when it opens
    pub view_2d_on_open: bool,
    /// Graph: show a newly added node in the 2D view
    pub auto_display_new_node: bool,
    /// Graph: insert a conversion node when a color wire meets a grayscale input (or the reverse)
    pub auto_convert: bool,
    /// 3D view: the environment is hidden behind the mesh by default
    pub env_hidden: bool,
    /// 3D view: default viewport render scale (0 = auto)
    pub viewport_scale: f32,
    /// 3D view: default samples per pixel of the resting frame (n × n)
    pub sample_count: u32,
    /// Bakers: worker threads (0 = every core)
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
            snap_to_grid: true,
            library_list: false,
            show_home: true,
            camera: CameraPrefs::default(),
            show_main_toolbar: true,
            cook_threads: 0,
            library_tile: 78.0,
            node_timings: false,
            straight_wires: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_prefs_persist_and_default_when_missing() {
        // Missing preferences use their defaults
        let old: Prefs = serde_json::from_value(serde_json::json!({"uiScale": 1.25})).unwrap();
        assert_eq!(old.camera, CameraPrefs::default());
        assert_eq!(old.ui_scale, 1.25);
        assert!(old.snap_to_grid);
        let p = Prefs { camera: CameraPrefs { pan_limit: 8.0, pan_speed: 0.5 }, ..Prefs::default() };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["camera"], serde_json::json!({"panLimit": 8.0, "panSpeed": 0.5}));
        assert_eq!(serde_json::from_value::<Prefs>(v).unwrap(), p);
    }
}
