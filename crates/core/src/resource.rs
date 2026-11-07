use crate::ResourceBytes;
use serde::{Deserialize, Serialize};

/// An embedded file: a bitmap a Bitmap node shows, or an SVG
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Unique name, referenced by nodes (`resource` parameter)
    pub name: String,
    /// File name inside the package's `resources/` folder
    pub file: String,
    /// The file's bytes (stored in the zip, not in `package.json`)
    #[serde(skip)]
    pub bytes: ResourceBytes,
    /// A linked resource: the file it is read from (relative to the package's folder when it
    /// lies inside it), reloaded by "Reload resources". `None` = embedded in the package
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

impl Resource {
    /// A resource named `name`, stored as `file` in the package
    pub fn new(name: impl Into<String>, file: impl Into<String>, bytes: ResourceBytes) -> Resource {
        Resource { name: name.into(), file: file.into(), bytes, link: None }
    }

    /// A resource that stays on disk at `link` and is read from there
    pub fn linked(name: impl Into<String>, file: impl Into<String>, bytes: ResourceBytes, link: impl Into<String>) -> Resource {
        Resource { name: name.into(), file: file.into(), bytes, link: Some(link.into()) }
    }
}
