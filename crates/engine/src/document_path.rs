pub(crate) fn normalize(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::path::{Component, PathBuf};
        let path = PathBuf::from(path);
        if let Ok(path) = path.canonicalize() {
            return path.to_string_lossy().into_owned();
        }
        let absolute = if path.is_absolute() { path } else { std::env::current_dir().unwrap_or_default().join(path) };
        // New files need a canonical parent so their identity stays the same after saving
        if let (Some(parent), Some(filename)) = (absolute.parent(), absolute.file_name())
            && let Ok(parent) = parent.canonicalize()
        {
            return parent.join(filename).to_string_lossy().into_owned();
        }
        let mut normalized = PathBuf::new();
        for part in absolute.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                part => normalized.push(part.as_os_str()),
            }
        }
        normalized.to_string_lossy().into_owned()
    }
    #[cfg(target_arch = "wasm32")]
    {
        path.replace('\\', "/")
    }
}

pub(crate) fn resolve(owner: &str, reference: &str) -> String {
    let path = std::path::Path::new(reference);
    if path.is_absolute() {
        return normalize(reference);
    }
    normalize(&std::path::Path::new(owner).parent().unwrap_or(std::path::Path::new("")).join(path).to_string_lossy())
}

pub(crate) fn relative(owner: &str, source: &str) -> String {
    let folder = std::path::Path::new(owner).parent().unwrap_or(std::path::Path::new(""));
    let from: Vec<_> = folder.components().collect();
    let to: Vec<_> = std::path::Path::new(source).components().collect();
    let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
    if common == 0 {
        return source.replace('\\', "/");
    }
    let mut path = std::path::PathBuf::new();
    for _ in common..from.len() {
        path.push("..");
    }
    for part in to.iter().skip(common) {
        path.push(part.as_os_str());
    }
    path.to_string_lossy().replace('\\', "/")
}
