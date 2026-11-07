//! Plugins (Tools ▸ Plugin Manager): packages dropped in the plugins folder whose graphs the
//! library offers (filed under each graph's category attribute). A plugin is a package file (`.sbs`) in `Documents/MaterialCraft/plugins`,
//! or a folder there holding package files and an optional `plugin.json`
//! (`{"name", "version", "author", "description"}`). Loaded plugins are remembered per user.

use std::path::{Path, PathBuf};

use egui::{RichText, vec2};
use egui_phosphor::regular as icon;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// One plugin: a package file or a folder of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plugin {
    /// The plugin's key in the loaded list: its file or folder path.
    pub path: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// The package files and the graphs each one holds: (path, [(graph name, category attribute)]).
    pub packages: Vec<(String, Vec<(String, String)>)>,
    /// Why a package could not be read, if any.
    pub error: Option<String>,
}

impl Plugin {
    /// Every graph of the plugin: (package path, graph name, category attribute).
    pub fn graphs(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.packages.iter().flat_map(|(p, gs)| gs.iter().map(move |(g, c)| (p.as_str(), g.as_str(), c.as_str())))
    }
}

/// The plugins found in the plugins folder (scanned on first use and on Refresh).
#[derive(Default)]
pub struct Plugins {
    pub list: Vec<Plugin>,
    scanned: bool,
    pub filter: String,
}

/// The plugins folder.
pub fn folder() -> PathBuf {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::user_dir("plugins")
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("plugins")
    }
}

fn is_package(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sbs"))
}

/// The graphs of a package file, read without opening it: (name, category attribute).
#[cfg(not(target_arch = "wasm32"))]
fn graphs_of(path: &Path) -> Result<Vec<(String, String)>, String> {
    const MAX_PLUGIN_BYTES: u64 = 256 * 1024 * 1024;
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_PLUGIN_BYTES {
        return Err(format!("{} is larger than {} MB", path.display(), MAX_PLUGIN_BYTES / (1024 * 1024)));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let name = path.display().to_string();
    let linked = materialcraft_engine::file_cmds::linked_files(&name);
    let pkg = crate::cook::guard("plugin", || materialcraft_engine::resource_cmds::load_bytes(&bytes, &name, &linked).map_err(|e| e.to_string()))?;
    Ok(pkg.graphs.iter().map(|g| (g.name.clone(), g.attributes.category.clone())).collect())
}

#[cfg(target_arch = "wasm32")]
fn graphs_of(_: &Path) -> Result<Vec<(String, String)>, String> {
    Err("the browser has no plugins folder".into())
}

/// `plugin.json` next to a folder plugin's packages.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Manifest {
    name: String,
    version: String,
    author: String,
    description: String,
}

fn read_plugin(path: &Path) -> Plugin {
    let key = path.display().to_string();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin").to_string();
    let mut plugin = Plugin { path: key, name: stem, ..Plugin::default() };
    let mut files: Vec<PathBuf> = Vec::new();
    if path.is_dir() {
        if let Ok(text) = std::fs::read_to_string(path.join("plugin.json"))
            && let Ok(m) = serde_json::from_str::<Manifest>(&text)
        {
            if !m.name.trim().is_empty() {
                plugin.name = m.name.trim().chars().take(80).collect();
            }
            plugin.version = m.version.chars().take(32).collect();
            plugin.author = m.author.chars().take(80).collect();
            plugin.description = m.description.chars().take(400).collect();
        }
        if let Ok(rd) = std::fs::read_dir(path) {
            files.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_package(p)));
        }
        files.sort();
    } else {
        files.push(path.to_path_buf());
    }
    for f in files {
        match graphs_of(&f) {
            Ok(graphs) => plugin.packages.push((f.display().to_string(), graphs)),
            Err(e) => plugin.error = Some(e),
        }
    }
    if plugin.packages.is_empty() && plugin.error.is_none() {
        plugin.error = Some(tr("No package files (.sbs) in this plugin").to_string());
    }
    plugin
}

