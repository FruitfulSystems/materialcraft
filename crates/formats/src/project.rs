//! SBS XML and its adjacent resources, either as files or a MaterialCraft ZIP.
//! Both representations use the same clean-room SBS reader and writer.

mod project_files;
pub use project_files::ProjectFiles;

use crate::{FormatError, Result, sbs};
use materialcraft_core::{Package, Value};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::io::{Cursor, Read};

pub const EXT: &str = "mcraft";
pub const SBS_ENTRY: &str = "package.sbs";
const MAX_BYTES: u64 = 256 << 20;
const MAX_ENTRIES: usize = 4096;
const MAX_PACKAGES: usize = 64;

fn bad(message: impl Into<String>) -> FormatError {
    FormatError::BadPackage(message.into())
}

pub fn is_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

fn references(package: &Package) -> Vec<String> {
    package
        .graphs
        .iter()
        .flat_map(|graph| {
            graph.instances().map(|instance| instance.package.clone()).chain(
                graph.nodes.iter().filter_map(|node| if let Some(Value::Text(path)) = node.params.get("sbs_package") { Some(path.clone()) } else { None }),
            )
        })
        .filter(|path| !path.is_empty())
        .collect()
}

fn remap(package: &mut Package, names: &BTreeMap<String, String>) {
    for graph in &mut package.graphs {
        for instance in graph.instances_mut() {
            if let Some(path) = names.get(&instance.package) {
                instance.package = path.clone();
            }
        }
        for node in &mut graph.nodes {
            if let Some(Value::Text(path)) = node.params.get_mut("sbs_package")
                && let Some(new) = names.get(path)
            {
                *path = new.clone();
            }
        }
    }
}

/// Resolve archive references without allowing absolute paths or traversal outside the archive.
fn resolve(owner: &str, reference: &str) -> Result<String> {
    let reference = reference.replace('\\', "/");
    if reference.starts_with('/') || reference.contains(':') || reference.contains('\0') {
        return Err(bad("Invalid archive resource path"));
    }

    let mut components: Vec<_> = owner.rsplit_once('/').map_or(Vec::new(), |(parent, _)| parent.split('/').collect());
    for part in reference.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(bad("Archive reference escapes its root"));
                }
            }
            name => components.push(name),
        }
    }
    Ok(components.join("/"))
}

pub fn read(bytes: &[u8], identity: &str) -> Result<sbs::SbsRead> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(bad("MaterialCraft archive exceeds 256 MiB"));
    }

    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| bad(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(bad("MaterialCraft archive exceeds 4096 entries"));
    }

    let mut files = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut remaining = MAX_BYTES;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| bad(error.to_string()))?;
        let name = entry.name().to_string();
        let path = name.trim_end_matches('/');
        if path.is_empty()
            || path.len() > 4096
            || path.split('/').count() > 64
            || path.contains('\\')
            || resolve("", path)? != path
            || !seen.insert(path.to_ascii_lowercase())
            || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(bad(format!("Invalid or duplicate archive entry `{name}`")));
        }

        if entry.is_dir() {
            continue;
        }

        if entry.size() > remaining {
            return Err(bad("Expanded archive exceeds 256 MiB"));
        }

        let mut content = Vec::new();
        (&mut entry).take(remaining + 1).read_to_end(&mut content).map_err(|error| bad(error.to_string()))?;
        remaining = remaining.checked_sub(content.len() as u64).ok_or_else(|| bad("Expanded archive exceeds 256 MiB"))?;
        files.insert(name, content);
    }
    let roots: Vec<_> = files.keys().filter(|name| !name.contains('/') && name.to_ascii_lowercase().ends_with(".sbs")).cloned().collect();
    if roots.len() != 1 {
        return Err(bad("A MaterialCraft archive must contain exactly one SBS file at its root"));
    }

    let root = roots.first().ok_or_else(|| bad("Missing SBS entry"))?.clone();
    let prefix = format!("mcraft://{identity}!/");
    let mut packages = BTreeMap::new();
    let mut pending = vec![root.clone()];
    let mut warnings = Vec::new();
    let mut resource_budget = MAX_BYTES;
    while let Some(file) = pending.pop() {
        if packages.contains_key(&file) {
            continue;
        }

        if packages.len() >= MAX_PACKAGES {
            return Err(bad("Too many archived SBS packages"));
        }

        let content = files.get(&file).ok_or_else(|| bad(format!("Missing archived package `{file}`")))?;
        let reader = |reference: &str| resolve(&file, reference).ok().and_then(|path| files.get(&path).cloned());
        let mut read = sbs::read_sbs(content, &file, &reader)?;

        // Several packages can reference the same archive asset; bound the loaded copies too
        for resource in &read.pkg.resources {
            resource_budget = resource_budget.checked_sub(resource.bytes.len() as u64).ok_or_else(|| bad("Loaded resources exceed 256 MiB"))?;
        }
        warnings.append(&mut read.warnings);
        let mut names = BTreeMap::new();
        for reference in references(&read.pkg) {
            if reference.starts_with("sbs://") {
                continue;
            }

            let target = resolve(&file, &reference)?;
            if !files.contains_key(&target) {
                return Err(bad(format!("Missing archived dependency `{target}`")));
            }

            names.insert(reference, format!("{prefix}{target}"));
            pending.push(target);
        }
        remap(&mut read.pkg, &names);
        packages.insert(file, read.pkg);
    }
    let mut root_package = packages.get(&root).cloned().ok_or_else(|| bad("Missing SBS package"))?;
    root_package.dependencies = packages.into_iter().map(|(path, package)| (format!("{prefix}{path}"), std::sync::Arc::new(package))).collect();
    Ok(sbs::SbsRead { pkg: root_package, warnings })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_files(path: &std::path::Path, files: &ProjectFiles) -> Result<()> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let root = std::fs::canonicalize(parent).map_err(|error| bad(error.to_string()))?;
    ensure_directory(&root, "resources")?;
    for (name, bytes) in &files.files {
        if name == &files.entry {
            continue;
        }

        if let Some((folder, _)) = name.rsplit_once('/') {
            ensure_directory(&root, folder)?;
        }
        let target = root.join(name);

        // Existing assets are immutable: they may be used by another SBS in this directory
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&target) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes.as_slice()).and_then(|_| file.sync_all()) {
                    let _ = std::fs::remove_file(&target);
                    return Err(bad(error.to_string()));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::fs::symlink_metadata(&target).map_err(|error| bad(error.to_string()))?.file_type().is_symlink()
                    || std::fs::metadata(&target).map_err(|error| bad(error.to_string()))?.len() != bytes.len() as u64
                    || std::fs::read(&target).map_err(|error| bad(error.to_string()))? != bytes.as_slice()
                {
                    return Err(bad(format!("Refusing to overwrite a different resource: {}", target.display())));
                }
            }
            Err(error) => return Err(bad(error.to_string())),
        }
    }
    crate::write_atomic(path, files.xml()?)
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_directory(root: &std::path::Path, relative: &str) -> Result<()> {
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {}
            Ok(_) => return Err(bad(format!("Resource directory is not a regular directory: {}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&path).map_err(|error| bad(error.to_string()))?,
            Err(error) => return Err(bad(error.to_string())),
        }
    }
    Ok(())
}
