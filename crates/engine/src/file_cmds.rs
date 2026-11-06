//! File menu: new/open/save packages, new graphs from templates, bitmap resources and texture
//! export. Host-path commands are native only; the browser and agents use the byte-based
//! functions (`open_bytes`, `save_bytes`, `export_outputs`).
//!
//! SBS (`.sbs`) is the main project format; `.mcraft` packages still open and save as `.mcraft`.
//! What an SBS file holds that MaterialCraft can't read is a warning (Console, `package.warnings`), never an error.

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
            "{name?, template? (see graph.templates), size? (px, power of two)} adds a new graph to the package and opens it",
            always,
            new_graph
        ),
        cmd!("file.newPackage", "Empty package", ["File", "New"], None, "{name?} replaces the session with an empty package", always, new_package),
        cmd!(
            "file.open",
            "Open...",
            ["File"],
            Some("Ctrl+O"),
            "{path} opens an .sbs or .mcraft package (or a .mcmat material's source); returns `warnings` for what an SBS file holds that MaterialCraft can't read yet, and `path`, null for a file another program wrote (Save then needs a new `path`)",
            always,
            open
        ),
        cmd!(
            "file.save",
            "Save",
            ["File"],
            Some("Ctrl+S"),
            "{path?} saves the package to its own file or `path`: SBS (.sbs, the default; appended when `path` has no package extension) or .mcraft",
            always,
            save
        ),
        cmd!("file.saveAs", "Save as...", ["File"], Some("Ctrl+Shift+S"), "{path} saves the package under a new name", always, save),
        cmd!(
            "file.saveCopy",
            "Save a copy as...",
            ["File"],
            None,
            "{path} writes the package to `path` while the session keeps its own file",
            always,
            save_copy
        ),
        cmd!(
            "file.exportWithDependencies",
            "Export with dependencies...",
            ["File"],
            None,
            "{dir} writes the package and copies of its linked bitmaps into `dir`, the links made relative; returns the files written",
            always,
            export_with_dependencies
        ),
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
        cmd!(
            "file.linkBitmap",
            "Link bitmap...",
            ["File"],
            None,
            "{path, name?} adds an image file as a linked resource (read from disk, reloaded by file.reloadResources; saved as a link when the file lies in the package's folder); returns its name",
            always,
            link_bitmap
        ),
        cmd!(
            "file.reloadResources",
            "Reload resources",
            ["File"],
            None,
            "{} reads every linked resource from its file again; returns the names reloaded and those whose file is missing",
            always,
            reload_resources
        ),
        cmd!(
            "file.deleteResource",
            "Remove resource",
            [],
            None,
            "{name} removes a bitmap resource; the nodes that used it show an error until they get another",
            always,
            delete_resource
        ),
        cmd!(
            "file.importGraph",
            "Import graph...",
            ["File"],
            None,
            "{path, graph?} copies a graph (default: the first) of another package file into this one, with the graphs it instances and the bitmaps it uses; returns the new graph's name",
            always,
            import_graph
        ),
        cmd!(
            "file.packageGraphs",
            "Graphs of a package file",
            [],
            None,
            "{path} lists the graphs of a package file without opening it: [{name, kind, outputs}]",
            always,
            package_graphs
        ),
        cmd!("graph.templates", "Graph templates", [], None, "{} lists the templates for file.new", always, |_, _| Ok(json!(
            TEMPLATES.iter().map(|(id, l)| json!({"id": id, "label": l})).collect::<Vec<_>>()
        ))),
    ]
}

fn new_graph(s: &mut Session, p: &J) -> Result<J> {
    let tpl = strp(p, "template").unwrap_or("pbr");
    let label = TEMPLATES.iter().find(|t| t.0 == tpl).map_or("Graph", |t| t.1);
    let size = u64p(p, "size").unwrap_or(1024).clamp(16, 8192);
    let log2 = 63 - size.leading_zeros();
    let base = strp(p, "name").map(str::to_string).unwrap_or_else(|| label.replace(' ', "_"));
    s.edit("New graph", |s| {
        let name = s.pkg.unique_graph_name(&base);
        s.pkg.graphs.push(template(tpl, &name, log2));
        s.graph = s.pkg.graphs.len() - 1;
        s.selection.clear();
        s.view2d.node = None;
        Ok(json!({"graph": s.graph, "name": name}))
    })
}

