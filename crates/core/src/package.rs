//! Packages: the document. A package holds graphs and the resources (bitmaps) they use.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{CoreError, Graph, Result};

/// The native package extension (a zip: `package.json` plus `resources/`).
pub const PACKAGE_EXT: &str = "mcraft";

/// An embedded file: a bitmap a Bitmap node shows, or an SVG.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Unique name, referenced by nodes (`resource` parameter).
    pub name: String,
    /// File name inside the package's `resources/` folder.
    pub file: String,
    /// The file's bytes (stored in the zip, not in `package.json`).
    #[serde(skip)]
    pub bytes: ResourceBytes,
    /// A linked resource: the file it is read from (relative to the package's folder when it
    /// lies inside it), reloaded by "Reload resources". `None` = embedded in the package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

impl Resource {
    /// A resource named `name`, stored as `file` in the package.
    pub fn new(name: impl Into<String>, file: impl Into<String>, bytes: ResourceBytes) -> Resource {
        Resource { name: name.into(), file: file.into(), bytes, link: None }
    }

    /// A resource that stays on disk at `link` and is read from there.
    pub fn linked(name: impl Into<String>, file: impl Into<String>, bytes: ResourceBytes, link: impl Into<String>) -> Resource {
        Resource { name: name.into(), file: file.into(), bytes, link: Some(link.into()) }
    }
}

/// A resource's bytes with a hash of their whole content, computed once when the bytes are
/// set, so caches can key on content (two files of one size are not the same image).
#[derive(Clone, Debug)]
pub struct ResourceBytes {
    bytes: Arc<Vec<u8>>,
    hash: u64,
}

impl ResourceBytes {
    /// Take ownership of `bytes` and hash them.
    pub fn new(bytes: Vec<u8>) -> ResourceBytes {
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        ResourceBytes { hash: h.finish(), bytes: Arc::new(bytes) }
    }

    /// The bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Hash of the whole content.
    pub fn content_hash(&self) -> u64 {
        self.hash
    }

    /// Whether both values share one allocation (cheap clones of one file).
    pub fn ptr_eq(&self, other: &ResourceBytes) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes)
    }
}

impl Default for ResourceBytes {
    fn default() -> Self {
        ResourceBytes::new(Vec::new())
    }
}

impl PartialEq for ResourceBytes {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.bytes == other.bytes
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub graphs: Vec<Graph>,
    #[serde(default)]
    pub resources: Vec<Resource>,
    /// Format version of `package.json`.
    #[serde(default = "version")]
    pub version: u32,
}

fn version() -> u32 {
    1
}

impl Package {
    pub fn new(name: &str) -> Package {
        Package { name: name.into(), graphs: Vec::new(), resources: Vec::new(), version: 1 }
    }

    pub fn graph(&self, i: usize) -> Result<&Graph> {
        self.graphs.get(i).ok_or(CoreError::NoGraph(i))
    }

    pub fn graph_mut(&mut self, i: usize) -> Result<&mut Graph> {
        self.graphs.get_mut(i).ok_or(CoreError::NoGraph(i))
    }

    pub fn graph_by_name(&self, name: &str) -> Option<(usize, &Graph)> {
        self.graphs.iter().enumerate().find(|(_, g)| g.name == name)
    }

    pub fn resource(&self, name: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.name == name)
    }

    /// A name not used by any graph: `base`, `base_1`, `base_2`…
    pub fn unique_graph_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() { "Graph" } else { base.trim() };
        if self.graph_by_name(base).is_none() {
            return base.to_string();
        }
        (1..).map(|i| format!("{base}_{i}")).find(|n| self.graph_by_name(n).is_none()).unwrap_or_else(|| base.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_same_prefix_bytes_hash_differently() {
        let a = vec![7u8; 200];
        let mut b = a.clone();
        b[150] = 8;
        let (a, b) = (ResourceBytes::new(a), ResourceBytes::new(b));
        assert_eq!(a.len(), b.len());
        assert_ne!(a.content_hash(), b.content_hash());
        assert_ne!(a, b);
        assert_eq!(ResourceBytes::default(), ResourceBytes::new(Vec::new()));
    }
}
