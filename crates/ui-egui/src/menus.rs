//! The menu bar and global shortcuts. The menu catalog follows the menu layout of a standard
//! node-based material authoring workflow, with stable ids; an item is live when the engine (or
//! the shell, for `ui.*` ids) implements its id. The same catalog drives `docs/parity-checklist.md`.

use egui::{Key, KeyboardShortcut, Modifiers};
use serde_json::Value;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// One menu entry. `label == "-"` is a separator; `sub` names a submenu group.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub menu: &'static str,
    pub sub: Option<&'static str>,
    pub label: &'static str,
    pub id: &'static str,
    pub shortcut: Option<&'static str>,
}

const fn it(menu: &'static str, label: &'static str, id: &'static str, shortcut: Option<&'static str>) -> Item {
    Item { menu, sub: None, label, id, shortcut }
}
const fn sub(menu: &'static str, sub: &'static str, label: &'static str, id: &'static str, shortcut: Option<&'static str>) -> Item {
    Item { menu, sub: Some(sub), label, id, shortcut }
}
const fn sep(menu: &'static str) -> Item {
    Item { menu, sub: None, label: "-", id: "", shortcut: None }
}

pub const TOP: [&str; 5] = ["File", "Edit", "Tools", "Windows", "Help"];

/// The menu catalog (the target menu tree with our ids).
pub const CATALOG: &[Item] = &[
    sub("File", "New", "Material Graph...", "ui.newGraph", Some("Ctrl+N")),
    sub("File", "New", "Function graph", "ui.newFunctionGraph", None),
    sub("File", "New", "Empty package", "file.newPackage", None),
    it("File", "Open...", "file.open", Some("Ctrl+O")),
    // "Recent packages" follows Open (menu_item draws it from the recent list).
    sep("File"),
    it("File", "Save all", "ui.saveAll", Some("Ctrl+Shift+S")),
    it("File", "Close all", "ui.closeAll", None),
    it("File", "Reload resources", "ui.reloadResources", None),
    sep("File"),
    it("File", "Exit", "ui.quit", Some("Ctrl+Q")),
    it("Edit", "Undo", "edit.undo", Some("Ctrl+Z")),
    it("Edit", "Redo", "edit.redo", Some("Ctrl+Y")),
    sep("Edit"),
    it("Edit", "Preferences...", "ui.preferences", None),
    it("Tools", "Cancel rendering", "ui.cancelRender", Some("Escape")),
    it("Tools", "Suspend engine", "ui.suspendEngine", Some("Shift+Escape")),
    sep("Tools"),
    it("Tools", "Switch engine...", "ui.switchEngine", Some("F9")),
    sep("Tools"),
    it("Tools", "Compute graph", "graph.cook", Some("F5")),
    it("Tools", "Clear cache", "ui.clearCache", None),
    sub("Tools", "Preview size", "256 px", "ui.preview.256", None),
    sub("Tools", "Preview size", "512 px", "ui.preview.512", None),
    sub("Tools", "Preview size", "1024 px", "ui.preview.1024", None),
    sub("Tools", "Preview size", "2048 px", "ui.preview.2048", None),
    sub("Tools", "Preview size", "Full size", "ui.preview.0", None),
    it("Tools", "Create graph from selection", "graph.createFromSelection", None),
    sep("Tools"),
    it("Tools", "Import bitmap...", "file.importBitmap", None),
    it("Tools", "Link bitmap...", "file.linkBitmap", None),
    it("Tools", "Export outputs...", "ui.export", Some("Ctrl+Shift+E")),
    it("Tools", "Export material...", "file.exportMaterial", None),
    it("Tools", "Send to TextureCraft", "file.sendToTextureCraft", None),
    sep("Tools"),
    it("Tools", "Plugin Manager...", "ui.pluginManager", None),
    it("Windows", "New Explorer", "ui.new.explorer", None),
    it("Windows", "New 3D View", "ui.new.3d", None),
    it("Windows", "New Library View", "ui.new.library", None),
    it("Windows", "Reset layout", "ui.resetLayout", None),
    it("Windows", "Unmaximize window", "ui.unmaximize", Some("Shift+Space")),
    sep("Windows"),
    it("Windows", "Explorer", "ui.view.explorer", None),
    it("Windows", "Graph", "ui.view.graph", None),
    it("Windows", "Parameters", "ui.view.properties", None),
    it("Windows", "Console", "ui.view.console", None),
    it("Windows", "3D View", "ui.view.3d", None),
    it("Windows", "Dependency Manager", "ui.view.dependencies", None),
    it("Windows", "2D View", "ui.view.2d", None),
    it("Windows", "Library", "ui.view.library", None),
    it("Windows", "History", "ui.view.history", None),
    it("Windows", "Main toolbar", "ui.view.mainToolbar", None),
    it("Help", "Tutorials...", "ui.help.tutorials", None),
    it("Help", "Release notes...", "ui.help.releaseNotes", None),
    it("Help", "Technical requirements...", "ui.help.requirements", None),
    it("Help", "Documentation...", "ui.help.docs", None),
    it("Help", "Automation documentation...", "ui.help.automation", None),
    sep("Help"),
    it("Help", "Forums...", "ui.help.discord", None),
    it("Help", "Report a bug...", "ui.help.bug", None),
    it("Help", "Export log...", "ui.help.exportLog", None),
    it("Help", "Give feedback...", "ui.help.feedback", None),
    sep("Help"),
    it("Help", "Welcome screen...", "ui.home", None),
    it("Help", "What's new...", "ui.help.whatsNew", None),
    it("Help", "Shortcuts list...", "ui.help.shortcuts", None),
    sep("Help"),
    it("Help", "About MaterialCraft...", "ui.about", None),
];

