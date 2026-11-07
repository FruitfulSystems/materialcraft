//! File ▸ New ▸ Graph: template tiles by category (the starters, and the graphs of loaded
//! library packages), a template description, the name, a linked width/height and the package
//! the graph goes into

use egui::{Color32, RichText, vec2};
use egui_phosphor::regular as icon;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// One tile of the dialog
#[derive(Clone, Debug, PartialEq)]
struct Template {
    /// A starter template id, or `+<package>\n<graph>` for a library package graph
    id: String,
    label: String,
    group: String,
    info: String,
}

/// What a starter template holds (the ⓘ on its tile)
fn starter_info(id: &str) -> &'static str {
    match id {
        "empty" => "A graph with no nodes: start from the library.",
        "pbr" => {
            "The material outputs (Base Color, Normal, Roughness, Metallic, Height, Ambient Occlusion) with uniform colors wired in; the normal is derived from the height."
        }
        "atomics" => "The atomic sample: a value input, a function, a pixel processor, an FX-Map pattern and a blur, with an image and a value output.",
        _ => "A graph template.",
    }
}

fn templates(app: &mut MaterialCraftApp) -> Vec<Template> {
    let mut out: Vec<Template> = materialcraft_engine::samples::TEMPLATES
        .iter()
        .map(|(id, label)| Template { id: id.to_string(), label: label.to_string(), group: "Starter".into(), info: starter_info(id).into() })
        .collect();
    for (plugin, package, graph, category) in app.plugin_graphs() {
        let group = if category.trim().is_empty() { plugin.clone() } else { category.clone() };
        out.push(Template {
            id: format!("+{package}\n{graph}"),
            label: graph.clone(),
            group,
            info: format!(
                "{} {plugin}: {}",
                tr("From the library package"),
                tr("the graph is copied into your package with the graphs it instances and the bitmaps it uses.")
            ),
        });
    }
    out
}

pub(crate) fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    use materialcraft_engine::core::GraphKind;
    let mut open = true;
    let mut create = false;
    let list = templates(app);
    let groups: Vec<String> = {
        let mut g: Vec<String> = Vec::new();
        for t in &list {
            if !g.contains(&t.group) {
                g.push(t.group.clone());
            }
        }
        g
    };
    crate::windows::modal(ctx, "new-graph", tr("New graph"), &mut open, vec2(700.0, 520.0), |ui| {
        let tk = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.ui.new_graph_kind, GraphKind::Material, tr("Graph"));
            ui.selectable_value(&mut app.ui.new_graph_kind, GraphKind::Function, tr("Function Graph"));
        });
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            if app.ui.new_graph_kind == GraphKind::Material {
                ui.vertical(|ui| {
                    ui.set_width(430.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Category")).strong());
                        let shown = if app.ui.new_graph_category.is_empty() { tr("All").to_string() } else { tr(&app.ui.new_graph_category).to_string() };
                        let r = egui::ComboBox::from_id_salt("new-category").selected_text(shown).show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.ui.new_graph_category, String::new(), tr("All"));
                            for g in &groups {
                                ui.selectable_value(&mut app.ui.new_graph_category, g.clone(), tr(g));
                            }
                        });
                        crate::widgets::name(&r.response, egui::Role::ComboBox, "Category");
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for group in &groups {
                            if !app.ui.new_graph_category.is_empty() && app.ui.new_graph_category != *group {
                                continue;
                            }
                            ui.label(RichText::new(tr(group)).small().strong().color(tk.text_dim));
                            let tiles: Vec<&Template> = list.iter().filter(|t| t.group == *group).collect();
                            for row in tiles.chunks(4) {
                                ui.horizontal(|ui| {
                                    for t in row {
                                        tile(app, ui, t, &mut create);
                                    }
                                });
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
                ui.separator();
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(tr("Graph name")).strong());
                let name = ui.text_edit_singleline(&mut app.ui.new_graph_name);
                crate::widgets::name(&name, egui::Role::TextInput, "Graph name");
                if app.ui.new_graph_kind == GraphKind::Material {
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr("Size")).strong());
                    sizes(app, ui);
                }
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Create graph in")).strong());
                let r = egui::ComboBox::from_id_salt("new-package").selected_text(&app.session.pkg.name).show_ui(ui, |ui| {
                    let _ = ui.selectable_label(true, &app.session.pkg.name);
                });
                crate::widgets::name(&r.response, egui::Role::ComboBox, "Create graph in");
                ui.label(RichText::new(tr("The open package (File > New > Empty package starts another)")).small().color(tk.text_dim));
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    if ui.add(crate::widgets::primary_button(ui, tr("Create"))).clicked() {
                        create = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        app.ui.windows.new_graph = false;
                    }
                });
            });
        });
    });
    if create && self::create(app) {
        open = false;
    }
    if !open {
        app.ui.windows.new_graph = false;
    }
}

