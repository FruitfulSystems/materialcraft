use std::sync::{Mutex, PoisonError};

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// What a picked file is for
#[derive(Clone, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum Pick {
    Package,
    Bitmap,
    Resource { document: u64, vector: bool, folder: String },
    Mesh,
}

/// Files picked in the browser, waiting for the next frame: (file name, bytes, purpose)
static INBOX: Mutex<Vec<(String, Vec<u8>, Pick)>> = Mutex::new(Vec::new());

/// Load files that arrived since the last frame. Call once per frame
pub fn poll(app: &mut MaterialCraftApp) {
    let picked = std::mem::take(&mut *INBOX.lock().unwrap_or_else(PoisonError::into_inner));
    for (name, bytes, pick) in picked {
        if let Pick::Resource { document, vector, folder } = pick {
            if let Err(error) = materialcraft_engine::file_cmds::validate_resource_extension(&name, vector) {
                app.toast(error.to_string(), true);
            } else {
                import_resource_bytes(app, document, &name, bytes, &folder);
            }
        } else if pick == Pick::Mesh {
            mesh(app, &name, &bytes);
        } else {
            load(app, &name, bytes, pick == Pick::Bitmap);
        }
    }
}

fn import_resource_bytes(app: &mut MaterialCraftApp, document: u64, name: &str, bytes: Vec<u8>, folder: &str) {
    // A browser picker can finish after the user activates or closes its originating package
    if app.run("package.activate", serde_json::json!({"document": document})).is_none() {
        return;
    }

    match materialcraft_engine::file_cmds::add_resource_into(&mut app.session, name, bytes, None, folder) {
        Ok(name) => app.toast(format!("{} {name}", tr("Imported")), false),
        Err(error) => app.toast(error.to_string(), true),
    }
}

/// Show a mesh file's bytes in the 3D view
fn mesh(app: &mut MaterialCraftApp, name: &str, bytes: &[u8]) {
    match app.session.load_mesh_bytes(name, bytes) {
        Ok(()) => app.toast(format!("{} {name}", tr("Showing")), false),
        Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't load")), true),
    }
}

/// 3D view ▸ Scene ▸ Load mesh: a native dialog on the desktop, the file picker in the browser
pub fn load_mesh(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        if let Some(p) = rfd::FileDialog::new().add_filter("Meshes", materialcraft_engine::formats::mesh::MESH_EXTENSIONS).pick_file() {
            app.run("view.3d.loadMesh", serde_json::json!({"path": p.display().to_string()}));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        web::pick_mesh(ctx);
    }
}

/// Open a package or import an image (by extension unless `bitmap` forces an image)
pub fn load(app: &mut MaterialCraftApp, name: &str, bytes: Vec<u8>, bitmap: bool) {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if materialcraft_engine::formats::mesh::MESH_EXTENSIONS.contains(&ext.as_str()) {
        mesh(app, name, &bytes);
        return;
    }
    if bitmap || materialcraft_engine::formats::IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        crate::graph::import_bitmap_bytes(app, name, bytes);
        return;
    }
    if app.ui.windows.unsaved.is_some() {
        return;
    }
    load_now(app, name, &bytes);
}

pub(crate) fn load_now(app: &mut MaterialCraftApp, name: &str, bytes: &[u8]) {
    crate::properties::GraphPropertyDraft::commit(app);

    // Browser SBS downloads contain the same directory layout in a transport ZIP
    let path = name.strip_suffix("_sbs.zip").map_or_else(|| name.to_string(), |stem| format!("{stem}.sbs"));
    match materialcraft_engine::file_cmds::open_bytes(&mut app.session, bytes, Some(path)) {
        Ok(()) => {
            app.close_subgraph_after("file.open");
            app.ui.frame_all = true;
            app.toast(format!("{} {name}", tr("Opened")), false);
            crate::windows::after_open(app);
        }
        Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't open")), true),
    }
}

/// A file dropped on the window: a package opens, an image becomes a Bitmap node (in the browser the bytes arrive on a later frame)
pub fn dropped(app: &mut MaterialCraftApp, ctx: &egui::Context, file: &egui::DroppedFileHandle) {
    let name = file.path().file_name().and_then(|n| n.to_str()).unwrap_or("dropped").to_string();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let lower = name.to_ascii_lowercase();
        if (lower.ends_with(".sbs") || lower.ends_with(".mcraft")) && file.path().is_file() {
            // Open from the path so an SBS package finds the bitmaps kept next to it
            crate::windows::open_path(app, &file.path().display().to_string());
            return;
        }

        match file.bytes() {
            Ok(bytes) => load(app, &name, bytes, false),
            Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't open")), true),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        let (file, ctx) = (file.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => INBOX.lock().unwrap_or_else(PoisonError::into_inner).push((name, bytes, Pick::Package)),
                Err(e) => log::warn!("couldn't read dropped file {name}: {e}"),
            }
            ctx.request_repaint();
        });
    }
}