/// Items that are not in a menu but keep a global shortcut (the reference keeps them on the
/// graph's and the explorer's context menus).
pub const HIDDEN_SHORTCUTS: &[(&str, &str)] = &[("file.save", "Ctrl+S")];

fn is_ui(id: &str) -> bool {
    id.starts_with("ui.")
}

/// Whether an item can run now.
pub fn enabled(app: &MaterialCraftApp, id: &str) -> bool {
    if is_ui(id) {
        return match id {
            "ui.newFunctionGraph" => false,
            "ui.cancelRender" => app.cooker.busy(),
            "ui.unmaximize" => app.ui.dock.maximized.is_some(),
            _ => true,
        };
    }
    materialcraft_engine::find(id).is_some_and(|c| (c.enabled)(&app.session).is_ok())
}

/// Whether an id is implemented at all (parity report).
pub fn implemented(id: &str) -> bool {
    (is_ui(id) && id != "ui.newFunctionGraph") || materialcraft_engine::find(id).is_some()
}

/// Invoke a menu item (menus, shortcuts, the control channel's `ui.menu.invoke`).
pub fn invoke(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str) {
    if let Some(panel) = crate::docking::view_panel(id) {
        app.ui.dock.toggle(panel);
        return;
    }
    if let Some(size) = id.strip_prefix("ui.preview.").and_then(|s| s.parse::<u32>().ok()) {
        app.ui.prefs.preview_size = size;
        app.apply_prefs();
        return;
    }
    match id {
        "ui.newGraph" => app.ui.windows.new_graph = true,
        "ui.export" => app.ui.windows.export = true,
        "ui.quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        "ui.preferences" => app.ui.windows.preferences = true,
        "ui.pluginManager" => app.ui.windows.plugin_manager = true,
        "ui.switchEngine" => app.ui.windows.switch_engine = true,
        "ui.view.dependencies" => app.ui.windows.dependencies = !app.ui.windows.dependencies,
        "ui.view.mainToolbar" => app.ui.prefs.show_main_toolbar = !app.ui.prefs.show_main_toolbar,
        "ui.saveAll" => invoke(app, ctx, "file.save"),
        "ui.cancelRender" => crate::cook::cancel(app),
        "ui.suspendEngine" => {
            app.cooker.suspended = !app.cooker.suspended;
            if !app.cooker.suspended {
                app.session.dirty = true;
            }
            let msg = if app.cooker.suspended { tr("Engine suspended: nothing computes until it is resumed") } else { tr("Engine resumed") };
            app.toast(msg, false);
        }
        "ui.new.explorer" => app.ui.dock.focus(crate::docking::Panel::Explorer),
        "ui.new.3d" => app.ui.dock.focus(crate::docking::Panel::View3d),
        "ui.new.library" => app.ui.dock.focus(crate::docking::Panel::Library),
        "ui.unmaximize" => app.ui.dock.maximized = None,
        "ui.view.graph" => {}
        "ui.resetLayout" => app.ui.dock = crate::docking::Dock::default(),
        // These replace the open package: ask first when it has unsaved changes.
        "ui.closeAll" | "file.newPackage" => crate::unsaved::guard(app, crate::unsaved::Guarded::Menu { id: id.to_string() }),
        "ui.clearCache" => {
            if let Ok(mut c) = app.session.cache.lock() {
                c.clear();
            }
            app.session.dirty = true;
        }
        "ui.reloadResources" => {
            if let Some(r) = app.run("file.reloadResources", Value::Null) {
                let n = r["reloaded"].as_array().map_or(0, Vec::len);
                let missing = r["missing"].as_array().map_or(0, Vec::len);
                if let Ok(mut c) = app.session.cache.lock() {
                    c.clear();
                }
                app.session.dirty = true;
                let msg = match (n, missing) {
                    (0, 0) => tr("Resources reloaded: nothing had changed").to_string(),
                    (n, 0) => format!("{} {n}", tr("Resources reloaded:")),
                    (n, m) => format!("{} {n}, {} {m} ({})", tr("Resources reloaded:"), tr("missing:"), tr("see the Console")),
                };
                app.toast(msg, missing > 0);
            }
        }
        "ui.home" => app.ui.windows.home = true,
        "ui.help.shortcuts" => app.ui.windows.shortcuts = true,
        "ui.about" => app.ui.windows.about = true,
        "ui.help.discord" => ctx.open_url(egui::OpenUrl::new_tab("https://discord.gg/artcraft")),
        "ui.help.website" => ctx.open_url(egui::OpenUrl::new_tab("https://getartcraft.com/apps/materialcraft")),
        "ui.help.docs" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft#readme")),
        "ui.help.tutorials" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/tree/main/docs")),
        "ui.help.releaseNotes" | "ui.help.whatsNew" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/releases")),
        "ui.help.requirements" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/blob/main/docs/releasing.md")),
        "ui.help.automation" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/blob/main/docs/control-protocol.md")),
        "ui.help.feedback" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/discussions")),
        "ui.help.bug" => ctx.open_url(egui::OpenUrl::new_tab("https://github.com/storytold/materialcraft/issues")),
        "ui.help.exportLog" => crate::windows::export_log(app),
        _ => {
            if crate::windows::file_action(app, ctx, id) {
                return;
            }
            app.run(id, Value::Null);
        }
    }
}

