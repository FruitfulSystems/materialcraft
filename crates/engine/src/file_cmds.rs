pub use crate::Exported;
pub use crate::linked_resource::reader as linked_files;

use serde_json::{Value as J, json};

use materialcraft_core::{Package, Resource, ResourceBytes};
use materialcraft_formats::{Depth, encode, output_name};

use crate::commands::{CommandSpec, always, bad, has_graph, strp, u64p};
use crate::samples::{TEMPLATES, template};
use crate::{Result, Session, cmd};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "file.new",
            "Graph...",
            ["File", "New"],
            Some("Ctrl+N"),
            "{name?, folder?, kind? (material|function), template? (see graph.templates), size? (px, power of two)} adds a new graph to the package and opens it",
            always,
            new_graph
        ),
        cmd!("file.newPackage", "Empty package", ["File", "New"], None, "{name?} opens a new package alongside the existing packages", always, new_package),
        cmd!(
            "file.open",
            "Open...",
            ["File"],
            Some("Ctrl+O"),
            "{path} opens an SBS package; returns `warnings` and `path` (null when unsupported content requires Save As)",
            always,
            open
        ),
        cmd!("file.save", "Save", ["File"], Some("Ctrl+S"), "{path?} saves native SBS to the current file or `path`", always, save),
        cmd!("file.saveAs", "Save as...", ["File"], Some("Ctrl+Shift+S"), "{path} saves the package under a new name", always, save),
        cmd!(
            "file.export",
            "Export outputs...",
            ["File"],
            Some("Ctrl+Shift+E"),
            "{dir, format? (png|jpg|tga|tif|bmp|webp|exr|hdr), pattern? ($(graph)_$(identifier)), depth? (8|16|32), size? (px of the larger side, rounded down to a power of two, 16..8192; default the graph's size, keeping its proportions), graph?} cooks the graph at full size and writes every output",
            has_graph,
            export
        ),
        cmd!(
            "file.importBitmap",
            "Import bitmap...",
            ["File"],
            None,
            "{path, name?} embeds an image file as a resource; returns its name",
            always,
            import_bitmap
        ),
        cmd!("graph.templates", "Graph templates", [], None, "{} lists the templates for file.new", always, |_, _| Ok(json!(
            TEMPLATES.iter().map(|(id, l)| json!({"id": id, "label": l})).collect::<Vec<_>>()
        ))),
    ]
}

fn new_graph(session: &mut Session, parameters: &J) -> Result<J> {
    let kind = match strp(parameters, "kind").unwrap_or("material") {
        "material" => materialcraft_core::GraphKind::Material,
        "function" => materialcraft_core::GraphKind::Function,
        _ => return Err(bad("file.new", "kind must be material or function")),
    };
    let template_id = strp(parameters, "template").unwrap_or("pbr");
    let label = if kind == materialcraft_core::GraphKind::Function {
        "Function Graph"
    } else {
        TEMPLATES.iter().find(|entry| entry.0 == template_id).map_or("Graph", |entry| entry.1)
    };
    let size = u64p(parameters, "size").unwrap_or(1024).clamp(16, 8192);
    let log2 = 63 - size.leading_zeros();
    let base = strp(parameters, "name").map(str::to_string).unwrap_or_else(|| label.replace(' ', "_"));
    let folder = strp(parameters, "folder").unwrap_or("");
    crate::folder_path::require(&session.pkg, folder)?;
    let base = crate::folder_path::name(&base)?;

    session.edit("New graph", |session| {
        let name = crate::folder_path::unique(&session.pkg, folder, base);
        let mut graph = if kind == materialcraft_core::GraphKind::Function {
            let mut graph = materialcraft_core::Graph::new(&name);
            graph.function = Some(materialcraft_core::FunctionGraph::default());
            graph
        } else {
            template(template_id, &name, log2)?
        };
        graph.folder = folder.into();
        session.pkg.graphs.push(graph);
        session.graph = session.pkg.graphs.len() - 1;
        session.open_material_preview();
        session.selection.clear();
        session.annotation_selection = None;
        session.view2d.node = None;
        Ok(json!({"graph": session.graph, "name": name, "kind": kind}))
    })
}

