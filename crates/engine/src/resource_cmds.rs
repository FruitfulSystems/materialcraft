//! Package resources and dependencies: linked bitmaps (read from disk, reloaded on demand),
//! resource removal, graphs imported from other package files, copies of the package with its
//! dependencies, and the session's size settings (history, cache, cooking limit)

use serde_json::{Value as J, json};

use materialcraft_core::{Package, ResourceBytes, Value};

use crate::commands::{CommandSpec, always, bad, strp, u64p};
use crate::{Result, Session, cmd};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
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
            "session.settings",
            "Session settings",
            [],
            None,
            "{historySize? (1..10000 undo steps), cacheMb? (16..65536, the cooked image cache), cookLimit? (256..8192 px, or 0 for none: caps every cook, exports included)} sets what is given; returns all three",
            always,
            session_settings
        ),
    ]
}

/// Read package bytes without touching the session: SBS, or a `.mcmat` material's source
pub fn load_bytes(bytes: &[u8], name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Package> {
    if materialcraft_formats::material::is_material(bytes) {
        let (_, source) = materialcraft_formats::material::read_material(bytes)?;
        return Ok(materialcraft_formats::sbs::read_sbs(&source, "source.sbs", &|_| None)?.pkg);
    }
    Ok(materialcraft_formats::sbs::read_sbs(bytes, name, linked)?.pkg)
}

/// Copy `graph` of `from` into the session's package: the graph, every graph it instances
/// (recursively) and the bitmap resources its nodes use. Graphs that already exist with the
/// same name are reused when identical, else the copy gets a unique name and the instances
/// that point at it follow. Returns the copied graph's name
pub fn import_graph_from(s: &mut Session, from: &Package, graph: &str) -> Result<String> {
    let (_, root) = from.graph_by_name(graph).ok_or_else(|| bad("file.importGraph", format!("`{}` has no graph `{graph}`", from.name)))?;
    // Walk the instance tree, deepest dependencies first so renames are known when they are used
    let mut order: Vec<String> = Vec::new();
    let mut stack = vec![root.name.clone()];
    while let Some(name) = stack.pop() {
        if order.contains(&name) {
            continue;
        }
        order.push(name.clone());
        if let Some((_, g)) = from.graph_by_name(&name) {
            for n in g.nodes.iter().filter(|n| n.def == "graph_instance") {
                if let Some(Value::Text(t)) = n.params.get("graph")
                    && !order.contains(t)
                {
                    stack.push(t.clone());
                }
            }
        }
    }
    order.reverse();
    let from = from.clone();
    let root_name = root.name.clone();
    s.edit("Import graph", |s| {
        let mut renamed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        for name in &order {
            let Some((_, g)) = from.graph_by_name(name) else { continue };
            let mut g = g.clone();
            for n in g.nodes.iter_mut() {
                if n.def == "graph_instance"
                    && let Some(Value::Text(t)) = n.params.get("graph").cloned()
                    && let Some(new) = renamed.get(&t)
                {
                    n.params.insert("graph".into(), Value::Text(new.clone()));
                }
                if let Some(Value::Text(r)) = n.params.get("resource").cloned()
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
        Ok(renamed.get(&root_name).cloned().unwrap_or(root_name))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn load_file(cmd: &str, p: &J) -> Result<(String, Package)> {
    let path = strp(p, "path").ok_or_else(|| bad(cmd, "`path` is required"))?;
    let bytes = std::fs::read(path).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
    Ok((path.to_string(), load_bytes(&bytes, path, &crate::file_cmds::linked_files(path))?))
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
            .map(|g| json!({"name": g.name, "kind": format!("{:?}", g.kind()), "outputs": g.nodes.iter().filter(|n| n.def == "output").count()}))
            .collect::<Vec<_>>()
    ))
}

#[cfg(target_arch = "wasm32")]
fn package_graphs(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.packageGraphs", "the browser has no package files to read"))
}

/// The path a linked file is stored under: relative to the package's folder when it lies
/// inside it (then the link survives in the saved file), else as given
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

/// Read a linked resource's file: a relative link resolves next to the package
#[cfg(not(target_arch = "wasm32"))]
fn read_link(s: &Session, link: &str) -> std::result::Result<Vec<u8>, String> {
    let plain = !link.contains(':') && !link.starts_with('/') && !link.starts_with('\\');
    if plain && let Some(path) = &s.path {
        return crate::file_cmds::linked_files(path)(link).ok_or_else(|| format!("`{link}` was not found next to the package"));
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
        // Relinking an existing resource keeps its name (the nodes that use it stay valid)
        if let Some(r) = wanted.as_deref().and_then(|w| s.pkg.resources.iter_mut().find(|r| r.name == w)) {
            r.bytes = ResourceBytes::new(bytes);
            r.link = Some(link.clone());
            return Ok(r.name.clone());
        }
        let name = crate::file_cmds::push_resource(&mut s.pkg, &file, bytes, wanted.as_deref());
        if let Some(r) = s.pkg.resources.iter_mut().find(|r| r.name == name) {
            r.link = Some(link.clone());
        }
        Ok(name)
    })?;
    s.clear_decoded();
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
        s.clear_decoded();
    }
    s.dirty = true;
    Ok(json!({"reloaded": changed, "missing": missing}))
}

#[cfg(target_arch = "wasm32")]
fn reload_resources(s: &mut Session, _: &J) -> Result<J> {
    s.dirty = true;
    Ok(json!({"reloaded": [], "missing": []}))
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

#[cfg(not(target_arch = "wasm32"))]
fn save_copy(s: &mut Session, p: &J) -> Result<J> {
    let path = crate::file_cmds::package_path(strp(p, "path").ok_or_else(|| bad("file.saveCopy", "`path` is required"))?);
    let bytes = crate::file_cmds::save_bytes(s)?;
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
    // The copy links every linked bitmap by its file name, next to the package
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
    let name = format!("{}.{}", materialcraft_formats::safe_file_name(&pkg.name), materialcraft_formats::sbs::SBS_EXT);
    let target = dir.join(&name);
    let bytes = materialcraft_formats::sbs::write_sbs(&pkg)?;
    materialcraft_formats::write_atomic(&target, &bytes)?;
    written.insert(0, target.display().to_string());
    s.log(format!("Exported {} with {} dependency file(s) to {}", pkg.name, written.len() - 1, dir.display()));
    Ok(json!({"files": written}))
}

#[cfg(target_arch = "wasm32")]
fn export_with_dependencies(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.exportWithDependencies", "the browser saves with a download"))
}

fn session_settings(s: &mut Session, p: &J) -> Result<J> {
    if let Some(n) = u64p(p, "historySize") {
        s.max_undo = n.clamp(1, 10_000) as usize;
        s.trim_undo();
    }
    if let Some(mb) = u64p(p, "cacheMb") {
        let bytes = (mb.clamp(16, 65_536) as usize) << 20;
        if let Ok(mut c) = s.cache.lock() {
            c.set_cap(bytes);
        }
    }
    if let Some(px) = u64p(p, "cookLimit") {
        let limit = if px == 0 { None } else { Some(px.clamp(256, 8192) as u32) };
        if limit != s.cook_limit {
            s.cook_limit = limit;
            s.dirty = true;
        }
    }
    let cache_mb = s.cache.lock().map(|c| c.cap() >> 20).unwrap_or(0);
    Ok(json!({"historySize": s.max_undo, "cacheMb": cache_mb, "cookLimit": s.cook_limit.unwrap_or(0)}))
}
