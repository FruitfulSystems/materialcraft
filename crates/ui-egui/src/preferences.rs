//! Edit ▸ Preferences: a tabbed window (General, Projects, Graph, 3D View, Bakers, Library,
//! Shortcuts) edited on a copy of the preferences; OK and Apply push the copy into the app.

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as icon;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::Prefs;
use crate::theme::{Accent, Brightness, Tokens};

/// The tabs, in order.
const TABS: [&str; 7] = ["General", "Projects", "Graph", "3D View", "Bakers", "Library", "Shortcuts"];

/// The window's working copy and tab (kept while it is open).
#[derive(Default)]
pub struct PrefsEditor {
    draft: Option<Prefs>,
    /// The preferences when the window opened (Cancel restores the live-previewed ones).
    original: Option<Prefs>,
    tab: usize,
    shortcut_search: String,
    shortcut_key_search: String,
    /// Node shortcuts being edited: def id → shortcut text.
    node_shortcuts: Vec<(String, String)>,
    /// The shortcut field waiting for a key press.
    capturing: Option<String>,
}

fn heading(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(RichText::new(text.to_uppercase()).size(12.5).color(tk.text_strong));
    ui.add_space(2.0);
}

fn sub_heading(ui: &mut Ui, text: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(tk.text_dim));
}

/// A numeric field with a unit: "[ 10 ] Files".
fn number(ui: &mut Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>, unit: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let r = ui.add(egui::DragValue::new(value).range(range).speed(1.0));
        crate::widgets::name(&r, egui::Role::SpinButton, label);
        changed = r.changed();
        ui.label(unit);
    });
    changed
}

/// A folder field with reset, browse and "use" buttons.
fn folder_field(ui: &mut Ui, label: &str, value: &mut String, default_hint: &str) {
    ui.label(label);
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(value).hint_text(default_hint).desired_width(ui.available_width() - 60.0));
        crate::widgets::name(&r, egui::Role::TextInput, label);
        if crate::widgets::icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset to the default"), false, 20.0).clicked() {
            value.clear();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::widgets::icon_button(ui, icon::DOTS_THREE, tr("Browse..."), false, 20.0).clicked()
            && let Some(p) = rfd::FileDialog::new().pick_folder()
        {
            *value = p.display().to_string();
        }
    });
}

pub fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if app.prefs_editor.draft.is_none() {
        app.prefs_editor.draft = Some(app.ui.prefs.clone());
        app.prefs_editor.original = Some(app.ui.prefs.clone());
        app.prefs_editor.node_shortcuts = app.ui.node_shortcuts.clone();
    }
    let mut open = true;
    let mut action: Option<&str> = None;
    egui::Window::new(tr("Preferences"))
        .id(egui::Id::unique("preferences"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(760.0, 560.0))
        .show(ctx, |ui| {
            let tk = Tokens::get(ui.ctx());
            let total = ui.available_height() - 36.0;
            ui.horizontal_top(|ui| {
                // Tabs down the left.
                ui.allocate_ui_with_layout(vec2(150.0, total), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, tk.panel);
                    ui.add_space(6.0);
                    for (i, tab) in TABS.iter().enumerate() {
                        let on = app.prefs_editor.tab == i;
                        let (r, resp) = ui.allocate_exact_size(vec2(146.0, 32.0), egui::Sense::click());
                        crate::widgets::name(&resp, egui::Role::Tab, tab);
                        if on {
                            ui.painter().rect_filled(r, 3.0, tk.row_selected);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 3.0, tk.row_hover);
                        }
                        ui.painter().text(
                            r.left_center() + vec2(10.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            tr(tab).to_uppercase(),
                            egui::FontId::proportional(13.0),
                            if on { tk.text_strong } else { tk.text },
                        );
                        if resp.clicked() {
                            app.prefs_editor.tab = i;
                        }
                    }
                });
                ui.separator();
                ui.allocate_ui_with_layout(vec2(ui.available_width(), total), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.set_width(ui.available_width() - 8.0);
                        let tab = app.prefs_editor.tab;
                        match tab {
                            0 => general(app, ui),
                            1 => projects(app, ui),
                            2 => graph(app, ui),
                            3 => view3d(app, ui),
                            4 => bakers(app, ui),
                            5 => library(app, ui),
                            _ => shortcuts(app, ui),
                        }
                    });
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tr("APPLY")).min_size(vec2(90.0, 24.0))).clicked() {
                    action = Some("apply");
                }
                if ui.add(egui::Button::new(tr("CANCEL")).min_size(vec2(90.0, 24.0))).clicked() {
                    action = Some("cancel");
                }
                if ui.add(crate::widgets::primary_button(ui, tr("OK")).min_size(vec2(90.0, 24.0))).clicked() {
                    action = Some("ok");
                }
            });
        });
    // Appearance previews live while the window is open (Cancel puts it back).
    if let Some(d) = &app.prefs_editor.draft {
        app.ui.prefs.ui_scale = d.ui_scale;
        app.ui.prefs.brightness = d.brightness;
        app.ui.prefs.accent = d.accent;
    }
    match action {
        Some("apply") => apply(app),
        Some("ok") => {
            apply(app);
            open = false;
        }
        Some("cancel") => {
            if let Some(o) = app.prefs_editor.original.take() {
                app.ui.prefs.ui_scale = o.ui_scale;
                app.ui.prefs.brightness = o.brightness;
                app.ui.prefs.accent = o.accent;
            }
            open = false;
        }
        _ => {}
    }
    if !open {
        app.ui.windows.preferences = false;
        app.prefs_editor.draft = None;
        app.prefs_editor.original = None;
        app.prefs_editor.capturing = None;
    }
}