fn new_package(s: &mut Session, p: &J) -> Result<J> {
    let mut pkg = Package::new(strp(p, "name").unwrap_or("Untitled package"));
    pkg.graphs.push(template("pbr", "New_graph", 10)?);
    s.open_package(pkg, None)?;
    Ok(json!({"name": s.pkg.name}))
}

/// Replace the active document explicitly, resetting its history and views
pub fn replace_package(s: &mut Session, pkg: Package, path: Option<String>) {
    s.pkg = pkg;
    s.clipboard = None;
    s.embedded_clipboard = None;
    s.clear_decoded();
    s.path = path;
    s.graph = 0;
    s.material_preview = crate::MaterialPreview::for_graph(&s.pkg, 0);
    s.clear_preview_results();
    s.selection.clear();
    s.annotation_selection = None;
    s.view2d.node = None;
    s.reset_history();
    s.modified = false;
    s.dirty = true;
    s.load_warnings.clear();
}

fn has_extension(path: &str, ext: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

pub fn package_path(path: &str) -> String {
    if has_extension(path, materialcraft_formats::sbs::SBS_EXT) { path.to_string() } else { format!("{path}.sbs") }
}

pub fn open_bytes(session: &mut Session, bytes: &[u8], name: Option<String>) -> Result<()> {
    open_bytes_linked(session, bytes, name, &|_| None)
}

pub fn open_bytes_linked(session: &mut Session, bytes: &[u8], name: Option<String>, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<()> {
    let name = name.map(|path| crate::document_path::normalize(&path));
    if let Some(name) = &name
        && let Some((id, ..)) = session.open_packages().into_iter().find(|entry| entry.2 == Some(name.as_str()))
    {
        return session.activate_document(id);
    }
    if materialcraft_formats::material::is_material(bytes) {
        let (manifest, source) = materialcraft_formats::material::read_material(bytes)?;
        session.open_package(materialcraft_formats::sbs::read_sbs(&source, "source.sbs", &|_| None)?.pkg, None)?;
        session.graph = session.pkg.graph_by_name(&manifest.graph).map_or(0, |(index, _)| index);
        session.open_material_preview();
        return Ok(());
    }

    let mut read = materialcraft_formats::sbs::read_sbs(bytes, name.as_deref().unwrap_or(""), linked)?;
    if let Some(path) = &name {
        crate::linked_files::normalize_references(&mut read.pkg, path);
    }
    read.pkg.dependencies = session.pkg.dependencies.clone();
    let link_warnings = crate::linked_files::load(&mut read.pkg);
    let path = if read.warnings.is_empty() { name } else { None };
    read.warnings.extend(link_warnings);
    session.open_package(read.pkg, path)?;
    for warning in &read.warnings {
        session.warn(warning.clone());
    }
    session.load_warnings = read.warnings;
    Ok(())
}

pub fn save_bytes(session: &Session) -> Result<Vec<u8>> {
    save_bytes_at(session, session.path.as_deref())
}

pub(crate) fn save_bytes_at(session: &Session, path: Option<&str>) -> Result<Vec<u8>> {
    let mut package = session.pkg.clone();
    if let Some(path) = path {
        for resource in &mut package.resources {
            if !resource.source.is_empty() {
                resource.source = crate::document_path::relative(path, &resource.source);
            }
        }

        for instance in package.graphs.iter_mut().flat_map(|graph| graph.instances_mut()) {
            if !instance.package.is_empty() {
                instance.package = crate::document_path::relative(path, &instance.package);
            }
        }
    }
    Ok(materialcraft_formats::sbs::write_sbs(&package)?)
}

/// Open the package file at `path` (native), with the bitmaps an SBS package keeps next to it
#[cfg(not(target_arch = "wasm32"))]
pub fn open_file(s: &mut Session, path: &str) -> Result<()> {
    if path.starts_with("sbs://") {
        let bytes = materialcraft_formats::sbs::library::read(path).map_err(|error| bad("file.open", error))?;
        return open_bytes_linked(s, &bytes, Some(path.into()), &linked_files(path));
    }

    let bytes = std::fs::read(path).map_err(|e| bad("file.open", format!("{path}: {e}")))?;
    open_bytes_linked(s, &bytes, Some(path.to_string()), &linked_files(path))
}

#[cfg(not(target_arch = "wasm32"))]
fn open(s: &mut Session, p: &J) -> Result<J> {
    let path = strp(p, "path").ok_or_else(|| bad("file.open", "`path` is required"))?;
    open_file(s, path)?;
    s.log(format!("Opened {path}"));
    Ok(json!({"name": s.pkg.name, "graphs": s.pkg.graphs.len(), "path": s.path, "warnings": s.load_warnings}))
}

#[cfg(target_arch = "wasm32")]
fn open(session: &mut Session, parameters: &J) -> Result<J> {
    let path = strp(parameters, "path")
        .filter(|path| path.starts_with("sbs://"))
        .ok_or_else(|| bad("file.open", "the browser opens local files with the file picker"))?;
    let bytes = materialcraft_formats::sbs::library::read(path).map_err(|error| bad("file.open", error))?;
    open_bytes_linked(session, &bytes, Some(path.into()), &linked_files(path))?;

    Ok(json!({"name": session.pkg.name, "graphs": session.pkg.graphs.len(), "path": session.path, "warnings": session.load_warnings}))
}

#[cfg(not(target_arch = "wasm32"))]
fn save(s: &mut Session, p: &J) -> Result<J> {
    let path = match strp(p, "path") {
        Some(path) => package_path(path),
        None => s.path.clone().ok_or_else(|| bad("file.save", "the package has no file yet (new, or opened from another program): give a `path`"))?,
    };

    let path = crate::document_path::normalize(&path);
    if path.contains("://") {
        return Err(bad("file.save", "library aliases cannot be overwritten; save to a local .sbs path"));
    }

    if s.open_packages().into_iter().any(|entry| entry.0 != s.document_id && entry.2 == Some(path.as_str())) {
        return Err(bad("file.save", "another open package already uses this file"));
    }
    let bytes = save_bytes_at(s, Some(&path))?;
    materialcraft_formats::write_atomic(std::path::Path::new(&path), &bytes)?;
    s.path = Some(path.clone());
    s.refresh_links();
    s.modified = false;
    s.log(format!("Saved {path}"));
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

#[cfg(target_arch = "wasm32")]
fn save(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.save", "the browser saves with a download"))
}

/// Cook graph `gi` at full size and encode every output (`pattern` names files)
pub fn export_outputs(s: &Session, gi: usize, ext: &str, pattern: &str, depth: Option<Depth>, size: Option<u32>) -> Result<Vec<Exported>> {
    let mut pkg = s.pkg.clone();
    if let Some(px) = size {
        let g = pkg.graph_mut(gi)?;
        // The larger side becomes `px`; the other keeps its proportion
        let target = 31 - px.clamp(16, 8192).leading_zeros();
        let bounded = g.size.map(|l| l.clamp(4, 13));
        let larger = bounded[0].max(bounded[1]);
        g.size = bounded.map(|l| (l + target).saturating_sub(larger).max(4));
    }
    let g = pkg.graph(gi)?;
    let dec = s.resource_decoder();
    let mut options = materialcraft_eval::Options::new(&dec);
    options.environment = s.environment;
    let cooked = materialcraft_eval::cook(&pkg, gi, &options, &s.cache);
    let mut out = Vec::new();
    let mut names = std::collections::HashSet::new();
    for (id, usage, ident, img) in materialcraft_eval::outputs(g, &cooked) {
        let Some(img) = img else { continue };
        let file = format!("{}.{}", output_name(pattern, &g.name, &ident, usage), ext.trim_start_matches('.'));
        if !names.insert(file.to_lowercase()) {
            return Err(bad("file.export", format!("duplicate export filename `{file}`: use unique output identifiers or a different filename pattern")));
        }
        let node = g.node(id)?;
        let depth = depth
            .unwrap_or_else(|| Depth::from_format(if node.base.format == materialcraft_core::OutputFormat::Inherit { g.format } else { node.base.format }));
        let bytes = encode(img, ext, depth, materialcraft_formats::material::srgb_usage(usage))?;
        out.push(Exported { file, bytes });
    }
    if out.is_empty() {
        return Err(bad("file.export", "the graph has no connected output nodes"));
    }
    Ok(out)
}

/// Export params: (graph index, extension, pattern, depth, size)
pub type ExportArgs = (usize, String, String, Option<Depth>, Option<u32>);

pub fn export_params(s: &Session, p: &J) -> Result<ExportArgs> {
    let gi = u64p(p, "graph").map_or(s.graph, |g| g as usize);
    s.pkg.graph(gi)?;
    let ext = strp(p, "format").unwrap_or("png").trim_start_matches('.').to_ascii_lowercase();
    if !materialcraft_formats::EXPORT_FORMATS.iter().any(|(e, _)| *e == ext) {
        return Err(bad("file.export", format!("unknown format `{ext}`")));
    }
    let pattern = strp(p, "pattern").unwrap_or("$(graph)_$(identifier)").to_string();
    let depth = match u64p(p, "depth") {
        None => None,
        Some(8) => Some(Depth::U8),
        Some(16) => Some(Depth::U16),
        Some(32) => Some(Depth::F32),
        Some(d) => return Err(bad("file.export", format!("depth must be 8, 16 or 32, not {d}"))),
    };

    let size = u64p(p, "size").filter(|&px| px > 0).map(|px| px.min(u64::from(u32::MAX)) as u32);
    Ok((gi, ext, pattern, depth, size))
}

#[cfg(not(target_arch = "wasm32"))]
fn export(s: &mut Session, p: &J) -> Result<J> {
    let dir = strp(p, "dir").ok_or_else(|| bad("file.export", "`dir` is required"))?;
    let (gi, ext, pattern, depth, size) = export_params(s, p)?;
    let files = export_outputs(s, gi, &ext, &pattern, depth, size)?;
    std::fs::create_dir_all(dir).map_err(|e| bad("file.export", format!("{dir}: {e}")))?;
    for f in &files {
        if std::path::Path::new(dir).join(&f.file).exists() {
            return Err(bad("file.export", format!("duplicate export filename `{}`: use unique output identifiers or a different filename pattern", f.file)));
        }
    }
    let mut written = Vec::new();
    for f in files {
        let path = std::path::Path::new(dir).join(&f.file);
        materialcraft_formats::write_atomic(&path, &f.bytes)?;
        written.push(path.display().to_string());
    }
    s.log(format!("Exported {} files to {dir}", written.len()));
    Ok(json!({"files": written}))
}

#[cfg(target_arch = "wasm32")]
fn export(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.export", "the browser exports with downloads"))
}

/// Embed image bytes as a resource; returns the resource name
pub fn add_resource(s: &mut Session, file: &str, bytes: Vec<u8>, name: Option<&str>) -> Result<String> {
    materialcraft_formats::decode(&bytes, false)?;
    s.edit("Import bitmap", |s| Ok(push_resource(&mut s.pkg, file, bytes, name)))
}

/// Add `bytes` to `pkg` as a resource named `name` (or after `file`), made unique; returns the name
pub(crate) fn push_resource(pkg: &mut Package, file: &str, bytes: Vec<u8>, name: Option<&str>) -> String {
    let file = materialcraft_formats::safe_file_name(file);
    let stem = name.map(str::to_string).unwrap_or_else(|| file.rsplit_once('.').map_or(file.clone(), |(a, _)| a.to_string()));
    let mut name = stem.clone();
    let mut i = 1;
    while pkg.resource(&name).is_some() {
        name = format!("{stem}_{i}");
        i += 1;
    }
    pkg.resources.push(Resource::new(name.clone(), format!("{name}_{file}"), ResourceBytes::new(bytes)));
    name
}

#[cfg(not(target_arch = "wasm32"))]
fn import_bitmap(s: &mut Session, p: &J) -> Result<J> {
    let path = strp(p, "path").ok_or_else(|| bad("file.importBitmap", "`path` is required"))?;
    let bytes = std::fs::read(path).map_err(|e| bad("file.importBitmap", format!("{path}: {e}")))?;
    let file = std::path::Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("bitmap.png").to_string();
    let name = add_resource(s, &file, bytes, strp(p, "name"))?;
    Ok(json!({"name": name, "use": {"command": "graph.node.add", "params": {"def": "bitmap", "params": {"resource": name}}}}))
}

#[cfg(target_arch = "wasm32")]
fn import_bitmap(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.importBitmap", "the browser imports with the file picker"))
}