impl Plugins {
    /// Read the plugins folder again.
    pub fn refresh(&mut self) {
        self.scanned = true;
        self.list.clear();
        let Ok(rd) = std::fs::read_dir(folder()) else { return };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() || is_package(p)).collect();
        paths.sort();
        self.list = paths.iter().take(500).map(|p| read_plugin(p)).collect();
    }

    /// Scan once, on first use.
    pub fn ensure(&mut self) {
        if !self.scanned {
            self.refresh();
        }
    }
}

impl MaterialCraftApp {
    /// Is the plugin at `path` loaded (its graphs in the library)?
    pub fn plugin_loaded(&self, path: &str) -> bool {
        self.ui.plugins_loaded.iter().any(|p| p == path)
    }

    /// The graphs of every loaded plugin: (plugin name, package path, graph name, category).
    pub fn plugin_graphs(&mut self) -> Vec<(String, String, String, String)> {
        self.plugins.ensure();
        let loaded = &self.ui.plugins_loaded;
        self.plugins
            .list
            .iter()
            .filter(|p| loaded.contains(&p.path))
            .flat_map(|p| p.graphs().map(|(pkg, g, c)| (p.name.clone(), pkg.to_string(), g.to_string(), c.to_string())).collect::<Vec<_>>())
            .collect()
    }

    /// Put a plugin graph into the open package and add an instance of it (library drop/double-click).
    pub fn add_plugin_graph(&mut self, package: &str, graph: &str, at: Option<[f32; 2]>) {
        let Some(v) = self.run("file.importGraph", serde_json::json!({"path": package, "graph": graph})) else { return };
        let name = v.get("name").and_then(|n| n.as_str()).unwrap_or(graph).to_string();
        let mut p = serde_json::json!({"def": "graph_instance", "params": {"graph": name}});
        if let Some(at) = at {
            p["at"] = serde_json::json!(at);
        }
        self.run("graph.node.add", p);
    }
}

/// Copy the picked package files (or a folder) into the plugins folder.
#[cfg(not(target_arch = "wasm32"))]
fn install(app: &mut MaterialCraftApp) {
    let Some(files) = rfd::FileDialog::new().add_filter("Packages", &["sbs"]).pick_files() else { return };
    let dir = folder();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        app.toast(format!("{}: {e}", tr("Could not create the plugins folder")), true);
        return;
    }
    let mut n = 0;
    for f in files {
        let Some(name) = f.file_name() else { continue };
        match std::fs::copy(&f, dir.join(name)) {
            Ok(_) => n += 1,
            Err(e) => app.toast(format!("{}: {e}", f.display()), true),
        }
    }
    app.plugins.refresh();
    app.toast(format!("{n} {}", tr("plugin file(s) installed")), false);
}

/// Open the plugins folder in the system's file browser.
#[cfg(not(target_arch = "wasm32"))]
fn browse(app: &mut MaterialCraftApp) {
    let dir = folder();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        app.toast(format!("{}: {e}", tr("Could not create the plugins folder")), true);
        return;
    }
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    match std::process::Command::new(program).arg(&dir).spawn() {
        Ok(_) => app.toast(format!("{} {}", tr("Plugins folder:"), dir.display()), false),
        Err(e) => app.toast(format!("{} {} ({e})", tr("Plugins folder:"), dir.display()), true),
    }
}

