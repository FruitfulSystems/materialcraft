use crate::{
    OpenDocument, Result, Session, cmd,
    commands::{CommandSpec, always, bad, u64p},
};
use materialcraft_core::Package;
use serde_json::{Value, json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "package.activate",
            "Open package",
            [],
            None,
            "{document} activates an open package without losing edits or history",
            always,
            |session, parameters| {
                let id = u64p(parameters, "document").ok_or_else(|| bad("package.activate", "document is required"))?;
                session.activate_document(id)?;
                Ok(json!({"document": session.document_id}))
            }
        ),
        cmd!(
            "package.close",
            "Close package",
            [],
            None,
            "{document?, discard?: bool} closes a package; modified packages require explicit discard",
            always,
            |session, parameters| {
                let id = u64p(parameters, "document").unwrap_or(session.document_id);
                let modified = session
                    .open_packages()
                    .into_iter()
                    .find(|entry| entry.0 == id)
                    .map(|entry| entry.3)
                    .ok_or_else(|| bad("package.close", "the package is no longer open"))?;
                if modified && parameters.get("discard").and_then(Value::as_bool) != Some(true) {
                    return Err(bad("package.close", "save the package or explicitly discard its edits"));
                }
                let path = session.open_packages().into_iter().find(|entry| entry.0 == id).and_then(|entry| entry.2).map(str::to_string);
                if id != session.document_id {
                    session.documents.remove(&id);
                } else if let Some(id) = session.documents.keys().next().copied() {
                    if let Some(mut document) = session.documents.remove(&id) {
                        document.exchange(session);
                    }
                } else {
                    crate::file_cmds::replace_package(session, Package::new("Untitled package"), None);
                }
                if modified && let Some(path) = path {
                    session.discard_source_changes(&path);
                }
                Ok(Value::Null)
            }
        ),
        cmd!(
            "package.closeAll",
            "Close all packages",
            [],
            None,
            "{discard?: bool} closes all packages; modified packages require explicit discard",
            always,
            |session, parameters| {
                if session.open_packages().iter().any(|entry| entry.3) && parameters.get("discard").and_then(Value::as_bool) != Some(true) {
                    return Err(bad("package.closeAll", "save the packages or explicitly discard their edits"));
                }
                session.documents.clear();
                session.linked_sources.clear();
                crate::file_cmds::replace_package(session, Package::new("Untitled package"), None);
                Ok(Value::Null)
            }
        ),
    ]
}

impl Session {
    pub fn activate_document(&mut self, id: u64) -> Result<()> {
        if id == self.document_id {
            return Ok(());
        }
        if !self.documents.contains_key(&id) {
            return Err(bad("package.activate", "the package is no longer open"));
        }
        // Publish pending source edits while the destination is still among the open dependents
        self.end_group();
        let mut document = self.documents.remove(&id).ok_or_else(|| bad("package.activate", "the package is no longer open"))?;
        document.exchange(self);
        self.documents.insert(document.id, document);
        self.refresh_links();
        Ok(())
    }

    pub(crate) fn open_package(&mut self, package: Package, path: Option<String>) -> Result<()> {
        if self.documents.len() >= 63 {
            return Err(bad("file.open", "at most 64 packages can be open at once"));
        }
        let id = self.next_document_id;
        self.next_document_id = id.checked_add(1).ok_or_else(|| bad("file.open", "document identifiers exhausted"))?;
        self.linked_sources.extend(package.dependencies.clone());
        let mut document = OpenDocument::new(id, package, path);
        document.exchange(self);
        self.documents.insert(document.id, document);
        self.refresh_links();
        Ok(())
    }

    pub fn open_packages(&self) -> Vec<(u64, &Package, Option<&str>, bool)> {
        let mut packages = vec![(self.document_id, &self.pkg, self.path.as_deref(), self.modified)];
        packages.extend(self.documents.values().map(|document| (document.id, &document.pkg, document.path.as_deref(), document.modified)));
        packages.sort_by_key(|entry| entry.0);
        packages
    }

    pub fn inspect_documents(&self) -> Value {
        json!(self.open_packages().into_iter().map(|(id, package, path, modified)| json!({
            "id": id, "name": package.name, "path": path, "modified": modified,
            "active": id == self.document_id,
            "folders": package.folder_paths(),
            "graphs": package.graphs.iter().enumerate().map(|(index, graph)| json!({"index": index, "name": graph.name, "path": graph.sbs_path(), "kind": graph.kind()})).collect::<Vec<_>>()
        })).collect::<Vec<_>>())
    }
}
