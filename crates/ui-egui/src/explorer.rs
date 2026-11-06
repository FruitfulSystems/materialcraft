//! The explorer: the open package, its graphs and its bitmap resources, each with a context menu.

use egui::{RichText, Sense, Ui, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// What a context menu asked for; carried out after the tree is drawn.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Menu(&'static str),
    Graph(&'static str, usize),
    Resource(&'static str, usize),
}

/// A graph on the explorer's clipboard (Copy / Paste a graph).
#[derive(Clone, Debug, Default)]
pub struct GraphClipboard(pub Option<serde_json::Value>);

/// Rename-in-place state: the graph index and the text being edited.
#[derive(Clone, Debug, Default)]
pub struct Rename(pub Option<(usize, String)>);

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if crate::widgets::icon_button(ui, icon::FILE_PLUS, tr("New graph..."), false, 22.0).clicked() {
            app.ui.windows.new_graph = true;
        }
        if crate::widgets::icon_button(ui, icon::FOLDER_OPEN, tr("Open package..."), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.open");
        }
        if crate::widgets::icon_button(ui, icon::FLOPPY_DISK, tr("Save package"), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.save");
        }
        if crate::widgets::icon_button(ui, icon::EXPORT, tr("Export outputs..."), false, 22.0).clicked() {
            app.ui.windows.export = true;
        }
        if crate::widgets::icon_button(ui, icon::IMAGE, tr("Import bitmap..."), false, 22.0).clicked() {
            crate::menus::invoke(app, ui.ctx(), "file.importBitmap");
        }
    });
    ui.add_space(2.0);
    let mut action: Option<Action> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let name = format!("{}{}", app.session.pkg.name, if app.session.modified { "*" } else { "" });
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        crate::widgets::name(&resp, egui::Role::TreeItem, &app.session.pkg.name);
        if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, tk.row_hover);
        }
        ui.painter().text(
            r.left_center() + vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!("{}  {name}", icon::PACKAGE),
            egui::FontId::proportional(12.5),
            tk.text_strong,
        );
        if let Some(path) = &app.session.path {
            resp.clone().on_hover_text(path);
        }
        resp.context_menu(|ui| package_menu(app, ui, &mut action));

        let focused_tree = ui.ui_contains_pointer();
        for i in 0..app.session.pkg.graphs.len() {
            graph_row(app, ui, i, &mut action);
        }
        // Keyboard: Return opens, Del removes, F2 renames the open graph while the pointer is here.
        if focused_tree && app.explorer_rename.0.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            let gi = app.session.graph;
            ui.input(|i| {
                if i.key_pressed(egui::Key::F2) {
                    action = Some(Action::Graph("rename", gi));
                } else if i.key_pressed(egui::Key::Delete) {
                    action = Some(Action::Graph("delete", gi));
                }
            });
        }
        if !app.session.pkg.resources.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Resources")).color(tk.text_dim).size(11.0));
            for i in 0..app.session.pkg.resources.len() {
                let Some(r) = app.session.pkg.resources.get(i) else { continue };
                let linked = r.link.is_some();
                let text = format!("{}  {}  ({} KB)", if linked { icon::LINK } else { icon::IMAGE }, r.name, r.bytes.len() / 1024);
                let hover = match &r.link {
                    Some(l) => format!("{} {l}", tr("Linked:")),
                    None => tr("Embedded in the package").to_string(),
                };
                let resp = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text(hover);
                resp.context_menu(|ui| {
                    if linked && ui.button(tr("Relink...")).clicked() {
                        action = Some(Action::Resource("relink", i));
                        ui.close();
                    }
                    if ui.button(tr("Add a Bitmap node")).clicked() {
                        action = Some(Action::Resource("node", i));
                        ui.close();
                    }
                    if ui.button(tr("Remove")).clicked() {
                        action = Some(Action::Resource("delete", i));
                        ui.close();
                    }
                });
            }
        }
    });
    if let Some(a) = action {
        perform(app, ui.ctx(), a);
    }
}