/// A zip of `(name, bytes)` files (export downloads several maps at once)
pub fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write as _;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            z.write_all(bytes).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::sync::PoisonError;

    use wasm_bindgen::JsCast as _;

    use super::{INBOX, Pick, zip_files};
    use crate::MaterialCraftApp;
    use crate::i18n::tr;

    fn pick(ctx: &egui::Context, filter: &'static str, exts: &'static [&'static str], kind: Pick) {
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(file) = rfd::AsyncFileDialog::new().add_filter(filter, exts).pick_file().await {
                let bytes = file.read().await;
                INBOX.lock().unwrap_or_else(PoisonError::into_inner).push((file.file_name(), bytes, kind));
            }
            ctx.request_repaint();
        });
    }

    /// File ▸ Open: a package, picked in the browser
    pub fn open(ctx: &egui::Context) {
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut dialog = rfd::AsyncFileDialog::new();
            for (label, extensions) in crate::windows::package_filters(true, false) {
                dialog = dialog.add_filter(label, extensions);
            }

            // Browser SBS downloads wrap the XML and adjacent resources in a transport ZIP
            dialog = dialog.add_filter("SBS Package Download (*.zip)", &["zip"]);
            if let Some(file) = dialog.pick_file().await {
                let bytes = file.read().await;
                INBOX.lock().unwrap_or_else(PoisonError::into_inner).push((file.file_name(), bytes, Pick::Package));
            }
            ctx.request_repaint();
        });
    }

    /// 3D view ▸ Scene ▸ Load mesh
    pub fn pick_mesh(ctx: &egui::Context) {
        pick(ctx, "Meshes", materialcraft_engine::formats::mesh::MESH_EXTENSIONS, Pick::Mesh);
    }

    /// File ▸ Import bitmap
    pub fn import_bitmap(ctx: &egui::Context) {
        pick_image(ctx, Pick::Bitmap, false);
    }

    pub fn import_resource(ctx: &egui::Context, document: u64, vector: bool, folder: &str) {
        pick_image(ctx, Pick::Resource { document, vector, folder: folder.into() }, vector);
    }

    fn pick_image(ctx: &egui::Context, kind: Pick, vector: bool) {
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut dialog = rfd::AsyncFileDialog::new();
            for (label, extensions) in crate::windows::resource_filters(vector) {
                dialog = dialog.add_filter(label, extensions);
            }
            if let Some(file) = dialog.pick_file().await {
                let bytes = file.read().await;
                INBOX.lock().unwrap_or_else(PoisonError::into_inner).push((file.file_name(), bytes, kind));
            }
            ctx.request_repaint();
        });
    }

    /// File ▸ Save: download either the native archive or SBS with its resources.
    pub fn save(app: &mut MaterialCraftApp) {
        if let Some(path) = app.session.path.clone() {
            save_download(app, &path, false);
        } else {
            save_as(app, false);
        }
    }

    pub fn save_copy(app: &mut MaterialCraftApp) {
        save_as(app, true);
    }

    pub fn save_as(app: &mut MaterialCraftApp, copy: bool) {
        let archive = app.session.path.as_deref().is_some_and(materialcraft_engine::file_cmds::is_mcraft);
        app.ui.windows.package_save = Some((app.session.document_id, app.session.pkg.name.clone(), archive, copy));
    }

    pub fn save_dialog(app: &mut MaterialCraftApp, ctx: &egui::Context) {
        let Some((document, mut name, mut archive, copy)) = app.ui.windows.package_save.clone() else { return };
        let mut open = true;
        let mut accept = false;
        egui::Window::new(tr("Save package")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            ui.label(tr("Name"));
            ui.text_edit_singleline(&mut name);
            ui.radio_value(&mut archive, false, tr("SBS XML Package (*.sbs)"));
            ui.radio_value(&mut archive, true, tr("MaterialCraft Project (*.mcraft)"));
            if !archive {
                ui.label(tr("Downloads the SBS and resources together in a ZIP."));
            }
            accept = ui.button(tr("Download")).clicked();
        });
        app.ui.windows.package_save = open.then_some((document, name.clone(), archive, copy));
        if accept {
            app.ui.windows.package_save = None;
            if app.run("package.activate", serde_json::json!({"document": document})).is_some() {
                let ext = if archive { "mcraft" } else { "sbs" };
                save_download(app, &format!("{}.{ext}", materialcraft_engine::formats::safe_file_name(&name)), copy);
            }
        }
    }

    fn save_download(app: &mut MaterialCraftApp, path: &str, copy: bool) {
        crate::properties::GraphPropertyDraft::commit(app);
        let archive = materialcraft_engine::file_cmds::is_mcraft(path);
        let name = if archive { path.to_string() } else { format!("{}_sbs.zip", path.strip_suffix(".sbs").unwrap_or(path)) };
        let result = materialcraft_engine::file_cmds::save_files_at(&app.session, path)
            .map_err(|e| e.to_string())
            .and_then(|files| files.zip().map_err(|e| e.to_string()))
            .and_then(|bytes| download(&name, &bytes));
        match result {
            Ok(()) => {
                if !copy {
                    app.session.path = Some(path.to_string());
                    app.session.modified = false;
                }
                app.toast(format!("{} {name}", tr("Downloaded")), false);
            }
            Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't save")), true),
        }
    }

    /// File ▸ Export material: download the open graph as `<graph>.mcmat`
    pub fn export_material(app: &mut MaterialCraftApp) {
        use materialcraft_engine::material_cmds::{MaterialOptions, material_bytes};
        let name = format!("{}.mcmat", materialcraft_engine::formats::safe_file_name(&app.session.graph().map(|g| g.name.clone()).unwrap_or_default()));
        match material_bytes(&app.session, app.session.graph, &MaterialOptions::default()).map_err(|e| e.to_string()).and_then(|b| download(&name, &b)) {
            Ok(()) => app.toast(format!("{} {name}", tr("Downloaded")), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Couldn't export")), true),
        }
    }

    /// Export outputs: one file downloads as is, several as `<graph>_outputs.zip`
    pub fn export(app: &mut MaterialCraftApp, params: &serde_json::Value) {
        let result = materialcraft_engine::file_cmds::export_params(&app.session, params)
            .and_then(|(gi, ext, pattern, depth, size)| materialcraft_engine::file_cmds::export_outputs(&app.session, gi, &ext, &pattern, depth, size))
            .map_err(|e| e.to_string())
            .and_then(|files| {
                let files: Vec<(String, Vec<u8>)> = files.into_iter().map(|f| (f.file, f.bytes)).collect();
                match files.as_slice() {
                    [(name, bytes)] => download(name, bytes).map(|()| name.clone()),
                    _ => {
                        let name = format!("{}_outputs.zip", app.session.graph().map(|g| g.name.clone()).unwrap_or_default());
                        download(&name, &zip_files(&files)?).map(|()| name)
                    }
                }
            });
        match result {
            Ok(name) => app.toast(format!("{} {name}", tr("Downloaded")), false),
            Err(e) => app.toast(format!("{}: {e}", tr("Couldn't export")), true),
        }
    }

    /// Start a browser download of `bytes` named `name` (Blob + a temporary `<a download>`)
    pub fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
        let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(if name.ends_with(".zip") { "application/zip" } else { "application/octet-stream" });
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
        let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
        a.set_href(&url);
        a.set_download(name);
        a.style().set_property("display", "none").map_err(js)?;
        document.body().ok_or("no body")?.append_child(&a).map_err(js)?;
        a.click();
        a.remove();
        // Revoke once the click has been dispatched; the download keeps its own reference
        let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
            web_sys::Url::revoke_object_url(&url).ok();
        });
        window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(modified: bool) -> MaterialCraftApp {
        let mut s = materialcraft_engine::sample_session();
        s.modified = modified;
        MaterialCraftApp::headless(s, crate::state::UiState::default())
    }

    #[test]
    fn an_invalid_package_reports_an_error_without_discarding_unsaved_work() {
        let mut app = app(true);
        let before = app.session.pkg.name.clone();
        load(&mut app, "other.sbs", b"not a package".to_vec(), false);
        assert!(app.ui.windows.unsaved.is_none());
        assert!(app.pending_load.is_none());
        assert_eq!(app.session.pkg.name, before);
        assert!(app.session.modified);
        assert!(!app.toasts.is_empty());
    }

    #[test]
    fn an_image_import_is_not_a_replacement_and_never_asks() {
        let mut app = app(true);
        load(&mut app, "x.png", b"junk".to_vec(), true);
        assert!(app.ui.windows.unsaved.is_none());
    }

    #[test]
    fn zip_round_trips() {
        let files = vec![("a.png".to_string(), b"one".to_vec()), ("b.png".to_string(), vec![7u8; 4096])];
        let bytes = zip_files(&files).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        for (name, data) in &files {
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut zip.by_name(name).unwrap(), &mut out).unwrap();
            assert_eq!(&out, data);
        }
    }

    #[test]
    fn delayed_resource_import_targets_its_original_package_without_a_node() {
        let mut app = app(false);
        let original = app.session.document_id;
        app.run("file.newPackage", serde_json::json!({"name": "Other"})).unwrap();
        let other = app.session.document_id;
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2"/></svg>"#;
        import_resource_bytes(&mut app, original, "shape.svg", svg.to_vec(), "Resources");

        assert_eq!(app.session.document_id, original);
        assert_eq!(app.session.pkg.resources.len(), 1);
        assert!(app.session.pkg.graphs.iter().all(|graph| graph.nodes.iter().all(|node| node.def != "svg")));

        app.run("package.close", serde_json::json!({"discard": true})).unwrap();

        assert_eq!(app.session.document_id, other);

        import_resource_bytes(&mut app, original, "shape.svg", svg.to_vec(), "Resources");

        assert!(app.session.pkg.resources.is_empty(), "a closed picker target must not import into a different package");
    }
}
