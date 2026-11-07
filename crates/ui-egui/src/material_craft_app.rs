#[cfg(not(target_arch = "wasm32"))]
use crate::user_dir;
use crate::{ControlRequest, Toast, UiState, now_ms};
use crate::{control, cook, docking, fonts, function, graph, i18n, menus, pattern, state, theme, thumbs, toolbar, view2d, view3d, web_files, windows};
use materialcraft_engine::Session;
use serde_json::Value;
use std::collections::VecDeque;

pub struct MaterialCraftApp {
    pub session: Session,
    pub ui: UiState,
    pub control: Option<std::sync::mpsc::Receiver<ControlRequest>>,
    pub toasts: Vec<Toast>,
    /// Background cooking of the open graph
    pub cooker: cook::Cooker,
    /// Node output thumbnails and library previews
    pub thumbs: thumbs::Thumbs,
    /// The graph editor's gesture state
    pub graph: graph::GraphView,
    pub view3d: view3d::View3dState,
    pub view2d: view2d::View2dState,
    /// Embedded graph editors in the main graph panel
    pub pattern_editor: Option<pattern::PatternEditor>,
    pub function_editor: Option<function::FunctionEditor>,
    /// The plugins folder's packages (Tools ▸ Plugin Manager)
    pub plugins: crate::plugins::Plugins,
    /// A graph copied in the explorer
    pub explorer_clipboard: crate::explorer::GraphClipboard,
    /// The explorer's rename-in-place field
    pub explorer_rename: crate::explorer::Rename,
    /// Import graph: the package file picked and its graphs, waiting for a choice
    pub import_pick: Option<(String, Vec<String>)>,
    /// The 3D view property page shown in the Properties panel, and the selection when it opened
    /// (selecting a node closes it)
    pub props3d: Option<(view3d::Page3d, Vec<materialcraft_engine::core::NodeId>)>,
    /// The Preferences window's working copy
    pub prefs_editor: crate::preferences::PrefsEditor,
    /// The cache size last pushed into the session (MB; preferences)
    applied_cache_mb: u32,
    pub(crate) graph_parents: Vec<graph::graph_parent::GraphParent>,
    /// Synthetic input from the control channel, fed to egui on the next frames
    pub input_queue: VecDeque<control::Scripted>,
    /// Replies waiting for the next window capture (`ui.screenshot`)
    pub pending_screenshots: Vec<std::sync::mpsc::Sender<Value>>,
    pub fps: f32,
    /// Smoothed CPU time spent building the UI per frame (ms)
    pub ui_ms: f32,
    /// The last status message (bottom left)
    pub status: String,
    pub(crate) pending_load: Option<(String, Vec<u8>)>,
    pub(crate) quit_confirmed: bool,
    last_time: f64,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    next_autosave: f64,
    styled: Option<(theme::Brightness, theme::Accent)>,
    /// Fallback fonts for Arabic
    pub fonts: fonts::UiFonts,
    /// The icon font is installed (it takes effect on the frame after `set_fonts`)
    fonts_ready: bool,
    ui_gesture_active: bool,
}

