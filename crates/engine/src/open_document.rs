use crate::{AnnotationSelection, Clip, EmbeddedClipboard, MaterialPreview, Session, View2d, View3d, snapshot::Snapshot};
use materialcraft_core::{NodeId, Package};

/// Editing state retained while another SBS package is active
pub struct OpenDocument {
    pub id: u64,
    pub pkg: Package,
    pub path: Option<String>,
    pub graph: usize,
    pub modified: bool,
    selection: Vec<NodeId>,
    annotation_selection: Option<AnnotationSelection>,
    view2d: View2d,
    view3d: View3d,
    material_preview: MaterialPreview,
    load_warnings: Vec<String>,
    clipboard: Option<Clip>,
    embedded_clipboard: Option<EmbeddedClipboard>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl OpenDocument {
    pub(crate) fn edit<T>(&mut self, label: &str, edit: impl FnOnce(&mut Package) -> crate::Result<T>) -> crate::Result<T> {
        let mut package = self.pkg.clone();
        let result = edit(&mut package)?;
        let snapshot = Snapshot {
            pkg: std::mem::replace(&mut self.pkg, package),
            graph: self.graph,
            selection: self.selection.clone(),
            annotation_selection: self.annotation_selection,
            material_preview: self.material_preview.clone(),
            label: label.into(),
            modified: self.modified,
            // The active document's cook cache cannot be reused after activation
            dirty: true,
            cook_epoch: 0,
        };
        self.undo.push(snapshot);
        if self.undo.len() > crate::session::MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.modified = true;
        Ok(result)
    }

    pub(crate) fn new(id: u64, pkg: Package, path: Option<String>) -> Self {
        Self {
            id,
            material_preview: MaterialPreview::for_graph(&pkg, 0),
            pkg,
            path,
            graph: 0,
            modified: false,
            selection: Vec::new(),
            annotation_selection: None,
            view2d: View2d::default(),
            view3d: View3d::default(),
            load_warnings: Vec::new(),
            clipboard: None,
            embedded_clipboard: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub(crate) fn exchange(&mut self, session: &mut Session) {
        session.end_group();
        session.finish_edit();
        std::mem::swap(&mut self.id, &mut session.document_id);
        macro_rules! exchange {
            ($($field:ident),+ $(,)?) => { $(std::mem::swap(&mut self.$field, &mut session.$field);)+ };
        }
        exchange!(
            pkg,
            path,
            graph,
            modified,
            selection,
            annotation_selection,
            view2d,
            view3d,
            material_preview,
            load_warnings,
            clipboard,
            embedded_clipboard,
            undo,
            redo
        );
        // A worker from the previous document must never publish into this one
        session.cook_epoch = session.cook_epoch.wrapping_add(1);
        session.clear_preview_results();
        session.cooked = Default::default();
        session.clear_decoded();
        session.dirty = true;
    }
}
