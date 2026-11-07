//! The library while a function graph is being edited: the operations by category, each with
//! a text icon saying what it is, its inputs, and a tooltip. Double-click adds one.

use egui::{Align2, Color32, FontId, RichText, Ui, vec2};
use materialcraft_engine::FunctionEdit;
use materialcraft_engine::nodes::function::FunctionDefinition;

use super::op_badge;
use crate::theme::Tokens;
use crate::{MaterialCraftApp, i18n::tr};

/// The categories in the order the reference library lists them
const ORDER: [&str; 10] = ["Constant", "Variables", "Vector", "Cast", "Operator", "Logical", "Comparison", "Function", "Control", "Samplers"];

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Some(target) = app.function_editor.as_ref().and_then(|editor| editor.command_target()) else { return };
    let tk = Tokens::get(ui.ctx());

    let search = app.ui.library_search.to_ascii_lowercase();
    let operations: Vec<&'static FunctionDefinition> = target
        .available_operations(&app.session)
        .filter(|d| {
            search.is_empty()
                || d.label.to_ascii_lowercase().contains(&search)
                || d.id.contains(&search)
                || op_badge::text(d.id).to_ascii_lowercase().contains(&search)
        })
        .collect();

    let r = ui.add(egui::TextEdit::singleline(&mut app.ui.library_search).hint_text(tr("Search functions")).desired_width(f32::INFINITY));
    crate::widgets::name(&r, egui::Role::TextInput, "Search functions");
    ui.label(RichText::new(tr("Double-click an operation to add it to the function")).small().color(tk.text_dim));
    ui.add_space(2.0);

    let mut add: Option<&'static str> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mut categories: Vec<&str> = ORDER.to_vec();
        for d in &operations {
            if !categories.contains(&d.category) {
                categories.push(d.category);
            }
        }
        for category in categories {
            let rows: Vec<&&FunctionDefinition> = operations.iter().filter(|d| d.category == category).collect();
            if rows.is_empty() {
                continue;
            }
            let key = format!("fn:{category}");
            let collapsed = app.ui.library_collapsed.contains(&key) && search.is_empty();
            let header = ui.horizontal(|ui| {
                let color = op_badge::color(category);
                ui.label(RichText::new(if collapsed { egui_phosphor::regular::CARET_RIGHT } else { egui_phosphor::regular::CARET_DOWN }).color(tk.text_dim));
                ui.label(RichText::new(op_badge::icon(category)).color(color));
                ui.label(RichText::new(tr(category)).strong());
                ui.label(RichText::new(format!("{}", rows.len())).small().color(tk.text_dim));
            });
            let header = ui.interact(header.response.rect, ui.scope_id().with(&key), egui::Sense::click());
            crate::widgets::name(&header, egui::Role::TreeItem, category);
            if header.clicked() {
                if collapsed {
                    app.ui.library_collapsed.retain(|k| k != &key);
                } else {
                    app.ui.library_collapsed.push(key.clone());
                }
            }
            if collapsed {
                continue;
            }
            for d in rows {
                if row(ui, d, &tk) {
                    add = Some(d.id);
                }
            }
            ui.add_space(4.0);
        }
    });

    if let Some(operation) = add
        && let Some(mut editor) = app.function_editor.take()
    {
        let at = editor.current(app).map_or([0.0; 2], |graph| [graph.nodes.len() as f32 * 140.0, 0.0]);
        editor.edit(app, FunctionEdit::Add { operation: operation.into(), at, snap: app.ui.prefs.snap_to_grid }, false);
        app.function_editor = Some(editor);
    }
}

/// One operation: the badge, the label, its inputs; true on a double-click
fn row(ui: &mut Ui, d: &FunctionDefinition, tk: &Tokens) -> bool {
    let height = 24.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::click());
    crate::widgets::name(&resp, egui::Role::ListItem, d.label);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 4.0, tk.row_hover);
    }
    let color = op_badge::color(d.category);
    let badge = egui::Rect::from_min_size(rect.min + vec2(14.0, 3.0), vec2(46.0, height - 6.0));
    p.rect_filled(badge, 4.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 48));
    p.rect_stroke(badge, 4.0, egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 140)), egui::StrokeKind::Inside);
    p.text(badge.center(), Align2::CENTER_CENTER, op_badge::text(d.id), FontId::monospace(11.0), color);
    p.text(badge.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, tr(d.label), FontId::proportional(12.5), tk.text);
    if !d.inputs.is_empty() {
        p.text(rect.right_center() - vec2(6.0, 0.0), Align2::RIGHT_CENTER, d.inputs.join(", "), FontId::proportional(10.5), tk.text_dim);
    }
    let added = resp.double_clicked();
    resp.on_hover_ui(|ui| {
        ui.label(RichText::new(tr(d.label)).strong());
        ui.label(tr(op_badge::describe(d.id)));
        ui.label(
            RichText::new(format!(
                "{}: {}   {}: {}",
                tr("Category"),
                tr(d.category),
                tr("Inputs"),
                if d.inputs.is_empty() { "none".to_string() } else { d.inputs.join(", ") }
            ))
            .small()
            .color(tk.text_dim),
        );
    });
    added
}

#[cfg(test)]
mod tests {
    use super::op_badge;

    /// Every operation has its own badge, and every badge is drawn from glyphs the UI font has
    #[test]
    fn every_operation_has_a_badge_in_the_font() {
        let allowed = "≠≥≤√×÷−·•¬ƒ";
        for d in materialcraft_engine::nodes::function::FUNCTIONS.iter() {
            let badge = op_badge::text(d.id);
            assert_ne!(badge, "ƒ", "{} has no badge", d.id);
            for c in badge.chars() {
                assert!(c.is_ascii() || allowed.contains(c), "{}: {c:?} is not in the UI font", d.id);
            }
            assert_ne!(op_badge::describe(d.id), "A function node.", "{} has no description", d.id);
        }
    }
}