fn new_package(s: &mut Session, p: &J) -> Result<J> {
    let mut pkg = Package::new(strp(p, "name").unwrap_or("Untitled package"));
    pkg.graphs.push(template("pbr", "Material_Graph", 10));
    replace_package(s, pkg, None);
    Ok(json!({"name": s.pkg.name}))
}

/// Make `pkg` the session's document (after open/new): history and views reset.
pub fn replace_package(s: &mut Session, pkg: Package, path: Option<String>) {
    s.pkg = pkg;
    s.clear_decoded();
    s.path = path;
    s.graph = 0;
    s.selection.clear();
    s.view2d.node = None;
    s.reset_history();
    s.modified = false;
    s.dirty = true;
    s.load_warnings.clear();
}

/// A package file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageFormat {
    /// SBS XML, the main project format.
    Sbs,
    /// The older native zip format.
    Mcraft,
}

impl PackageFormat {
    /// The format a file at `path` is saved in: `.mcraft` stays `.mcraft`, anything else is SBS.
    pub fn of_path(path: &str) -> PackageFormat {
        if has_extension(path, PackageFormat::Mcraft.ext()) { PackageFormat::Mcraft } else { PackageFormat::Sbs }
    }

    pub fn ext(self) -> &'static str {
        match self {
            PackageFormat::Sbs => materialcraft_formats::sbs::SBS_EXT,
            PackageFormat::Mcraft => materialcraft_core::PACKAGE_EXT,
        }
    }
}