fn graph_row(app: &mut MaterialCraftApp, ui: &mut Ui, i: usize, action: &mut Option<Action>) {
    let tk = Tokens::get(ui.ctx());
    let Some(g) = app.session.pkg.graphs.get(i) else { return };
    let gname = g.name.clone();
    let on = i == app.session.graph;
    // Rename in place.
    if let Some((ri, text)) = &mut app.explorer_rename.0
        && *ri == i
    {
        let resp = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 24.0));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        let enter = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
        let escape = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
        if resp.lost_focus() || enter {
            let new = text.trim().to_string();
            app.explorer_rename.0 = None;
            if !new.is_empty() && new != gname && !escape {
                app.run("graph.rename", json!({"graph": i, "name": new}));
            }
        } else if escape {
            app.explorer_rename.0 = None;
        }
        return;
    }
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    crate::widgets::name(&resp, egui::Role::TreeItem, &gname);
    if on {
        ui.painter().rect_filled(r, 4.0, tk.row_selected);
        ui.painter().rect_stroke(r, 4.0, egui::Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, tk.row_hover);
    }
    ui.painter().text(
        r.left_center() + vec2(18.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{}  {}", icon::TREE_STRUCTURE, gname),
        egui::FontId::proportional(12.0),
        if on { tk.text_strong } else { tk.text },
    );
    if resp.clicked() {
        *action = Some(Action::Graph("open", i));
    }
    resp.context_menu(|ui| {
        let can_paste = app.explorer_clipboard.0.is_some();
        for (label, a, key, enabled) in [
            ("Open", "open", "Return", true),
            ("Copy", "copy", "", true),
            ("Paste", "paste", "", can_paste),
            ("Remove", "delete", "Del", true),
            ("Rename", "rename", "F2", true),
            ("View Outputs In 3D View", "view3d", "", true),
            ("Compute Outputs", "compute", "", true),
            ("Export outputs...", "export", "", true),
        ] {
            if crate::menus::menu_row(ui, tr(label), key, enabled).clicked() {
                *action = Some(Action::Graph(a, i));
                ui.close();
            }
        }
    });
}

fn package_menu(app: &mut MaterialCraftApp, ui: &mut Ui, action: &mut Option<Action>) {
    let mut pick = |ui: &mut Ui, label: &str, id: &'static str, key: &str, enabled: bool| {
        if crate::menus::menu_row(ui, tr(label), key, enabled).clicked() {
            *action = Some(Action::Menu(id));
            ui.close();
        }
    };
    ui.menu_button(tr("New"), |ui| {
        pick(ui, "Graph...", "ui.newGraph", "Ctrl+N", true);
        pick(ui, "Function graph...", "ui.newFunctionGraph", "", false);
    });
    ui.menu_button(tr("Import"), |ui| {
        pick(ui, "Bitmap...", "file.importBitmap", "", true);
        pick(ui, "Graph from a package...", "file.importGraph", "", true);
    });
    ui.menu_button(tr("Link"), |ui| {
        pick(ui, "Bitmap...", "file.linkBitmap", "", true);
    });
    pick(ui, "Reload", "ui.reloadResources", "", true);
    ui.separator();
    pick(ui, "Save", "file.save", "Ctrl+S", true);
    pick(ui, "Save as...", "file.saveAs", "Ctrl+Shift+S", true);
    pick(ui, "Save a copy as...", "file.saveCopy", "", true);
    pick(ui, "Export with dependencies...", "file.exportWithDependencies", "", true);
    ui.menu_button(tr("Send to"), |ui| {
        pick(ui, "TextureCraft", "file.sendToTextureCraft", "", true);
    });
    ui.separator();
    pick(ui, "Copy", "explorer.copy", "", true);
    pick(ui, "Paste", "explorer.paste", "", app.explorer_clipboard.0.is_some());
    ui.separator();
    pick(ui, "Close package", "ui.closeAll", "", true);
    pick(ui, "Compute Outputs", "explorer.compute", "", true);
    pick(ui, "Show in Explorer...", "explorer.reveal", "", app.session.path.is_some());
    ui.separator();
    pick(ui, "Dependency Manager", "ui.view.dependencies", "", true);
}

