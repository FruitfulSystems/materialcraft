//! What the explorer's menus do beyond running a command: copy and paste whole graphs, compute
//! the outputs again, reveal the package file, relink a bitmap

use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// A graph on the explorer's clipboard (Copy / Paste a graph)
#[derive(Clone, Debug, Default)]
pub struct GraphClipboard(pub Option<serde_json::Value>);

/// Rename-in-place state: the graph index and the text being edited
#[derive(Clone, Debug, Default)]
pub struct Rename(pub Option<(usize, String)>);

/// Copy a graph as JSON: to the explorer clipboard, and as text to the system clipboard
pub(super) fn copy_graph(app: &mut MaterialCraftApp, ctx: &egui::Context, i: usize) {
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

pub(super) fn paste_graph(app: &mut MaterialCraftApp) {
    let Some(v) = app.explorer_clipboard.0.clone() else { return };
    if let Some(r) = app.run("graph.add", json!({"graph": v})) {
        app.toast(format!("{} {}", tr("Pasted graph"), r["name"].as_str().unwrap_or_default()), false);
    }
}

/// Compute the open graph's outputs now: the cache is kept, the cook restarts (also lifts a Cancel)
pub(super) fn compute(app: &mut MaterialCraftApp) {
    app.cooker.hold = false;
    app.session.dirty = true;
}

/// Show the package file in the system's file browser
pub(super) fn reveal(app: &mut MaterialCraftApp) {
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

/// Pick a new file for a linked resource
pub(super) fn relink(app: &mut MaterialCraftApp, i: usize) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(name) = app.session.pkg.resources.get(i).map(|r| r.name.clone())
        && let Some(p) = crate::windows::dialog(app).add_filter("Images", materialcraft_engine::formats::IMAGE_EXTENSIONS).pick_file()
    {
        crate::windows::remember_dir(app, &p);
        app.run("file.linkBitmap", json!({"path": p.display().to_string(), "name": name}));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = i;
        app.toast(tr("The browser has no files to link to"), true);
    }
}
