//! Resolve the portable SBS library alias against the application's asset directory.

/// Validate alias paths before joining a filesystem root. No package may escape the library.
pub fn relative_path(uri: &str) -> Result<&str, String> {
    let path = uri.strip_prefix("sbs://").ok_or("not an SBS library alias")?;
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':', '?', '#', '%', '\0'])
        || path.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("invalid SBS library path".into());
    }
    Ok(path)
}

/// Resolve a package-relative resource or dependency without leaving the library root.
pub fn resolve(owner: &str, reference: &str) -> Result<String, String> {
    let owner = relative_path(owner)?;
    if reference.starts_with("sbs://") {
        relative_path(reference)?;
        return Ok(reference.into());
    }

    let reference = reference.trim().replace('\\', "/");
    if reference.is_empty() || reference.len() > 4096 || reference.starts_with('/') || reference.contains([':', '?', '#', '%', '\0']) {
        return Err("invalid relative SBS library path".into());
    }

    let mut parts: Vec<_> = owner.split('/').collect();
    parts.pop();
    for part in reference.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("SBS reference escapes its library directory".into());
                }
            }
            part => parts.push(part),
        }
    }

    let uri = format!("sbs://{}", parts.join("/"));
    relative_path(&uri)?;
    Ok(uri)
}

/// The native asset root may be configured for installations with a separate data directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn root() -> std::path::PathBuf {
    if let Some(root) = std::env::var_os("MATERIALCRAFT_SBS_DIR") {
        return root.into();
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
        && parent.join("assets/sbs").is_dir()
    {
        return parent.join("assets/sbs");
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sbs")
}

pub fn read(uri: &str) -> Result<Vec<u8>, String> {
    let relative = relative_path(uri)?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Read;
        let root = root();
        if root.exists() || std::env::var_os("MATERIALCRAFT_SBS_DIR").is_some() {
            let root = root.canonicalize().map_err(|error| error.to_string())?;
            let file = root.join(relative).canonicalize().map_err(|error| format!("Missing library asset {uri}: {error}"))?;
            if !file.starts_with(&root) {
                return Err("SBS library symlink escapes its asset directory".into());
            }
            let mut bytes = Vec::new();
            std::fs::File::open(file).map_err(|error| error.to_string())?.take((256 << 20) + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
            if bytes.len() > 256 << 20 {
                return Err("SBS library asset exceeds 256 MiB".into());
            }
            return Ok(bytes);
        }
    }
    // Installed and browser builds use the same bundled asset when no disk asset root exists
    match relative {
        "functions.sbs" => Ok(include_bytes!("../../../../assets/sbs/functions.sbs").to_vec()),
        _ => Err(format!("Missing library asset {uri}")),
    }
}