/// Parse "Ctrl+Shift+E" style shortcuts.
pub fn parse_shortcut(s: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Ctrl" | "Cmd" => mods |= Modifiers::COMMAND,
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" => mods |= Modifiers::ALT,
            k => key = Key::from_name(k),
        }
    }
    key.map(|k| KeyboardShortcut::new(mods, k))
}

/// The shortcut text shown for `id` (the catalog's, else the engine command's).
pub fn shortcut_of(id: &str) -> Option<&'static str> {
    CATALOG
        .iter()
        .find(|i| i.id == id)
        .and_then(|i| i.shortcut)
        .or_else(|| HIDDEN_SHORTCUTS.iter().find(|(h, _)| *h == id).map(|(_, sc)| *sc))
        .or_else(|| materialcraft_engine::find(id).and_then(|c| c.shortcut))
}

pub fn menu_bar(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::top("menu_bar").exact_size(24.0).frame(egui::Frame::NONE.fill(tk.chrome).inner_margin(egui::Margin::symmetric(6, 0))).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            let ctx = ui.ctx().clone();
            for top in TOP {
                ui.menu_button(tr(top), |ui| {
                    ui.set_min_width(220.0);
                    let mut current_sub: Option<&str> = None;
                    let items: Vec<Item> = CATALOG.iter().filter(|i| i.menu == top).copied().collect();
                    for item in &items {
                        if let Some(s) = item.sub {
                            if current_sub == Some(s) {
                                continue;
                            }
                            current_sub = Some(s);
                            ui.menu_button(tr(s), |ui| {
                                for x in items.iter().filter(|x| x.sub == Some(s)) {
                                    menu_item(app, &ctx, ui, x);
                                }
                            });
                            continue;
                        }
                        if item.label == "-" {
                            ui.separator();
                            continue;
                        }
                        menu_item(app, &ctx, ui, item);
                    }
                });
            }
        });
    });
}

/// A context-menu row with an optional shortcut hint on the right.
pub fn menu_row(ui: &mut egui::Ui, label: &str, key: &str, enabled: bool) -> egui::Response {
    let mut b = egui::Button::new(label);
    if !key.is_empty() {
        b = b.shortcut_text(key);
    }
    ui.add_enabled(enabled, b)
}

