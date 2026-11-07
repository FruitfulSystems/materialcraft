use egui::{Color32, RichText, Ui, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::{core::GraphKind, samples::TEMPLATES};
use serde_json::json;

use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};

pub(crate) fn show(app: &mut MaterialCraftApp, context: &egui::Context) {
    let mut open = true;
    let mut create = false;
    crate::windows::modal(context, "new-graph", tr("New graph"), &mut open, vec2(660.0, 330.0), |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.ui.new_graph_kind, GraphKind::Material, tr("Graph"));
            ui.selectable_value(&mut app.ui.new_graph_kind, GraphKind::Function, tr("Function Graph"));
        });
        ui.add_space(10.0);
        ui.horizontal_top(|ui| {
            if app.ui.new_graph_kind == GraphKind::Material {
                ui.vertical(|ui| {
                    ui.set_width(320.0);
                    ui.label(RichText::new(tr("Templates")).strong());
                    ui.horizontal(|ui| {
                        for (template, label) in TEMPLATES {
                            template_tile(app, ui, template, label);
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(tr(template_description(&app.ui.new_graph_template)));
                });
                ui.separator();
            }
            ui.vertical(|ui| {
                ui.set_width(280.0);
                ui.label(RichText::new(tr("Graph name")).strong());
                let name = ui.text_edit_singleline(&mut app.ui.new_graph_name);
                crate::widgets::name(&name, egui::Role::TextInput, "Graph name");
                if app.ui.new_graph_kind == GraphKind::Material {
                    ui.add_space(10.0);
                    dimensions(app, ui);
                }
                ui.add_space(12.0);
                ui.label(format!("{}: {}", tr("Package"), app.session.pkg.name));
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    create = ui.add(crate::widgets::primary_button(ui, tr("Create"))).clicked();
                    if ui.button(tr("Cancel")).clicked() {
                        app.ui.windows.new_graph = false;
                    }
                });
            });
        });
    });

    if create {
        let mut parameters = json!({
            "kind": app.ui.new_graph_kind,
            "template": app.ui.new_graph_template,
            "name": app.ui.new_graph_name,
            "size": app.ui.new_graph_size,
        });
        if app.ui.new_graph_kind == GraphKind::Material {
            let height = if app.ui.new_graph_link { app.ui.new_graph_size } else { app.ui.new_graph_height };
            parameters["height"] = json!(height);
        }
        if app.run("file.new", parameters).is_some() {
            app.ui.frame_all = true;
            open = false;
        }
    }
    if !open {
        app.ui.windows.new_graph = false;
    }
}

fn dimensions(app: &mut MaterialCraftApp, ui: &mut Ui) {
    if app.ui.new_graph_link {
        app.ui.new_graph_height = app.ui.new_graph_size;
    }
    ui.horizontal(|ui| {
        size_picker(ui, "Width", &mut app.ui.new_graph_size);
        let linked = app.ui.new_graph_link;
        if crate::widgets::icon_button(ui, if linked { icon::LINK } else { icon::LINK_BREAK }, tr("Link width and height"), linked, 22.0).clicked() {
            app.ui.new_graph_link = !linked;
        }
        if app.ui.new_graph_link {
            app.ui.new_graph_height = app.ui.new_graph_size;
        }
        ui.add_enabled_ui(!app.ui.new_graph_link, |ui| size_picker(ui, "Height", &mut app.ui.new_graph_height));
    });
}

fn size_picker(ui: &mut Ui, label: &str, pixels: &mut u32) {
    ui.vertical(|ui| {
        ui.label(tr(label));
        let response = egui::ComboBox::from_id_salt(label).width(100.0).selected_text(format!("{pixels} px")).show_ui(ui, |ui| {
            for size in [16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192] {
                ui.selectable_value(pixels, size, format!("{size} px"));
            }
        });
        crate::widgets::name(&response.response, egui::Role::ComboBox, label);
    });
}

fn template_description(template: &str) -> &'static str {
    match template {
        "empty" => "Start with an empty graph and add nodes from the library.",
        "pbr" => "Uniform inputs connected to six material outputs: base color, normal, roughness, metallic, height and ambient occlusion.",
        "atomics" => "An original sample combining a value input, function, pixel processor, pattern graph and blur.",
        _ => "Choose a graph template.",
    }
}

fn template_tile(app: &mut MaterialCraftApp, ui: &mut Ui, template: &str, label: &str) {
    let tokens = Tokens::get(ui.ctx());
    let selected = app.ui.new_graph_template == template;
    let (rectangle, response) = ui.allocate_exact_size(vec2(100.0, 124.0), egui::Sense::click());
    crate::widgets::name(&response, egui::Role::ListItem, label);
    if selected || response.hovered() {
        ui.painter().rect_filled(rectangle, 6.0, if selected { tokens.row_selected } else { tokens.row_hover });
    }
    if selected {
        ui.painter().rect_stroke(rectangle, 6.0, egui::Stroke::new(1.5, tokens.accent), egui::StrokeKind::Inside);
    }
    let image = egui::Rect::from_min_size(rectangle.min + vec2(10.0, 6.0), vec2(80.0, 80.0));
    if let Some(texture) = crate::windows::template_preview(app, ui.ctx(), template) {
        ui.painter().image(texture, image, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        ui.painter().circle_filled(image.center(), 36.0, tokens.row);
        ui.painter().text(image.center(), egui::Align2::CENTER_CENTER, icon::TREE_STRUCTURE, egui::FontId::proportional(28.0), tokens.text_dim);
    }
    let caption = ui.painter().layout(tr(label).to_string(), egui::FontId::proportional(11.0), tokens.text, 96.0);
    ui.painter().galley(egui::pos2(rectangle.center().x - caption.size().x * 0.5, image.bottom() + 6.0), caption, tokens.text);
    if response.clicked() {
        app.ui.new_graph_template = template.into();
    }
}
