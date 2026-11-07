//! The package row's context menu (New, Import, Link, Save, Export, Send to, Copy/Paste,
//! Close, Compute, Show in Explorer, Dependency Manager)

use egui::Ui;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Draw the menu; returns the menu id picked (a `ui.`/`file.` command or an `explorer.` action)
pub(super) fn show(app: &MaterialCraftApp, ui: &mut Ui) -> Option<&'static str> {
    let mut picked: Option<&'static str> = None;
    let mut pick = |ui: &mut Ui, label: &str, id: &'static str, key: &str, enabled: bool| {
        if crate::menus::menu_row(ui, tr(label), key, enabled).clicked() {
            picked = Some(id);
            ui.close();
        }
    };
    ui.menu_button(tr("New"), |ui| {
        pick(ui, "Graph...", "ui.newGraph", "Ctrl+N", true);
        pick(ui, "Function Graph...", "ui.newFunctionGraph", "", true);
    });
    ui.menu_button(tr("Import"), |ui| {
        pick(ui, "Bitmap...", "file.importBitmap", "", true);
        pick(ui, "Graph from a package...", "file.importGraph", "", true);
    });
    ui.menu_button(tr("Link"), |ui| {
        pick(ui, "Bitmap...", "file.linkBitmap", "", true);
    });
    pick(ui, "Reload", "ui.reloadResources", "", true);
    ui.separator();
    pick(ui, "Save", "file.save", "Ctrl+S", true);
    pick(ui, "Save as...", "file.saveAs", "Ctrl+Shift+S", true);
    pick(ui, "Save a copy as...", "file.saveCopy", "", true);
    pick(ui, "Export with dependencies...", "file.exportWithDependencies", "", true);
    ui.menu_button(tr("Send to"), |ui| {
        pick(ui, "TextureCraft", "file.sendToTextureCraft", "", true);
    });
    ui.separator();
    pick(ui, "Copy", "explorer.copy", "", true);
    pick(ui, "Paste", "explorer.paste", "", app.explorer_clipboard.0.is_some());
    ui.separator();
    pick(ui, "Close package", "ui.closeAll", "", true);
    pick(ui, "Compute Outputs", "explorer.compute", "", true);
    pick(ui, "Show in Explorer...", "explorer.reveal", "", app.session.path.is_some());
    ui.separator();
    pick(ui, "Dependency Manager", "ui.view.dependencies", "", true);
    picked
}