fn menu_item(app: &mut MaterialCraftApp, ctx: &egui::Context, ui: &mut egui::Ui, item: &Item) {
    let on = enabled(app, item.id);
    let mut b = egui::Button::new(tr(item.label));
    if let Some(sc) = item.shortcut {
        b = b.shortcut_text(sc);
    }
    let checked = match item.id {
        "ui.view.graph" => Some(true),
        "ui.view.mainToolbar" => Some(app.ui.prefs.show_main_toolbar),
        "ui.view.dependencies" => Some(app.ui.windows.dependencies),
        "ui.suspendEngine" => Some(app.cooker.suspended),
        id if id.starts_with("ui.preview.") => id.strip_prefix("ui.preview.").and_then(|s| s.parse::<u32>().ok()).map(|s| s == app.ui.prefs.preview_size),
        id => crate::docking::view_panel(id).map(|p| app.ui.dock.is_open(p)),
    };
    if let Some(c) = checked {
        b = b.selected(c);
    }
    if ui.add_enabled(on, b).clicked() {
        invoke(app, ctx, item.id);
        ui.close();
    }
    #[cfg(not(target_arch = "wasm32"))]
    if item.id == "file.open" {
        ui.add_enabled_ui(!app.ui.recent.is_empty(), |ui| {
            ui.menu_button(tr("Recent packages"), |ui| {
                for path in app.ui.recent.clone() {
                    if ui.button(&path).clicked() {
                        crate::windows::open_path(app, &path);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("Clear recent packages")).clicked() {
                    app.ui.recent.clear();
                    ui.close();
                }
            });
        });
    }
}

/// Global shortcuts from the catalog and the engine commands (skipped while a text field has
/// the keyboard).
pub fn shortcuts(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    if !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
        return;
    }
    // Selected wires take Delete and Backspace before the node commands do, and X swaps them.
    if !app.graph.selected_wires.is_empty() {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) | i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)) {
            crate::graph_view::delete_selected_wires(app);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::X)) {
            crate::graph_view::swap_selected_wires(app);
        }
    }
    let mut table: Vec<(&'static str, KeyboardShortcut)> = CATALOG.iter().filter_map(|i| Some((i.id, parse_shortcut(i.shortcut?)?))).collect();
    table.extend(HIDDEN_SHORTCUTS.iter().filter_map(|(id, sc)| Some((*id, parse_shortcut(sc)?))));
    // A disabled item leaves its key alone (Escape closes dialogs unless a cook is running).
    table.retain(|(id, _)| !is_ui(id) || enabled(app, id));
    for c in materialcraft_engine::command_specs() {
        if table.iter().any(|(id, _)| *id == c.id) {
            continue;
        }
        if let Some(sc) = c.shortcut.and_then(parse_shortcut) {
            table.push((c.id, sc));
        }
    }
    // Longer chords first, so Ctrl+Shift+S is not taken by Ctrl+S.
    table.sort_by_key(|(_, sc)| std::cmp::Reverse(sc.modifiers.ctrl as u8 + sc.modifiers.shift as u8 + sc.modifiers.alt as u8 + sc.modifiers.command as u8));
    let mut fire: Vec<&str> = Vec::new();
    ctx.input_mut(|i| {
        for (id, sc) in &table {
            if i.consume_shortcut(sc) {
                fire.push(*id);
            }
        }
    });
    for id in fire {
        invoke(app, ctx, id);
    }
    // Node creation shortcuts (Preferences ▸ Shortcuts): add the node next to the selection.
    let node_keys: Vec<(String, KeyboardShortcut)> = app.ui.node_shortcuts.iter().filter_map(|(def, sc)| Some((def.clone(), parse_shortcut(sc)?))).collect();
    let mut add: Vec<String> = Vec::new();
    ctx.input_mut(|i| {
        for (def, sc) in &node_keys {
            if i.consume_shortcut(sc) {
                add.push(def.clone());
            }
        }
    });
    for def in add {
        if app.ui.dock.is_open(crate::docking::Panel::Graph) {
            crate::graph_view::add_node_near_selection(app, &def);
        }
    }
}

/// Menu items for `ui.menu.list`.
pub fn list(app: &MaterialCraftApp) -> Value {
    Value::Array(
        CATALOG
            .iter()
            .filter(|i| i.label != "-")
            .map(|i| {
                serde_json::json!({"menu": i.menu, "submenu": i.sub, "label": i.label, "id": i.id, "shortcut": i.shortcut,
                    "implemented": implemented(i.id), "enabled": enabled(app, i.id)})
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_shortcuts_parse_and_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for i in CATALOG.iter().filter(|i| i.label != "-") {
            assert!(seen.insert(i.id), "duplicate menu id {}", i.id);
            if let Some(sc) = i.shortcut {
                assert!(parse_shortcut(sc).is_some(), "{sc}");
            }
        }
        // Almost every item is live.
        let live = CATALOG.iter().filter(|i| i.label != "-" && implemented(i.id)).count();
        assert!(live + 1 >= CATALOG.iter().filter(|i| i.label != "-").count());
    }
}