/// Width and height pickers with a link between them
fn sizes(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let sizes = [16u32, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192];
    ui.horizontal(|ui| {
        let r = egui::ComboBox::from_id_salt("new-size").width(90.0).selected_text(format!("{} px", app.ui.new_graph_size)).show_ui(ui, |ui| {
            for s in sizes {
                ui.selectable_value(&mut app.ui.new_graph_size, s, format!("{s} px"));
            }
        });
        crate::widgets::name(&r.response, egui::Role::ComboBox, "Size");
        let link = app.ui.new_graph_link;
        if crate::widgets::icon_button(ui, if link { icon::LINK } else { icon::LINK_BREAK }, tr("Link width and height"), link, 20.0).clicked() {
            app.ui.new_graph_link = !link;
            if !app.ui.new_graph_link && app.ui.new_graph_height == 0 {
                app.ui.new_graph_height = app.ui.new_graph_size;
            }
        }
        let height = if app.ui.new_graph_link || app.ui.new_graph_height == 0 { app.ui.new_graph_size } else { app.ui.new_graph_height };
        ui.add_enabled_ui(!app.ui.new_graph_link, |ui| {
            let r = egui::ComboBox::from_id_salt("new-height").width(90.0).selected_text(format!("{height} px")).show_ui(ui, |ui| {
                for s in sizes {
                    ui.selectable_value(&mut app.ui.new_graph_height, s, format!("{s} px"));
                }
            });
            crate::widgets::name(&r.response, egui::Role::ComboBox, "Height");
        });
    });
    ui.label(RichText::new(tr("width × height")).small().color(Tokens::get(ui.ctx()).text_dim));
}

fn tile(app: &mut MaterialCraftApp, ui: &mut egui::Ui, t: &Template, create: &mut bool) {
    let tk = Tokens::get(ui.ctx());
    let on = app.ui.new_graph_template == t.id;
    let (r, resp) = ui.allocate_exact_size(vec2(100.0, 124.0), egui::Sense::click());
    crate::widgets::name(&resp, egui::Role::ListItem, &t.label);
    if on {
        ui.painter().rect_filled(r, 6.0, tk.row_selected);
        ui.painter().rect_stroke(r, 6.0, egui::Stroke::new(1.5, tk.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 6.0, tk.row_hover);
    }
    let img = egui::Rect::from_min_size(r.min + vec2(10.0, 6.0), vec2(80.0, 80.0));
    let preview = if t.id.starts_with('+') { None } else { crate::windows::template_preview(ui.ctx(), &t.id) };
    match preview {
        Some(tex) => {
            ui.painter().image(tex, img, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            ui.painter().circle_filled(img.center(), 36.0, tk.row);
            ui.painter().text(
                img.center(),
                egui::Align2::CENTER_CENTER,
                if t.id.starts_with('+') { icon::PUZZLE_PIECE } else { icon::TREE_STRUCTURE },
                egui::FontId::proportional(28.0),
                tk.text_dim,
            );
        }
    }
    // ⓘ in the corner: what the template holds
    let info = egui::Rect::from_center_size(img.right_top() + vec2(-8.0, 8.0), vec2(16.0, 16.0));
    let info_resp = ui.interact(info, resp.id.with("info"), egui::Sense::hover());
    ui.painter().text(
        info.center(),
        egui::Align2::CENTER_CENTER,
        icon::INFO,
        egui::FontId::proportional(13.0),
        if info_resp.hovered() { tk.text_strong } else { tk.text_dim },
    );
    info_resp.on_hover_text(tr(&t.info));
    let g = ui.painter().layout(tr(&t.label).to_string(), egui::FontId::proportional(11.0), tk.text, 96.0);
    ui.painter().galley(egui::pos2(r.center().x - g.size().x * 0.5, img.bottom() + 6.0), g, tk.text);
    if resp.clicked() {
        app.ui.new_graph_template = t.id.clone();
        app.ui.new_graph_name = t.label.replace(' ', "_");
    }
    if resp.double_clicked() {
        *create = true;
    }
}

/// Make the graph; true when it was created
fn create(app: &mut MaterialCraftApp) -> bool {
    use materialcraft_engine::core::GraphKind;
    let template = app.ui.new_graph_template.clone();
    if app.ui.new_graph_kind == GraphKind::Material
        && let Some(plugin) = template.strip_prefix('+')
        && let Some((package, graph)) = plugin.split_once('\n')
    {
        let Some(r) = app.run("file.importGraph", json!({"path": package, "graph": graph})) else { return false };
        let name = r["name"].as_str().unwrap_or(graph).to_string();
        let wanted = app.ui.new_graph_name.trim().to_string();
        if !wanted.is_empty() && wanted != name {
            app.run("graph.rename", json!({"graph": name, "name": wanted}));
        }
        app.ui.frame_all = true;
        return true;
    }
    let template = if materialcraft_engine::samples::TEMPLATES.iter().any(|(id, _)| *id == template) { template } else { "empty".to_string() };
    let parameters = json!({"kind": app.ui.new_graph_kind, "template": template, "name": app.ui.new_graph_name, "size": app.ui.new_graph_size});
    if app.run("file.new", parameters).is_none() {
        return false;
    }
    if app.ui.new_graph_kind == GraphKind::Material
        && !app.ui.new_graph_link
        && app.ui.new_graph_height != 0
        && app.ui.new_graph_height != app.ui.new_graph_size
    {
        app.run("graph.size", json!({"width": app.ui.new_graph_size, "height": app.ui.new_graph_height}));
    }
    app.ui.frame_all = true;
    true
}