fn perform(app: &mut MaterialCraftApp, ctx: &egui::Context, action: Action) {
    match action {
        Action::Menu("explorer.copy") => copy_graph(app, ctx, app.session.graph),
        Action::Menu("explorer.paste") => paste_graph(app),
        Action::Menu("explorer.compute") => compute(app),
        Action::Menu("explorer.reveal") => reveal(app),
        Action::Menu(id) => crate::menus::invoke(app, ctx, id),
        Action::Graph("open", i) => {
            app.run("graph.open", json!({"graph": i}));
        }
        Action::Graph("copy", i) => copy_graph(app, ctx, i),
        Action::Graph("paste", _) => paste_graph(app),
        Action::Graph("delete", i) => {
            app.run("graph.delete", json!({"graph": i}));
        }
        Action::Graph("rename", i) => {
            if let Some(g) = app.session.pkg.graphs.get(i) {
                app.explorer_rename.0 = Some((i, g.name.clone()));
            }
        }
        Action::Graph("view3d", i) => {
            if app.run("graph.open", json!({"graph": i})).is_some() {
                app.ui.dock.focus(crate::docking::Panel::View3d);
                compute(app);
            }
        }
        Action::Graph("compute", i) => {
            if app.run("graph.open", json!({"graph": i})).is_some() {
                compute(app);
            }
        }
        Action::Graph("export", i) => {
            // The export window works on the open graph.
            if app.run("graph.open", json!({"graph": i})).is_some() {
                app.ui.windows.export = true;
            }
        }
        Action::Resource("delete", i) => {
            if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone()) {
                app.run("file.deleteResource", json!({"name": name}));
            }
        }
        Action::Resource("node", i) => {
            if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone()) {
                app.run("graph.node.add", json!({"def": "bitmap", "params": {"resource": name}}));
            }
        }
        Action::Resource("relink", i) => relink(app, i),
        Action::Graph(..) | Action::Resource(..) => {}
    }
}

/// Copy a graph as JSON: to the explorer clipboard, and as text to the system clipboard.
fn copy_graph(app: &mut MaterialCraftApp, ctx: &egui::Context, i: usize) {
    let Some(g) = app.session.pkg.graphs.get(i) else { return };
    match serde_json::to_value(g) {
        Ok(v) => {
            if let Ok(text) = serde_json::to_string(&v) {
                ctx.copy_text(text);
            }
            app.explorer_clipboard.0 = Some(v);
            app.toast(format!("{} {}", tr("Copied graph"), g.name), false);
        }
        Err(e) => app.toast(format!("{}: {e}", tr("Could not copy the graph")), true),
    }
}

fn paste_graph(app: &mut MaterialCraftApp) {
    let Some(v) = app.explorer_clipboard.0.clone() else { return };
    if let Some(r) = app.run("graph.add", json!({"graph": v})) {
        app.toast(format!("{} {}", tr("Pasted graph"), r["name"].as_str().unwrap_or_default()), false);
    }
}

/// Compute the open graph's outputs now: the cache is kept, the cook restarts (also lifts a Cancel).
fn compute(app: &mut MaterialCraftApp) {
    app.cooker.hold = false;
    app.session.dirty = true;
}

/// Show the package file in the system's file browser.
fn reveal(app: &mut MaterialCraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = app.session.path.clone() else { return };
        let folder = std::path::Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let result = if cfg!(target_os = "windows") {
            std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn()
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg("-R").arg(&path).spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&folder).spawn()
        };
        if let Err(e) = result {
            app.toast(format!("{} {} ({e})", tr("Package folder:"), folder.display()), true);
        }
    }
    #[cfg(target_arch = "wasm32")]
    app.toast(tr("The browser keeps no package folder"), true);
}

fn relink(app: &mut MaterialCraftApp, i: usize) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone())
        && let Some(p) = rfd::FileDialog::new().add_filter("Images", materialcraft_engine::formats::IMAGE_EXTENSIONS).pick_file()
    {
        app.run("file.linkBitmap", json!({"path": p.display().to_string(), "name": name}));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = i;
        app.toast(tr("The browser has no files to link to"), true);
    }
}