/// Push the draft into the live preferences.
fn apply(app: &mut MaterialCraftApp) {
    let Some(draft) = app.prefs_editor.draft.clone() else { return };
    app.ui.prefs = draft;
    app.ui.node_shortcuts = app.prefs_editor.node_shortcuts.clone();
    crate::i18n::set_language(&app.ui.prefs.language);
    app.apply_prefs();
    app.toast(tr("Preferences applied"), false);
}

fn draft(app: &mut MaterialCraftApp) -> &mut Prefs {
    app.prefs_editor.draft.get_or_insert_with(Prefs::default)
}

fn general(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let cache_used = app.session.cache.lock().map(|c| c.bytes() >> 20).unwrap_or(0);
    let cache_cap = app.session.cache.lock().map(|c| c.cap() >> 20).unwrap_or(0);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let p = draft(app);
    heading(ui, tr("Recent documents"));
    number(ui, tr("Recent documents list contains:"), &mut p.recent_count, 1..=50, tr("Files"));
    heading(ui, tr("History"));
    number(ui, tr("History stack size:"), &mut p.history_size, 1..=10_000, tr("States"));
    heading(ui, tr("Language"));
    ui.label(tr("Choose the language of the application:"));
    crate::widgets::combo(ui, "", crate::i18n::LANGUAGES.iter().find(|l| l.code == p.language).map_or("English", |l| l.name), |ui| {
        for l in crate::i18n::LANGUAGES.iter().filter(|l| l.code == "en") {
            if ui.selectable_label(p.language == l.code, l.name).clicked() {
                p.language = l.code.to_owned();
            }
        }
    });
    ui.label(RichText::new(tr("More languages arrive in a later version.")).small().color(tk.text_dim));
    heading(ui, tr("Appearance"));
    crate::widgets::combo(ui, tr("Brightness"), tr(p.brightness.label()), |ui| {
        for b in Brightness::ALL {
            ui.selectable_value(&mut p.brightness, b, tr(b.label()));
        }
    });
    crate::widgets::combo(ui, tr("Accent color"), tr(p.accent.label()), |ui| {
        for a in Accent::ALL {
            ui.selectable_value(&mut p.accent, a, tr(a.label()));
        }
    });
    crate::widgets::slider(ui, tr("Interface scale"), &mut p.ui_scale, 0.75..=2.0, 2);
    ui.checkbox(&mut p.show_home, tr("Show the home screen at start"));
    heading(ui, tr("Views"));
    ui.checkbox(&mut p.invert_zoom, tr("Invert zoom in views"));
    heading(ui, tr("Paths"));
    ui.label(tr("Save/Export Path:"));
    let modes = [("last", "Last path"), ("package", "The package's folder"), ("custom", "A fixed folder")];
    let cur = modes.iter().find(|(id, _)| *id == p.save_path_mode).map_or("Last path", |(_, l)| l);
    crate::widgets::combo(ui, "", tr(cur), |ui| {
        for (id, label) in modes {
            if ui.selectable_label(p.save_path_mode == id, tr(label)).clicked() {
                p.save_path_mode = id.into();
            }
        }
    });
    if p.save_path_mode == "custom" {
        folder_field(ui, tr("Fixed folder:"), &mut p.save_path, "");
    }
    folder_field(ui, tr("Temporary folder (auto backups):"), &mut p.temp_dir, &crate::user_dir("autosave").display().to_string());
    heading(ui, tr("Memory"));
    sub_heading(ui, tr("Image cache"));
    ui.label(format!("{}: {cache_used} MB {} {cache_cap} MB", tr("Current memory used"), tr("of")));
    ui.label(tr("Memory Budget:"));
    ui.horizontal(|ui| {
        let auto = p.cache_mb == 0;
        let r = egui::ComboBox::from_id_salt("cache-mode").selected_text(if auto { tr("Automatic") } else { tr("Fixed") }).show_ui(ui, |ui| {
            if ui.selectable_label(auto, tr("Automatic")).clicked() {
                p.cache_mb = 0;
            }
            if ui.selectable_label(!auto, tr("Fixed")).clicked() && auto {
                p.cache_mb = 1024;
            }
        });
        crate::widgets::name(&r.response, egui::Role::ComboBox, "Memory budget");
        if !auto {
            let r = ui.add(egui::DragValue::new(&mut p.cache_mb).range(16..=65_536).speed(16.0).suffix(" MB"));
            crate::widgets::name(&r, egui::Role::SpinButton, "Cache MB");
        } else {
            ui.label(RichText::new("1024 MB").color(tk.text_dim));
        }
    });
    sub_heading(ui, tr("Job scheduler"));
    ui.label(format!("{}: {cores}", tr("Cores on this machine")));
    let mut threads = p.cook_threads as u32;
    if number(ui, tr("Cook threads (0 = every core):"), &mut threads, 0..=256, tr("threads")) {
        p.cook_threads = threads as usize;
    }
    heading(ui, tr("Auto backup"));
    let mut backup_on = p.autosave_minutes > 0;
    if ui.checkbox(&mut backup_on, tr("Auto backup")).changed() {
        p.autosave_minutes = if backup_on { 5 } else { 0 };
    }
    ui.add_enabled_ui(backup_on, |ui| {
        let mut m = p.autosave_minutes.max(1);
        if number(ui, tr("Auto backup every:"), &mut m, 1..=120, tr("minutes")) {
            p.autosave_minutes = m;
        }
        number(ui, tr("Keep up to:"), &mut p.backup_versions, 1..=50, tr("versions"));
    });
    ui.label(
        RichText::new(tr("Backups are written to the temporary folder above; open one with File ▸ Open and save it over the original."))
            .small()
            .color(tk.text_dim),
    );
    heading(ui, tr("Cooker"));
    ui.horizontal(|ui| {
        ui.label(tr("Cooking size limit:"));
        let mut v = (31 - p.cook_limit.clamp(256, 8192).leading_zeros()) as f32;
        if ui.add(egui::Slider::new(&mut v, 8.0..=13.0).show_value(false).step_by(1.0)).changed() {
            p.cook_limit = 1 << (v as u32);
        }
        ui.label(format!("{} {}", p.cook_limit, tr("pixels")));
    });
    ui.label(
        RichText::new(tr("Caps every cook, exports included. Preview size while editing is set in the 3D view's Display menu.")).small().color(tk.text_dim),
    );
}