/// Tools ▸ Plugin Manager.
pub fn window(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if !app.ui.windows.plugin_manager {
        return;
    }
    app.plugins.ensure();
    let mut open = true;
    let mut toggle: Option<(String, bool)> = None;
    let mut action: Option<&'static str> = None;
    crate::windows::modal(ctx, "plugin-manager", tr("Plugin Manager"), &mut open, vec2(640.0, 400.0), |ui| {
        let tk = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(tk.text_dim));
            ui.add(egui::TextEdit::singleline(&mut app.plugins.filter).hint_text(tr("Filter")).desired_width(220.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Refresh")).clicked() {
                    action = Some("refresh");
                }
                if ui.button(tr("Install...")).clicked() {
                    action = Some("install");
                }
                if ui.button(tr("Browse...")).on_hover_text(tr("Open the plugins folder")).clicked() {
                    action = Some("browse");
                }
            });
        });
        ui.add_space(4.0);
        let filter = app.plugins.filter.to_ascii_lowercase();
        let shown: Vec<Plugin> = app
            .plugins
            .list
            .iter()
            .filter(|p| filter.is_empty() || p.name.to_ascii_lowercase().contains(&filter) || p.path.to_ascii_lowercase().contains(&filter))
            .cloned()
            .collect();
        egui::ScrollArea::vertical().max_height(270.0).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("plugins-grid").num_columns(5).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new(tr("Loaded")).strong());
                ui.label(RichText::new(tr("Plugin")).strong());
                ui.label(RichText::new(tr("Version")).strong());
                ui.label(RichText::new(tr("Graphs")).strong());
                ui.label(RichText::new(tr("Location")).strong());
                ui.end_row();
                for p in &shown {
                    let mut on = app.plugin_loaded(&p.path);
                    let usable = p.error.is_none();
                    if ui.add_enabled(usable, egui::Checkbox::without_text(&mut on)).changed() {
                        toggle = Some((p.path.clone(), on));
                    }
                    let name = ui.label(&p.name);
                    if !p.description.is_empty() || !p.author.is_empty() {
                        name.on_hover_text(format!("{}\n{}", p.description, p.author));
                    }
                    ui.label(if p.version.is_empty() { "—".to_string() } else { p.version.clone() });
                    match &p.error {
                        Some(e) => {
                            ui.label(RichText::new(format!("{} {}", icon::WARNING, tr("unreadable"))).color(tk.danger)).on_hover_text(e);
                        }
                        None => {
                            let n = p.graphs().count();
                            ui.label(n.to_string()).on_hover_text(p.graphs().map(|(_, g, _)| g).collect::<Vec<_>>().join("\n"));
                        }
                    }
                    ui.label(RichText::new(&p.path).color(tk.text_dim).small());
                    ui.end_row();
                }
            });
            if shown.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("{} {}", tr("No plugins. Drop package files into"), folder().display())).color(tk.text_dim));
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr("Loaded plugins add their graphs to the library (Plugins category); adding one copies the graph into your package."))
                .small()
                .color(tk.text_dim),
        );
    });
    if let Some((path, on)) = toggle {
        app.ui.plugins_loaded.retain(|p| *p != path);
        if on {
            app.ui.plugins_loaded.push(path);
        }
    }
    match action {
        Some("refresh") => app.plugins.refresh(),
        #[cfg(not(target_arch = "wasm32"))]
        Some("install") => install(app),
        #[cfg(not(target_arch = "wasm32"))]
        Some("browse") => browse(app),
        _ => {}
    }
    if !open {
        app.ui.windows.plugin_manager = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_packages_reports_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("mc-plugin-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("plugin.json"), "{\"name\": \"Test\", \"version\": \"1.0\"}");
        let p = read_plugin(&dir);
        assert_eq!(p.name, "Test");
        assert_eq!(p.version, "1.0");
        assert!(p.error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_garbage_manifest_keeps_the_folder_name() {
        let dir = std::env::temp_dir().join(format!("mc-plugin-garbage-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("plugin.json"), "{not json");
        let _ = std::fs::write(dir.join("broken.sbs"), "<xml");
        let p = read_plugin(&dir);
        assert_eq!(Some(p.name.as_str()), dir.file_name().and_then(|s| s.to_str()));
        assert!(p.error.is_some(), "a broken package is reported");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