impl MaterialCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>, session: Session) -> MaterialCraftApp {
        let ui = cc.storage.and_then(|s| eframe::get_value::<UiState>(s, "ui")).unwrap_or_default();
        theme::install(&cc.egui_ctx);
        let mut app = MaterialCraftApp::headless(session, ui);
        i18n::set_language(&app.ui.prefs.language);
        app.fonts_ready = true;
        app.ui.windows.home = app.ui.prefs.show_home;
        // The 3D view's saved default state, else the default lighting preference
        match app.ui.view3d_default.clone() {
            Some(state) => {
                app.run("view.3d.loadState", serde_json::json!({"state": state}));
            }
            None => {
                let [ambient, p1, p2] = app.ui.prefs.default_lights;
                app.run("view.3d", serde_json::json!({"lights": {"ambientOn": ambient, "point": [{"on": p1}, {"on": p2}]}}));
            }
        }
        app.view3d.show_env = !app.ui.prefs.env_hidden;
        app
    }

    /// An app without eframe (offscreen snapshots and tests): fonts are installed on the first frame, which draws nothing
    pub fn headless(session: Session, mut ui: UiState) -> MaterialCraftApp {
        ui.windows = state::Windows::default();
        ui.frame_all = true;
        let next_autosave = ui.prefs.autosave_minutes as f64 * 60.0;
        let mut app = MaterialCraftApp {
            session,
            ui,
            control: None,
            toasts: Vec::new(),
            cooker: cook::Cooker::default(),
            thumbs: thumbs::Thumbs::default(),
            graph: graph::GraphView::default(),
            view3d: view3d::View3dState::default(),
            view2d: view2d::View2dState::default(),
            function_editor: None,
            plugins: crate::plugins::Plugins::default(),
            explorer_clipboard: crate::explorer::GraphClipboard::default(),
            explorer_rename: crate::explorer::Rename::default(),
            import_pick: None,
            props3d: None,
            prefs_editor: crate::preferences::PrefsEditor::default(),
            applied_cache_mb: u32::MAX,
            graph_parents: Vec::new(),
            pattern_editor: None,
            input_queue: VecDeque::new(),
            pending_screenshots: Vec::new(),
            fps: 0.0,
            ui_ms: 0.0,
            status: String::new(),
            pending_load: None,
            quit_confirmed: false,
            last_time: 0.0,
            next_autosave,
            styled: None,
            fonts: fonts::UiFonts::default(),
            fonts_ready: false,
            ui_gesture_active: false,
        };
        i18n::set_language(&app.ui.prefs.language);
        app.apply_prefs();
        app
    }

    /// Push the preferences that live in the session (preview size, 3D camera)
    pub fn apply_prefs(&mut self) {
        self.cooker.threads = self.ui.prefs.cook_threads;
        let p = &self.ui.prefs;
        if self.session.max_undo != p.history_size as usize || self.session.cook_limit.unwrap_or(0) != p.cook_limit || p.cache_mb != self.applied_cache_mb {
            self.applied_cache_mb = p.cache_mb;
            let settings = serde_json::json!({"historySize": p.history_size.clamp(1, 10_000), "cacheMb": if p.cache_mb == 0 { 1024 } else { p.cache_mb }, "cookLimit": p.cook_limit});
            let _ = materialcraft_engine::execute(&mut self.session, "session.settings", &settings);
        }
        self.ui.recent.truncate(self.ui.prefs.recent_count.clamp(1, 50) as usize);
        if self.session.camera != self.ui.prefs.camera.sanitized() {
            self.session.set_camera_prefs(self.ui.prefs.camera);
        }
        let p = if self.ui.prefs.preview_size == 0 { None } else { Some(self.ui.prefs.preview_size.clamp(64, 8192)) };
        if self.session.preview_size != p {
            let _ = materialcraft_engine::execute(&mut self.session, "view.previewSize", &serde_json::json!({"size": p.unwrap_or(0)}));
        }
    }

    /// Run an engine command, reporting errors as toasts. Returns the result on success
    pub fn run(&mut self, id: &str, mut params: Value) -> Option<Value> {
        if matches!(
            id,
            "graph.node.add"
                | "graph.node.move"
                | "graph.frame.move"
                | "graph.wire.insertDot"
                | "edit.paste"
                | "edit.pasteWithoutLinks"
                | "edit.duplicate"
                | "edit.duplicateWithoutLinks"
        ) {
            if params.is_null() {
                params = serde_json::json!({});
            }

            if let Some(parameters) = params.as_object_mut() {
                parameters.entry("snapToGrid").or_insert(Value::Bool(self.ui.prefs.snap_to_grid));
            }
        }

        let graph_before = (self.session.graph, self.session.pkg.graphs.len());
        match cook::execute_guarded(&mut self.session, id, &params) {
            Ok(v) => {
                if id == "view.previewSize" {
                    self.ui.prefs.preview_size = self.session.preview_size.unwrap_or(0);
                }
                self.close_subgraph_after(id);
                if matches!(id, "view.3d.assign" | "view.3d.outputs") {
                    self.ui.dock.focus(docking::Panel::View3d);
                }
                if id == "graph.node.add" && self.ui.prefs.auto_display_new_node {
                    // Preferences ▸ Graph: a new node shows in the 2D view
                    if let Some(n) = v.get("id").and_then(Value::as_u64) {
                        let _ = cook::execute_guarded(&mut self.session, "view.2d", &serde_json::json!({"node": n, "port": 0}));
                    }
                }
                if (self.session.graph, self.session.pkg.graphs.len()) != graph_before
                    && matches!(id, "graph.open" | "file.new" | "file.open" | "file.newPackage" | "file.importGraph" | "graph.add" | "graph.duplicate")
                {
                    self.graph_opened();
                }
                Some(v)
            }
            Err(e) => {
                self.toast(e.to_string(), true);
                None
            }
        }
    }

    /// Preferences ▸ Graph: what happens when a graph opens (3D view, 2D view, frame all)
    fn graph_opened(&mut self) {
        self.ui.frame_all = true;
        if self.ui.prefs.view_3d_on_open && !self.ui.dock.is_open(docking::Panel::View3d) {
            self.ui.dock.toggle(docking::Panel::View3d);
        }
        let output = self.session.graph().ok().and_then(|g| g.nodes.iter().find(|n| n.def == "output").map(|n| n.id.0));
        if self.ui.prefs.view_2d_on_open
            && let Some(out) = output
        {
            let _ = cook::execute_guarded(&mut self.session, "view.2d", &serde_json::json!({"node": out, "port": 0}));
        }
    }

    pub(crate) fn close_subgraph_after(&mut self, command: &str) {
        if command.starts_with("file.new") || matches!(command, "file.open" | "graph.open" | "graph.delete" | "graph.duplicate") {
            self.graph.inspected_node = None;
            self.graph_parents.clear();
            self.pattern_editor = None;
            self.function_editor = None;
        }
        self.reconcile_editors();
        if (materialcraft_engine::selection_commands::supports(command) || matches!(command, "edit.undo" | "edit.redo"))
            && let Some(editor) = &mut self.function_editor
            && matches!(editor.target, function::FunctionTarget::Graph)
        {
            editor.selection = self.session.selection.iter().filter_map(|id| u32::try_from(id.0).ok()).collect();
        }
    }

    pub(crate) fn reconcile_editors(&mut self) {
        if self.function_editor.as_ref().is_some_and(|editor| editor.current(self).is_none()) {
            self.function_editor = None;
        }
        if self.pattern_editor.as_ref().is_some_and(|editor| editor.current(self).is_none()) {
            self.pattern_editor = None;
        }
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        self.status = text.clone();
        self.session.log(text.clone());
        self.toasts.push(Toast { text, error, until: self.last_time + if error { 6.0 } else { 3.0 } });
    }

    /// Every `prefs.autosave_minutes`, write a modified package to the autosave folder
    #[cfg(not(target_arch = "wasm32"))]
    fn autosave(&mut self, now: f64) {
        let period = self.ui.prefs.autosave_minutes as f64 * 60.0;
        if period <= 0.0 || !self.session.modified {
            self.next_autosave = now + period;
            return;
        }
        if now < self.next_autosave {
            return;
        }
        self.next_autosave = now + period;
        let dir = if self.ui.prefs.temp_dir.trim().is_empty() { user_dir("autosave") } else { std::path::PathBuf::from(self.ui.prefs.temp_dir.trim()) };
        let stem = materialcraft_engine::formats::safe_file_name(&self.session.pkg.name);
        let path = dir.join(format!("{stem}.sbs"));
        let versions = self.ui.prefs.backup_versions.clamp(1, 50) as usize;
        let pkg = self.session.pkg.clone();
        let spawned = std::thread::Builder::new().name("materialcraft-autosave".into()).spawn(move || {
            // Older backups shift down one slot: name.sbs, name.1.sbs, name.2.sbs, …
            let slot = |i: usize| if i == 0 { dir.join(format!("{stem}.sbs")) } else { dir.join(format!("{stem}.{i}.sbs")) };
            for i in (1..versions).rev() {
                if slot(i - 1).exists() {
                    let _ = std::fs::rename(slot(i - 1), slot(i));
                }
            }
            let _ = std::fs::remove_file(slot(versions));
            let result = std::fs::create_dir_all(&dir)
                .map_err(|e| e.to_string())
                .and_then(|_| materialcraft_engine::formats::sbs::write_sbs(&pkg).map_err(|e| e.to_string()))
                .and_then(|bytes| materialcraft_engine::formats::write_atomic(&path, &bytes).map_err(|e| e.to_string()));
            match result {
                Ok(()) => log::info!("autosaved {}", path.display()),
                Err(e) => log::warn!("autosave to {} failed: {e}", path.display()),
            }
        });
        if let Err(e) = spawned {
            log::warn!("could not start autosave: {e}");
        }
    }
}