fn has_extension(path: &str, ext: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

/// `path` with `.sbs` appended unless it already ends in `.sbs` or `.mcraft`.
pub fn package_path(path: &str) -> String {
    if [PackageFormat::Sbs, PackageFormat::Mcraft].iter().any(|f| has_extension(path, f.ext())) {
        path.to_string()
    } else {
        format!("{path}.{}", PackageFormat::Sbs.ext())
    }
}

fn delete_resource(s: &mut Session, p: &J) -> Result<J> {
    let name = strp(p, "name").ok_or_else(|| bad("file.deleteResource", "`name` is required"))?.to_string();
    if s.pkg.resource(&name).is_none() {
        return Err(bad("file.deleteResource", format!("no resource `{name}`")));
    }
    s.edit("Remove resource", |s| {
        s.pkg.resources.retain(|r| r.name != name);
        Ok(J::Null)
    })?;
    s.clear_decoded();
    Ok(json!({"name": name}))
}

/// Read package bytes without touching the session: SBS, `.mcraft`, or a `.mcmat` material's source.
pub fn load_bytes(bytes: &[u8], name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Package> {
    if materialcraft_formats::material::is_material(bytes) {
        let (_, source) = materialcraft_formats::material::read_material(bytes)?;
        return Ok(materialcraft_formats::load_package(&source)?);
    }
    let named_sbs = has_extension(name, PackageFormat::Sbs.ext()) && !bytes.starts_with(b"PK");
    if !named_sbs && !materialcraft_formats::sbs::is_sbs(bytes) {
        return Ok(materialcraft_formats::load_package(bytes)?);
    }
    Ok(materialcraft_formats::sbs::read_sbs(bytes, name, linked)?.pkg)
}

/// Copy `graph` of `from` into the session's package: the graph, every graph it instances
/// (recursively) and the bitmap resources its nodes use. Graphs that already exist with the
/// same name are reused when identical, else the copy gets a unique name and the instances
/// that point at it follow. Returns the copied graph's name.
pub fn import_graph_from(s: &mut Session, from: &Package, graph: &str) -> Result<String> {
    let (_, root) = from.graph_by_name(graph).ok_or_else(|| bad("file.importGraph", format!("`{}` has no graph `{graph}`", from.name)))?;
    // Walk the instance tree, deepest dependencies first so renames are known when they are used.
    let mut order: Vec<String> = Vec::new();
    let mut stack = vec![root.name.clone()];
    while let Some(name) = stack.pop() {
        if order.contains(&name) {
            continue;
        }
        order.push(name.clone());
        if let Some((_, g)) = from.graph_by_name(&name) {
            for n in g.nodes.iter().filter(|n| n.def == "graph_instance") {
                if let Some(materialcraft_core::Value::Text(t)) = n.params.get("graph")
                    && !order.contains(t)
                {
                    stack.push(t.clone());
                }
            }
        }
    }
    order.reverse();
    let from = from.clone();
    s.edit("Import graph", |s| {
        let mut renamed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        for name in &order {
            let Some((_, g)) = from.graph_by_name(name) else { continue };
            let mut g = g.clone();
            for n in g.nodes.iter_mut() {
                if n.def == "graph_instance"
                    && let Some(materialcraft_core::Value::Text(t)) = n.params.get("graph").cloned()
                    && let Some(new) = renamed.get(&t)
                {
                    n.params.insert("graph".into(), materialcraft_core::Value::Text(new.clone()));
                }
                if let Some(materialcraft_core::Value::Text(r)) = n.params.get("resource").cloned()
                    && let Some(res) = from.resource(&r)
                    && s.pkg.resource(&r).is_none()
                {
                    s.pkg.resources.push(res.clone());
                }
            }
            if let Some((_, have)) = s.pkg.graph_by_name(name)
                && *have == g
            {
                renamed.insert(name.clone(), name.clone());
                continue;
            }
            let new = s.pkg.unique_graph_name(name);
            g.name = new.clone();
            renamed.insert(name.clone(), new);
            s.pkg.graphs.push(g);
        }
        Ok(renamed.get(&root.name).cloned().unwrap_or_else(|| root.name.clone()))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn load_file(cmd: &str, p: &J) -> Result<(String, Package)> {
    let path = strp(p, "path").ok_or_else(|| bad(cmd, "`path` is required"))?;
    let bytes = std::fs::read(path).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
    Ok((path.to_string(), load_bytes(&bytes, path, &linked_files(path))?))
}

#[cfg(not(target_arch = "wasm32"))]
fn import_graph(s: &mut Session, p: &J) -> Result<J> {
    let (_, from) = load_file("file.importGraph", p)?;
    let graph = match strp(p, "graph") {
        Some(g) => g.to_string(),
        None => from.graphs.first().map(|g| g.name.clone()).ok_or_else(|| bad("file.importGraph", "the package has no graphs"))?,
    };
    let name = import_graph_from(s, &from, &graph)?;
    Ok(json!({"name": name}))
}

#[cfg(target_arch = "wasm32")]
fn import_graph(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.importGraph", "the browser has no package files to import from"))
}

#[cfg(not(target_arch = "wasm32"))]
fn package_graphs(_: &mut Session, p: &J) -> Result<J> {
    let (_, from) = load_file("file.packageGraphs", p)?;
    Ok(json!(
        from.graphs
            .iter()
            .map(|g| json!({"name": g.name, "kind": format!("{:?}", g.kind), "outputs": g.nodes.iter().filter(|n| n.def == "output").count()}))
            .collect::<Vec<_>>()
    ))
}

#[cfg(target_arch = "wasm32")]
fn package_graphs(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.packageGraphs", "the browser has no package files to read"))
}

/// Open package bytes on any platform: SBS, `.mcraft`, or a `.mcmat` material's source package.
pub fn open_bytes(s: &mut Session, bytes: &[u8], name: Option<String>) -> Result<()> {
    open_bytes_linked(s, bytes, name, &|_| None)
}

/// [`open_bytes`] with `linked` loading files an SBS package links to; materials and SBS files from other programs open unnamed so Save can't overwrite them.
pub fn open_bytes_linked(s: &mut Session, bytes: &[u8], name: Option<String>, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<()> {
    if materialcraft_formats::material::is_material(bytes) {
        let (manifest, source) = materialcraft_formats::material::read_material(bytes)?;
        replace_package(s, materialcraft_formats::load_package(&source)?, None);
        s.graph = s.pkg.graph_by_name(&manifest.graph).map_or(0, |(i, _)| i);
        return Ok(());
    }

    // A broken `.sbs` should report an SBS error, but a renamed zip package still opens
    let named_sbs = name.as_deref().is_some_and(|n| has_extension(n, PackageFormat::Sbs.ext())) && !bytes.starts_with(b"PK");
    if !named_sbs && !materialcraft_formats::sbs::is_sbs(bytes) {
        replace_package(s, materialcraft_formats::load_package(bytes)?, name);
        return Ok(());
    }

    let read = materialcraft_formats::sbs::read_sbs(bytes, name.as_deref().unwrap_or(""), linked)?;
    let from_elsewhere = !read.exact;
    replace_package(s, read.pkg, if from_elsewhere { None } else { name.clone() });

    if !read.warnings.is_empty() {
        s.warn(format!("{} opened with {} warning(s): what MaterialCraft can't read yet is listed below", s.pkg.name, read.warnings.len()));
        for warning in &read.warnings {
            s.warn(warning.clone());
        }
    }
    if from_elsewhere && let Some(file) = &name {
        s.log(format!("{file} was not saved by MaterialCraft: Save asks for a new file, so the original keeps everything MaterialCraft can't read"));
    }
    s.load_warnings = read.warnings;
    Ok(())
}

/// The package as SBS bytes, the main project format.
pub fn save_bytes(s: &Session) -> Result<Vec<u8>> {
    save_bytes_as(s, PackageFormat::Sbs)
}

pub fn save_bytes_as(s: &Session, format: PackageFormat) -> Result<Vec<u8>> {
    Ok(match format {
        PackageFormat::Sbs => materialcraft_formats::sbs::write_sbs(&s.pkg)?,
        PackageFormat::Mcraft => materialcraft_formats::save_package(&s.pkg)?,
    })
}

/// Largest file an SBS package may link to.
#[cfg(not(target_arch = "wasm32"))]
const MAX_LINKED_FILE_BYTES: u64 = 256 << 20;

/// Reads files an SBS package at `path` links to: plain relative paths that resolve inside its folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn linked_files(path: &str) -> impl Fn(&str) -> Option<Vec<u8>> + use<> {
    let parent = std::path::Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let folder = std::fs::canonicalize(parent).ok();

    move |relative: &str| {
        let relative = relative.trim().replace('\\', "/");
        let plain = !relative.is_empty() && !relative.contains(':') && !relative.starts_with('/');
        if !plain || std::path::Path::new(&relative).components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return None;
        }

        let folder = folder.as_ref()?;
        let file = std::fs::canonicalize(folder.join(&relative)).ok()?;
        let metadata = std::fs::metadata(&file).ok()?;
        if !file.starts_with(folder) || !metadata.is_file() || metadata.len() > MAX_LINKED_FILE_BYTES {
            return None;
        }
        std::fs::read(file).ok()
    }
}

/// Open the package file at `path` (native), with the bitmaps an SBS package keeps next to it.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_file(s: &mut Session, path: &str) -> Result<()> {
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
fn open(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.open", "the browser opens files with the file picker"))
}

#[cfg(not(target_arch = "wasm32"))]
fn save(s: &mut Session, p: &J) -> Result<J> {
    let path = match strp(p, "path") {
        Some(path) => package_path(path),
        None => s.path.clone().ok_or_else(|| bad("file.save", "the package has no file yet (new, or opened from another program): give a `path`"))?,
    };

    let bytes = save_bytes_as(s, PackageFormat::of_path(&path))?;
    materialcraft_formats::write_atomic(std::path::Path::new(&path), &bytes)?;
    s.path = Some(path.clone());
    s.modified = false;
    s.log(format!("Saved {path}"));
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

#[cfg(target_arch = "wasm32")]
fn save(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.save", "the browser saves with a download"))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_copy(s: &mut Session, p: &J) -> Result<J> {
    let path = package_path(strp(p, "path").ok_or_else(|| bad("file.saveCopy", "`path` is required"))?);
    let bytes = save_bytes_as(s, PackageFormat::of_path(&path))?;
    materialcraft_formats::write_atomic(std::path::Path::new(&path), &bytes)?;
    s.log(format!("Saved a copy to {path}"));
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

#[cfg(target_arch = "wasm32")]
fn save_copy(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.saveCopy", "the browser saves with a download"))
}

#[cfg(not(target_arch = "wasm32"))]
fn export_with_dependencies(s: &mut Session, p: &J) -> Result<J> {
    let dir = strp(p, "dir").ok_or_else(|| bad("file.exportWithDependencies", "`dir` is required"))?;
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| bad("file.exportWithDependencies", format!("{}: {e}", dir.display())))?;
    let mut written: Vec<String> = Vec::new();
    // The copy links every linked bitmap by its file name, next to the package.
    let mut pkg = s.pkg.clone();
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for r in pkg.resources.iter_mut() {
        let Some(link) = r.link.clone() else { continue };
        let bytes = match read_link(s, &link) {
            Ok(b) => b,
            Err(why) => {
                s.log(format!("`{}` exported embedded: {why}", r.name));
                r.link = None;
                continue;
            }
        };
        let base = std::path::Path::new(&link).file_name().and_then(|f| f.to_str()).unwrap_or(&r.file).to_string();
        let mut file = base.clone();
        let mut n = 2;
        while !used.insert(file.clone()) {
            file = match base.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem}_{n}.{ext}"),
                None => format!("{base}_{n}"),
            };
            n += 1;
        }
        let target = dir.join(&file);
        materialcraft_formats::write_atomic(&target, &bytes)?;
        written.push(target.display().to_string());
        r.link = Some(file);
        r.bytes = ResourceBytes::new(bytes);
    }
    let name = format!("{}.{}", materialcraft_formats::safe_file_name(&pkg.name), PackageFormat::Sbs.ext());
    let target = dir.join(&name);
    let bytes = materialcraft_formats::sbs::write_sbs(&pkg)?;
    materialcraft_formats::write_atomic(&target, &bytes)?;
    written.insert(0, target.display().to_string());
    s.log(format!("Exported {} with {} dependenc{} to {}", pkg.name, written.len() - 1, if written.len() == 2 { "y" } else { "ies" }, dir.display()));
    Ok(json!({"files": written}))
}

#[cfg(target_arch = "wasm32")]
fn export_with_dependencies(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.exportWithDependencies", "the browser saves with a download"))
}

/// One exported file.
pub struct Exported {
    pub file: String,
    pub bytes: Vec<u8>,
}

/// Cook graph `gi` at full size and encode every output (`pattern` names files).
pub fn export_outputs(s: &Session, gi: usize, ext: &str, pattern: &str, depth: Option<Depth>, size: Option<u32>) -> Result<Vec<Exported>> {
    let mut pkg = s.pkg.clone();
    if let Some(px) = size {
        let g = pkg.graph_mut(gi)?;
        // The larger side becomes `px`; the other keeps its proportion.
        let target = 31 - px.clamp(16, 8192).leading_zeros();
        let bounded = g.size.map(|l| l.clamp(4, 13));
        let larger = bounded[0].max(bounded[1]);
        g.size = bounded.map(|l| (l + target).saturating_sub(larger).max(4));
    }
    let g = pkg.graph(gi)?;
    let dec = s.resource_decoder();
    let mut opts = materialcraft_eval::Options::new(&dec);
    opts.max_size = s.cook_limit;
    let cooked = materialcraft_eval::cook(&pkg, gi, &opts, &s.cache);
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

/// Export params: (graph index, extension, pattern, depth, size).
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

/// Embed image bytes as a resource; returns the resource name.
pub fn add_resource(s: &mut Session, file: &str, bytes: Vec<u8>, name: Option<&str>) -> Result<String> {
    materialcraft_formats::decode(&bytes, false)?;
    s.edit("Import bitmap", |s| Ok(push_resource(&mut s.pkg, file, bytes, name)))
}

/// Add `bytes` to `pkg` as a resource named `name` (or after `file`), made unique; returns the name.
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

/// The path a linked file is stored under: relative to the package's folder when it lies
/// inside it (then the link survives in the saved file), else as given.
#[cfg(not(target_arch = "wasm32"))]
fn link_path(s: &Session, path: &str) -> String {
    let Some(folder) = s.path.as_deref().and_then(|p| std::path::Path::new(p).parent()).and_then(|p| std::fs::canonicalize(p).ok()) else {
        return path.to_string();
    };
    match std::fs::canonicalize(path).ok().and_then(|f| f.strip_prefix(&folder).ok().map(|r| r.to_path_buf())) {
        Some(rel) => rel.to_string_lossy().replace('\\', "/"),
        None => path.to_string(),
    }
}

/// Read a linked resource's file: a relative link resolves next to the package.
#[cfg(not(target_arch = "wasm32"))]
fn read_link(s: &Session, link: &str) -> std::result::Result<Vec<u8>, String> {
    let plain = !link.contains(':') && !link.starts_with('/') && !link.starts_with('\\');
    if plain && let Some(path) = &s.path {
        return linked_files(path)(link).ok_or_else(|| format!("`{link}` was not found next to the package"));
    }
    std::fs::read(link).map_err(|e| format!("{link}: {e}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn link_bitmap(s: &mut Session, p: &J) -> Result<J> {
    let path = strp(p, "path").ok_or_else(|| bad("file.linkBitmap", "`path` is required"))?;
    let bytes = std::fs::read(path).map_err(|e| bad("file.linkBitmap", format!("{path}: {e}")))?;
    materialcraft_formats::decode(&bytes, false)?;
    let file = std::path::Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("bitmap.png").to_string();
    let link = link_path(s, path);
    let wanted = strp(p, "name").map(str::to_string);
    let name = s.edit("Link bitmap", |s| {
        // Relinking an existing resource keeps its name (the nodes that use it stay valid).
        if let Some(r) = wanted.as_deref().and_then(|w| s.pkg.resources.iter_mut().find(|r| r.name == w)) {
            r.bytes = ResourceBytes::new(bytes);
            r.link = Some(link.clone());
            return Ok(r.name.clone());
        }
        let name = push_resource(&mut s.pkg, &file, bytes, wanted.as_deref());
        if let Some(r) = s.pkg.resources.iter_mut().find(|r| r.name == name) {
            r.link = Some(link.clone());
        }
        Ok(name)
    })?;
    Ok(json!({"name": name, "link": link, "use": {"command": "graph.node.add", "params": {"def": "bitmap", "params": {"resource": name}}}}))
}

#[cfg(target_arch = "wasm32")]
fn link_bitmap(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.linkBitmap", "the browser has no files to link to; import the bitmap instead"))
}

#[cfg(not(target_arch = "wasm32"))]
fn reload_resources(s: &mut Session, _: &J) -> Result<J> {
    let links: Vec<(String, String)> = s.pkg.resources.iter().filter_map(|r| r.link.clone().map(|l| (r.name.clone(), l))).collect();
    let mut fresh: Vec<(String, Vec<u8>)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for (name, link) in links {
        match read_link(s, &link) {
            Ok(bytes) => fresh.push((name, bytes)),
            Err(why) => {
                s.log(format!("resource `{name}` not reloaded: {why}"));
                missing.push(name);
            }
        }
    }
    let changed: Vec<String> =
        fresh.iter().filter(|(n, b)| s.pkg.resources.iter().any(|r| &r.name == n && r.bytes.as_slice() != b.as_slice())).map(|(n, _)| n.clone()).collect();
    if !changed.is_empty() {
        s.edit("Reload resources", |s| {
            for (name, bytes) in fresh {
                if let Some(r) = s.pkg.resources.iter_mut().find(|r| r.name == name) {
                    r.bytes = ResourceBytes::new(bytes);
                }
            }
            Ok(())
        })?;
    }
    s.dirty = true;
    Ok(json!({"reloaded": changed, "missing": missing}))
}

#[cfg(target_arch = "wasm32")]
fn reload_resources(s: &mut Session, _: &J) -> Result<J> {
    s.dirty = true;
    Ok(json!({"reloaded": [], "missing": []}))
}
