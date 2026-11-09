//! SBS XML and its adjacent resources, either as files or a MaterialCraft ZIP.
//! Both representations use the same clean-room SBS reader and writer.

use crate::{FormatError, Result, sbs};
use materialcraft_core::{Package, ResourceBytes, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Write};

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

/// Files relative to the directory containing `entry`. Assets keep their original bytes.
pub struct ProjectFiles {
    pub entry: String,
    pub files: BTreeMap<String, ResourceBytes>,
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

impl ProjectFiles {
    pub fn build(package: &Package, filename: &str, archive: bool) -> Result<Self> {
        let filename = crate::safe_file_name(filename);
        let stem = filename.rsplit_once('.').map_or(filename.as_str(), |(stem, _)| stem);
        let entry = if archive { SBS_ENTRY.to_string() } else { format!("{stem}.sbs") };
        let mut names = BTreeMap::new();
        let mut pending = references(package);
        let mut visited = BTreeSet::new();
        while let Some(path) = pending.pop() {
            if !visited.insert(path.clone()) || (!archive && !path.starts_with("mcraft://")) {
                continue;
            }
            // Installed library aliases remain valid without copying the whole standard library.
            let Some(source) = package.dependencies.get(&path) else {
                if path.starts_with("sbs://") {
                    continue;
                }
                return Err(bad(format!("Cannot package missing dependency `{path}`")));
            };
            if names.len() >= MAX_PACKAGES - 1 {
                return Err(bad("Too many dependent SBS packages"));
            }
            names.insert(path, format!("resources/packages/{}.sbs", names.len() + 1));
            pending.extend(references(source));
        }
        // Changes to any dependency create a new immutable directory, including references between dependencies.
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        for source in names.keys() {
            source.hash(&mut digest);
            let source = package.dependencies.get(source).ok_or_else(|| bad("Missing package dependency"))?;
            sbs::write_sbs(source)?.hash(&mut digest);
        }
        let directory = format!("resources/packages/{:016x}", digest.finish());
        for (index, file) in names.values_mut().enumerate() {
            *file = format!("{directory}/{}.sbs", index + 1);
        }
        let mut result = Self { entry: entry.clone(), files: BTreeMap::new() };
        result.add_package(package, &entry, &names)?;
        for (source, file) in &names {
            let package = package.dependencies.get(source).ok_or_else(|| bad("Package dependency disappeared"))?;
            result.add_package(package, file, &names)?;
        }
        let total = result.files.values().try_fold(0_u64, |sum, bytes| sum.checked_add(bytes.len() as u64)).ok_or_else(|| bad("Package is too large"))?;
        if total > MAX_BYTES || result.files.len().saturating_add(1) > MAX_ENTRIES {
            return Err(bad("Package exceeds the 256 MiB or 4096-file limit"));
        }
        Ok(result)
    }

    fn add_package(&mut self, package: &Package, file: &str, names: &BTreeMap<String, String>) -> Result<()> {
        let mut package = package.clone();
        let folder = file.rsplit_once('/').map_or("", |(folder, _)| folder);
        let local_names = names
            .iter()
            .map(|(old, new)| {
                let new = if folder.is_empty() { new.clone() } else { new.strip_prefix(&format!("{folder}/")).unwrap_or(new).to_string() };
                (old.clone(), new)
            })
            .collect();
        remap(&mut package, &local_names);
        for resource in &mut package.resources {
            if resource.bytes.is_empty() {
                if !resource.source.is_empty() {
                    return Err(bad(format!("Cannot save missing resource `{}`: {}", resource.name, resource.source)));
                }
                return Err(bad(format!("Resource `{}` has no data", resource.name)));
            }
            let name = crate::safe_file_name(&resource.file);
            let extension = name.rsplit_once('.').map_or("bin", |(_, extension)| extension);
            let stem: String = crate::safe_file_name(&resource.name).chars().take(40).collect();
            let extension: String = extension.chars().take(16).collect::<String>().to_ascii_lowercase();
            // Immutable resource names keep a failed save, or a sibling SBS, from overwriting assets used by the last saved document.
            let relative = format!("resources/{stem}-{:016x}.{extension}", resource.bytes.content_hash());
            let path = if folder.is_empty() { relative.clone() } else { format!("{folder}/{relative}") };
            if let Some(existing) = self.files.get(&path) {
                if existing != &resource.bytes {
                    return Err(bad("Resource filename collision"));
                }
            } else {
                self.files.insert(path, resource.bytes.clone());
            }
            resource.source = relative;
            resource.bytes = ResourceBytes::default();
        }
        self.files.insert(file.into(), ResourceBytes::new(sbs::write_sbs(&package)?));
        Ok(())
    }

    pub fn zip(&self) -> Result<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer.add_directory("resources/", zip::write::SimpleFileOptions::default()).map_err(|error| bad(error.to_string()))?;
        for (name, bytes) in &self.files {
            let text = name.ends_with(".sbs") || name.ends_with(".svg");
            let method = if text { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
            writer.start_file(name, zip::write::SimpleFileOptions::default().compression_method(method)).map_err(|error| bad(error.to_string()))?;
            writer.write_all(bytes.as_slice()).map_err(|error| bad(error.to_string()))?;
        }
        let bytes = writer.finish().map_err(|error| bad(error.to_string()))?.into_inner();
        if bytes.len() as u64 > MAX_BYTES {
            return Err(bad("MaterialCraft archive exceeds 256 MiB"));
        }
        Ok(bytes)
    }

    pub fn xml(&self) -> Result<&[u8]> {
        self.files.get(&self.entry).map(ResourceBytes::as_slice).ok_or_else(|| bad("The entry SBS is missing"))
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
        // Several packages can reference the same archive asset; bound the loaded copies too.
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
        // Existing assets are immutable: they may be used by another SBS in this directory.
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