impl eframe::App for MaterialCraftApp {
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(if cfg!(target_arch = "wasm32") { 5 } else { 30 })
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "ui", &self.ui);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        control::feed_input(self, raw);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.step(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl MaterialCraftApp {
    /// Per-frame logic before drawing: styles, the control channel, cooking, shortcuts
    pub fn step(&mut self, ctx: &egui::Context) {
        if !self.fonts_ready {
            theme::install(ctx);
            ctx.request_repaint();
            return;
        }
        i18n::set_language(&self.ui.prefs.language);
        self.fonts.update(ctx, &self.ui.prefs.language, self.ui.windows.preferences);
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_time) as f32;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        self.last_time = now;
        let style = (self.ui.prefs.brightness, self.ui.prefs.accent);
        if self.styled != Some(style) {
            theme::apply(ctx, style.0, style.1);
            self.styled = Some(style);
        }
        let scale = self.ui.prefs.ui_scale.clamp(0.5, 3.0);
        if (ctx.zoom_factor() - scale).abs() > 1e-3 && !ctx.input(|input| input.pointer.any_down()) {
            ctx.set_zoom_factor(scale);
        }
        if ctx.input(|input| input.pointer.any_pressed()) {
            self.session.finish_edit();
        }
        crate::unsaved::handle_close(self, ctx);
        control::poll(self, ctx);
        web_files::poll(self);
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            web_files::dropped(self, ctx, &f);
        }
        self.apply_prefs();
        cook::drive(self, ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.autosave(now);
        self.reconcile_editors();
        menus::shortcuts(self, ctx);
        self.toasts.retain(|t| t.until > now);
        if !self.input_queue.is_empty() {
            ctx.request_repaint();
        }
    }

    /// Draw the whole window into `ui`
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        if !self.fonts_ready {
            self.fonts_ready = true;
            return;
        }
        let ctx = ui.ctx().clone();
        let t0 = now_ms();
        // Tab can focus a menu or toolbar button while drawing; preserve keyboard ownership
        self.graph.keyboard_was_focused = ctx.egui_wants_keyboard_input();
        menus::menu_bar(self, ui);
        if self.ui.prefs.show_main_toolbar {
            toolbar::main_toolbar(self, ui);
        }
        toolbar::status_bar(self, ui);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::Tokens::get(&ctx).chrome)).show(ui, |ui| docking::show(self, ui));
        windows::show(self, &ctx);
        let gesture_active = ctx.input(|input| input.pointer.any_down()) || ctx.egui_wants_keyboard_input();
        if !gesture_active && (self.ui_gesture_active || ctx.input(|input| input.pointer.any_released())) {
            self.session.finish_edit();
        }
        self.ui_gesture_active = gesture_active;
        crate::toast::show(self, &ctx);
        control::request_screenshots(self, &ctx);
        let dt = (now_ms() - t0) as f32;
        self.ui_ms = if self.ui_ms == 0.0 { dt } else { self.ui_ms * 0.9 + dt * 0.1 };
    }
}

#[cfg(test)]
#[path = "material_craft_app.test.rs"]
mod tests;
