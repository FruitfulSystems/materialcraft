//! Tools and Windows menu dialogs: the engine switch, the dependency manager and the log export.

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as icon;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// The engines the cook can run on: worker thread counts (0 = every core).
fn engines() -> Vec<(usize, String)> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut out = vec![(0, format!("{} ({cores} {})", tr("CPU, all cores"), tr("threads")))];
    if cores > 2 {
        out.push((cores / 2, format!("{} ({} {})", tr("CPU, half the cores"), cores / 2, tr("threads"))));
    }
    out.push((1, tr("CPU, one thread (leaves the machine responsive)").to_string()));
    out
}

/// The engine's name for the status bar.
pub fn engine_label(app: &MaterialCraftApp) -> String {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let n = if app.cooker.threads == 0 { cores } else { app.cooker.threads.min(cores) };
    if app.cooker.suspended { format!("CPU ({n} {}, {})", tr("threads"), tr("suspended")) } else { format!("CPU ({n} {})", tr("threads")) }
}

/// Tools ▸ Switch engine (F9): pick how many threads the cook may use.
pub fn switch_engine(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    let mut chosen: Option<usize> = None;
    crate::windows::modal(ctx, "switch-engine", tr("Switch engine"), &mut open, vec2(420.0, 200.0), |ui| {
        ui.label(tr("Every graph computes on the CPU through the content-addressed cache. Choose how much of the machine it may use:"));
        ui.add_space(6.0);
        for (threads, label) in engines() {
            if ui.radio(app.cooker.threads == threads, label).clicked() {
                chosen = Some(threads);
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new(tr("The next compute uses the new setting; a running one finishes first.")).small().color(Tokens::get(ui.ctx()).text_dim));
    });
    if let Some(t) = chosen {
        app.cooker.threads = t;
        app.ui.prefs.cook_threads = t;
        app.session.dirty = true;
        open = false;
    }
    if !open {
        app.ui.windows.switch_engine = false;
    }
}

/// Everything the open package depends on: its bitmap resources (embedded or linked to a file)
/// and the graphs its graph instances point at.
pub fn dependencies(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    let mut relink: Option<usize> = None;
    let mut reload = false;
    crate::windows::modal(ctx, "dependencies", tr("Dependency Manager"), &mut open, vec2(620.0, 380.0), |ui| {
        let tk = Tokens::get(ui.ctx());
        ui.label(RichText::new(tr("RESOURCES")).small().color(tk.text_dim));
        if app.session.pkg.resources.is_empty() {
            ui.label(RichText::new(tr("No bitmap resources: Tools > Import bitmap... embeds one, Link bitmap... keeps it on disk.")).color(tk.text_dim));
        }
        egui::Grid::new("deps-resources").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(tr("Name")).strong());
            ui.label(RichText::new(tr("Source")).strong());
            ui.label(RichText::new(tr("Size")).strong());
            ui.label("");
            ui.end_row();
            for (i, r) in app.session.pkg.resources.iter().enumerate() {
                ui.label(&r.name);
                match &r.link {
                    Some(path) => {
                        let exists = std::path::Path::new(path).exists();
                        let text = RichText::new(format!("{} {path}", icon::LINK));
                        ui.label(if exists { text } else { text.color(tk.danger) }).on_hover_text(if exists {
                            tr("Linked file")
                        } else {
                            tr("Linked file not found")
                        });
                    }
                    None => {
                        ui.label(RichText::new(format!("{} {}", icon::PACKAGE, tr("embedded in the package"))).color(tk.text_dim));
                    }
                }
                ui.label(format!("{} KB", r.bytes.as_slice().len() / 1024));
                ui.horizontal(|ui| {
                    if r.link.is_some() && ui.small_button(tr("Relink...")).clicked() {
                        relink = Some(i);
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new(tr("GRAPH INSTANCES")).small().color(tk.text_dim));
        let mut rows: Vec<(String, String, bool)> = Vec::new();
        for g in &app.session.pkg.graphs {
            for n in g.nodes.iter().filter(|n| n.def == "graph_instance") {
                let target = match n.params.get("graph") {
                    Some(materialcraft_engine::core::Value::Text(t)) => t.clone(),
                    _ => String::new(),
                };
                let found = app.session.pkg.graphs.iter().any(|x| x.name == target);
                rows.push((g.name.clone(), target, found));
            }
        }
        if rows.is_empty() {
            ui.label(RichText::new(tr("No graph instances: every graph of this package stands on its own.")).color(tk.text_dim));
        } else {
            egui::Grid::new("deps-graphs").num_columns(3).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new(tr("Graph")).strong());
                ui.label(RichText::new(tr("Uses")).strong());
                ui.label(RichText::new(tr("State")).strong());
                ui.end_row();
                for (from, to, found) in rows {
                    ui.label(from);
                    ui.label(if to.is_empty() { tr("(not set)").to_string() } else { to });
                    if found {
                        ui.label(RichText::new(tr("found")).color(tk.text_dim));
                    } else {
                        ui.label(RichText::new(tr("missing")).color(tk.danger));
                    }
                    ui.end_row();
                }
            });
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(tr("Reload linked files")).clicked() {
                reload = true;
            }
        });
    });
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(i) = relink
        && let Some(p) = rfd::FileDialog::new().add_filter("Images", materialcraft_engine::formats::IMAGE_EXTENSIONS).pick_file()
        && let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone())
    {
        app.run("file.linkBitmap", serde_json::json!({"path": p.display().to_string(), "name": name}));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = relink;
    if reload {
        crate::menus::invoke(app, ctx, "ui.reloadResources");
    }
    if !open {
        app.ui.windows.dependencies = false;
    }
}

/// Import graph: which graph of the picked package to copy into this one.
pub fn import_pick(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let Some((path, graphs)) = app.import_pick.clone() else { return };
    let mut open = true;
    let mut chosen: Option<String> = None;
    crate::windows::modal(ctx, "import-graph", tr("Import graph"), &mut open, vec2(420.0, 260.0), |ui| {
        ui.label(RichText::new(&path).small().color(Tokens::get(ui.ctx()).text_dim));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for g in &graphs {
                if ui.button(format!("{} {g}", icon::TREE_STRUCTURE)).clicked() {
                    chosen = Some(g.clone());
                }
            }
        });
    });
    if let Some(g) = chosen {
        if let Some(r) = app.run("file.importGraph", serde_json::json!({"path": path, "graph": g})) {
            app.toast(format!("{} {}", tr("Imported graph"), r["name"].as_str().unwrap_or_default()), false);
        }
        open = false;
    }
    if !open {
        app.import_pick = None;
    }
}

/// Help ▸ Export log: the session's log as a text file.
pub fn export_log(app: &mut MaterialCraftApp) {
    let text = app.session.log.join("\n");
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(p) = rfd::FileDialog::new().set_file_name("materialcraft-log.txt").add_filter("Text", &["txt"]).save_file() else { return };
        match std::fs::write(&p, text) {
            Ok(()) => app.toast(format!("{} {}", tr("Log saved to"), p.display()), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Could not save the log")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        match crate::web_files::download("materialcraft-log.txt", text.as_bytes()) {
            Ok(()) => app.toast(tr("Log downloaded"), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Could not save the log")), true),
        }
    }
}

/// A small heading row used by the dialogs (title left, hint right).
pub fn heading(ui: &mut Ui, title: &str) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    ui.label(RichText::new(title).small().strong().color(tk.text_dim));
}