fn projects(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    heading(ui, tr("Configuration"));
    ui.horizontal(|ui| {
        ui.label(tr("Preferences file:"));
        ui.label(RichText::new(tr("kept by the app in its settings store")).color(tk.text_dim));
    });
    ui.horizontal(|ui| {
        ui.label(tr("Plugins folder:"));
        ui.label(RichText::new(crate::plugins::folder().display().to_string()).color(tk.text_dim));
        if ui.small_button(tr("Plugin Manager...")).clicked() {
            app.ui.windows.plugin_manager = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Backups folder:"));
        let dir = if app.ui.prefs.temp_dir.is_empty() { crate::user_dir("autosave").display().to_string() } else { app.ui.prefs.temp_dir.clone() };
        ui.label(RichText::new(dir).color(tk.text_dim));
    });
    heading(ui, tr("Project"));
    sub_heading(ui, tr("3D View"));
    ui.horizontal(|ui| {
        ui.label(tr("Default State File:"));
        let has = app.ui.view3d_default.is_some();
        ui.label(RichText::new(if has { tr("saved (Scene ▸ Save Current State As Default)") } else { tr("none: the built-in state") }).color(tk.text_dim));
        if has && ui.small_button(tr("Clear")).clicked() {
            app.ui.view3d_default = None;
        }
        if ui.small_button(tr("Save the current 3D view as the default")).clicked() {
            app.ui.view3d_default = app.run("view.3d.state", serde_json::json!({}));
        }
    });
    let p = draft(app);
    ui.label(tr("Default Lighting State (if not set by Default State File):"));
    ui.indent("lights", |ui| {
        ui.checkbox(&mut p.default_lights[0], tr("Ambient Light"));
        ui.checkbox(&mut p.default_lights[1], tr("Point Light 1"));
        ui.checkbox(&mut p.default_lights[2], tr("Point Light 2"));
    });
    sub_heading(ui, tr("Library"));
    ui.checkbox(&mut p.library_list, tr("Show the library as a list"));
    crate::widgets::slider(ui, tr("Library tile size"), &mut p.library_tile, 56.0..=160.0, 0);
    sub_heading(ui, tr("General"));
    crate::widgets::combo(
        ui,
        tr("Preview size while editing"),
        if p.preview_size == 0 { tr("Full size").to_string() } else { format!("{} px", p.preview_size) },
        |ui| {
            for s in [256u32, 512, 1024, 2048, 4096, 0] {
                let label = if s == 0 { tr("Full size").to_string() } else { format!("{s} px") };
                ui.selectable_value(&mut p.preview_size, s, label);
            }
        },
    );
    crate::widgets::slider(ui, tr("3D pan limit (mesh radii)"), &mut p.camera.pan_limit, 0.0..=20.0, 1);
    crate::widgets::slider(ui, tr("3D pan speed"), &mut p.camera.pan_speed, 0.1..=4.0, 2);
}

fn graph(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let p = draft(app);
    heading(ui, tr("Common"));
    ui.checkbox(&mut p.tab_key_menu, tr("Tab key shows node menu"));
    ui.checkbox(&mut p.drag_create, tr("Enable creation of nodes by click-dragging connectors"));
    ui.checkbox(&mut p.view_3d_on_open, tr("View outputs in 3D view when opening a graph"));
    heading(ui, tr("Material graph"));
    ui.checkbox(&mut p.thumbnails, tr("Automatically compute all nodes thumbnails when opening a graph"));
    ui.checkbox(&mut p.view_2d_on_open, tr("View output in 2D view when opening a graph"));
    ui.checkbox(&mut p.auto_display_new_node, tr("Automatically display newly created node"));
    ui.checkbox(&mut p.auto_convert, tr("Automatically insert color/grayscale conversion node"));
    heading(ui, tr("Graph toolbar"));
    ui.checkbox(&mut p.show_grid, tr("Show the grid"));
    ui.checkbox(&mut p.snap_grid, tr("Snap nodes to the grid"));
    ui.checkbox(&mut p.straight_wires, tr("Straight wires"));
    ui.checkbox(&mut p.node_timings, tr("Show cook times on nodes"));
}

fn view3d(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let p = draft(app);
    heading(ui, tr("Misc"));
    ui.checkbox(&mut p.env_hidden, tr("Environment hidden by default"));
    crate::widgets::combo(
        ui,
        tr("Viewport Scaling"),
        if p.viewport_scale <= 0.0 { tr("Auto").to_string() } else { format!("{:.0} %", p.viewport_scale * 100.0) },
        |ui| {
            if ui.selectable_label(p.viewport_scale <= 0.0, tr("Auto")).clicked() {
                p.viewport_scale = 0.0;
            }
            for s in [0.5f32, 0.75, 1.0, 1.5, 2.0] {
                if ui.selectable_label((p.viewport_scale - s).abs() < 1e-3, format!("{:.0} %", s * 100.0)).clicked() {
                    p.viewport_scale = s;
                }
            }
        },
    );
    heading(ui, tr("Renderer"));
    sub_heading(ui, tr("Shading"));
    let labels = [(1u32, "Low (1 spp)"), (2, "Medium (4 spp)"), (3, "High (9 spp)"), (4, "Very high (16 spp)")];
    let cur = labels.iter().find(|(n, _)| *n == p.sample_count).map_or("Medium (4 spp)", |(_, l)| l);
    crate::widgets::combo(ui, tr("Sample Count"), tr(cur), |ui| {
        for (n, label) in labels {
            if ui.selectable_label(p.sample_count == n, tr(label)).clicked() {
                p.sample_count = n;
            }
        }
    });
    ui.label(
        RichText::new(tr("Samples per pixel of the resting frame; the renderer settings of a session can override it."))
            .small()
            .color(Tokens::get(ui.ctx()).text_dim),
    );
}

fn bakers(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let p = draft(app);
    heading(ui, tr("Bakers"));
    let mut threads = p.bake_threads;
    if number(ui, tr("Baking threads (0 = every core):"), &mut threads, 0..=256, tr("threads")) {
        p.bake_threads = threads;
    }
    ui.label(RichText::new(tr("Mesh maps bake on the CPU ray tracer; GPU ray tracing arrives with the GPU renderer.")).small().color(tk.text_dim));
}

fn library(app: &mut MaterialCraftApp, ui: &mut Ui) {
    heading(ui, tr("Library"));
    if ui.add(egui::Button::new(tr("Rebuild Thumbnails")).min_size(vec2(260.0, 24.0))).clicked() {
        app.thumbs.rebuild_library();
        app.toast(tr("Library thumbnails are being rebuilt"), false);
    }
    ui.add_space(6.0);
    if ui.add(egui::Button::new(tr("Clear the cooked image cache")).min_size(vec2(260.0, 24.0))).clicked() {
        if let Ok(mut c) = app.session.cache.lock() {
            c.clear();
        }
        app.session.dirty = true;
    }
}

fn shortcuts(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        heading(ui, tr("Node creation shortcuts"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button(ui, icon::ARROWS_CLOCKWISE, tr("Clear every shortcut"), false, 22.0).clicked() {
                app.prefs_editor.node_shortcuts.clear();
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.prefs_editor.shortcut_search).hint_text(tr("Search")).desired_width(220.0));
        ui.add(egui::TextEdit::singleline(&mut app.prefs_editor.shortcut_key_search).hint_text(tr("Search by shortcut")).desired_width(160.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let n = app.prefs_editor.node_shortcuts.len();
            ui.label(RichText::new(format!("{n} {}", tr("assigned"))).color(tk.text_dim));
        });
    });
    ui.label(RichText::new(tr("Click a field and press the keys (Escape clears). The node is added next to the selection.")).small().color(tk.text_dim));
    let search = app.prefs_editor.shortcut_search.to_ascii_lowercase();
    let key_search = app.prefs_editor.shortcut_key_search.to_ascii_lowercase();
    // Capture a chord for the field that waits for one.
    if let Some(def) = app.prefs_editor.capturing.clone() {
        let pressed = ui.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                return Some(String::new());
            }
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } if !matches!(key, egui::Key::Escape) => {
                    let mut parts = Vec::new();
                    if modifiers.ctrl || modifiers.command {
                        parts.push("Ctrl");
                    }
                    if modifiers.shift {
                        parts.push("Shift");
                    }
                    if modifiers.alt {
                        parts.push("Alt");
                    }
                    let name = key.name();
                    parts.push(name);
                    Some(parts.join("+"))
                }
                _ => None,
            })
        });
        if let Some(chord) = pressed {
            app.prefs_editor.node_shortcuts.retain(|(d, _)| *d != def);
            if !chord.is_empty() {
                // One chord per node: the previous owner loses it.
                app.prefs_editor.node_shortcuts.retain(|(_, c)| *c != chord);
                app.prefs_editor.node_shortcuts.push((def, chord));
            }
            app.prefs_editor.capturing = None;
        }
    }
    egui::Grid::new("node-shortcuts").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
        for d in materialcraft_engine::nodes::catalog() {
            if d.id == "graph_instance" {
                continue;
            }
            let current = app.prefs_editor.node_shortcuts.iter().find(|(def, _)| *def == d.id).map(|(_, c)| c.clone()).unwrap_or_default();
            if !search.is_empty() && !d.label.to_ascii_lowercase().contains(&search) && !d.id.contains(&search) {
                continue;
            }
            if !key_search.is_empty() && !current.to_ascii_lowercase().contains(&key_search) {
                continue;
            }
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 3.0, crate::graph_view::header_color(d.id));
                ui.label(d.label);
            });
            let capturing = app.prefs_editor.capturing.as_deref() == Some(d.id);
            let text = if capturing { tr("Press keys...").to_string() } else { current.clone() };
            let b = ui.add(egui::Button::new(if text.is_empty() { " ".to_string() } else { text }).min_size(vec2(200.0, 20.0)).selected(capturing));
            crate::widgets::name(&b, egui::Role::TextInput, d.label);
            if b.clicked() {
                app.prefs_editor.capturing = Some(d.id.to_string());
            }
            ui.end_row();
        }
    });
}
