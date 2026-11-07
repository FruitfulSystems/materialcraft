use super::navigation::{add_node_near_selection, view_center};
use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};
use egui::Ui;
use egui_phosphor::regular as icon;
use serde_json::json;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let function = app.function_editor.is_some();
    let pattern = !function && crate::pattern::active(app);
    let embedded_function = app.function_editor.as_ref().is_some_and(|editor| editor.target != crate::function::FunctionTarget::Graph);
    let function_target = app.function_editor.as_ref().and_then(|editor| editor.command_target());
    let selection: Vec<u64> = if pattern {
        app.pattern_editor.as_ref().map(|editor| editor.selection.iter().copied().map(u64::from).collect()).unwrap_or_default()
    } else if function {
        app.function_editor.as_ref().map(|editor| editor.selection.iter().copied().map(u64::from).collect()).unwrap_or_default()
    } else {
        app.session.selection.iter().map(|id| id.0).collect()
    };
    let graph = app.session.graph;
    let owner = pattern.then(|| app.pattern_editor.as_ref().map(|editor| editor.node.0)).flatten();
    row(ui, |ui| {
        let b = button;
        if common(app, ui, selection.len(), pattern || function, || {
            let mut parameters = json!({"graph": graph, "ids": selection});
            if let Some(owner) = owner {
                parameters["pattern"] = json!(owner);
            }
            if let Some(target) = &function_target {
                parameters["function"] = json!(target);
            }
            parameters
        }) {
            app.ui.frame_all = true;
        }
        if !pattern && !embedded_function {
            ui.separator();
            if b(ui, icon::SELECTION, tr("Add frame around the selection")) {
                let parameters = if function { function_frame(app, &selection) } else { json!({}) };
                app.run("graph.frame.add", parameters);
            }
            if b(ui, icon::NOTE, tr("Add comment")) {
                app.run("graph.comment.add", json!({}));
            }
            if b(ui, icon::MAP_PIN, tr("Add navigation pin")) {
                let center = view_center(app);
                app.run("graph.pin.add", json!({"at": [center.x, center.y]}));
            }
        }

        if !pattern && !function {
            view_tools(app, ui);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if super::subgraph_navigation::can_return(app) {
                if b(ui, icon::ARROW_BEND_UP_LEFT, tr("Return to parent graph")) {
                    crate::menus::invoke(app, ui.ctx(), "ui.graph.parent");
                }
                ui.label(super::subgraph_navigation::breadcrumb(app));
            }
            if pattern {
                if app.pattern_editor.as_ref().and_then(|editor| editor.current(app)).is_some_and(|graph| graph.root.is_none()) {
                    ui.label(tr("Set a node as the root"));
                }
            } else {
                thumbnails(app, ui);
                if function {
                    if app.function_editor.as_ref().and_then(|editor| editor.current(app)).is_some_and(|graph| graph.output.is_none()) {
                        ui.label(tr("Set a node as the output"));
                    }
                } else {
                    parent_size(app, ui);
                }
            }
        });
    });
    if pattern || function {
        return;
    }
    // Row 2: quick insert of the atomic nodes and the annotations
    row(ui, |ui| {
        let b = button;
        {
            for (ic, def) in [
                (icon::IMAGE, "bitmap"),
                (icon::BEZIER_CURVE, "svg"),
                (icon::TEXT_T, "text"),
                (icon::PALETTE, "uniform"),
                (icon::STACK, "blend"),
                (icon::DROP, "blur"),
                (icon::SHUFFLE, "channels_shuffle"),
                (icon::CHART_LINE, "curve"),
                (icon::WIND, "directional_blur"),
                (icon::ARROWS_OUT_LINE_HORIZONTAL, "directional_warp"),
                (icon::RULER, "distance"),
                (icon::MOUNTAINS, "emboss"),
                (icon::GRADIENT, "gradient_map"),
                (icon::ARROWS_LEFT_RIGHT, "gradient_dynamic"),
                (icon::CIRCLE_HALF, "grayscale_conversion"),
                (icon::SWATCHES, "hsl"),
                (icon::SLIDERS_HORIZONTAL, "levels"),
                (icon::FAST_FORWARD, "motion_blur"),
                (icon::CUBE, "normal"),
                (icon::CODE, "pixel_processor"),
                (icon::FUNCTION, "value_processor"),
                (icon::DIAMOND, "sharpen"),
                (icon::ARROWS_CLOCKWISE, "transformation"),
                (icon::WAVES, "warp"),
                (icon::DICE_FIVE, "fx_map"),
                (icon::SIGN_IN, "input_grayscale"),
                (icon::PALETTE, "input_color"),
                (icon::NUMBER_CIRCLE_ONE, "input_value"),
                (icon::SIGN_OUT, "output"),
            ] {
                let label = materialcraft_engine::nodes::find(def).map_or(def, |d| d.label);
                if b(ui, ic, label) {
                    add_node_near_selection(app, def);
                }
            }
        }
        ui.separator();
        if b(ui, icon::DOT_OUTLINE, tr("Add dot (reroute)")) {
            add_node_near_selection(app, "dot");
        }
    });
}

/// Row 1 extras for the material graph: 1:1, screenshot, graph info, find, display toggles,
/// compute and clean-up
fn view_tools(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let toggle = |ui: &mut Ui, ic: &str, tip: &str, on: bool| crate::widgets::icon_button(ui, ic, tip, on, 22.0).clicked();
    if button(ui, icon::FRAME_CORNERS, tr("Actual size (1:1)")) {
        super::toolbar_tools::zoom_to(app, 1.0);
    }
    if button(ui, icon::CAMERA, tr("Save a screenshot of the graph...")) {
        super::toolbar_tools::screenshot(app, ui.ctx());
    }
    let info = crate::widgets::icon_button(ui, icon::INFO, tr("Graph information"), false, 22.0);
    egui::Popup::menu(&info).show(|ui| super::toolbar_tools::graph_info(app, ui));
    if button(ui, icon::MAGNIFYING_GLASS, tr("Find a node (Tab)")) {
        let c = view_center(app);
        super::search::open_search(app, c, None);
    }
    ui.separator();
    let timings = app.ui.prefs.node_timings;
    if toggle(ui, icon::TIMER, tr("Show cook times on nodes"), timings) {
        app.ui.prefs.node_timings = !timings;
    }
    let straight = app.ui.prefs.straight_wires;
    if toggle(
        ui,
        if straight { icon::LINE_SEGMENT } else { icon::WAVE_SINE },
        if straight { tr("Wires: straight (click for curved)") } else { tr("Wires: curved (click for straight)") },
        straight,
    ) {
        app.ui.prefs.straight_wires = !straight;
    }
    let grid_on = app.ui.prefs.show_grid;
    if toggle(ui, icon::DOTS_NINE, tr("Show the grid"), grid_on) {
        app.ui.prefs.show_grid = !grid_on;
    }
    ui.separator();
    if button(ui, icon::ARROWS_CLOCKWISE, tr("Compute outputs")) {
        app.cooker.hold = false;
        app.session.dirty = true;
    }
    if button(ui, icon::BROOM, tr("Clean up: remove unconnected dots and disabled nodes with nothing attached")) {
        super::toolbar_tools::clean_up(app);
    }
}

fn row(ui: &mut Ui, draw: impl FnOnce(&mut Ui)) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            draw(ui);
        });
    });
}

fn button(ui: &mut Ui, icon: &str, tooltip: &str) -> bool {
    crate::widgets::icon_button(ui, icon, tooltip, false, 22.0).clicked()
}

fn common(app: &mut MaterialCraftApp, ui: &mut Ui, selection_count: usize, embedded: bool, parameters: impl Fn() -> serde_json::Value) -> bool {
    let frame = button(ui, icon::CORNERS_OUT, tr("Frame all (F)"));
    let can_align = selection_count > 1;
    let stack = app.ui.stack_align;
    for (icon, mode, tooltip) in [
        (icon::ALIGN_LEFT, "left", "Align left"),
        (icon::ALIGN_TOP, "top", "Align top"),
        (icon::ALIGN_CENTER_HORIZONTAL, "hcenter", "Align centers horizontally"),
        (icon::ALIGN_CENTER_VERTICAL, "vcenter", "Align centers vertically"),
        (icon::COLUMNS, "hdistribute", "Distribute horizontally"),
        (icon::ROWS, "vdistribute", "Distribute vertically"),
    ] {
        if ui.add_enabled_ui(can_align, |ui| button(ui, icon, tr(tooltip))).inner {
            let mut parameters = parameters();
            parameters["mode"] = json!(mode);
            app.run("graph.align", parameters.clone());
            // Stack: centring the nodes also closes the gaps between them
            let also = match (stack, mode) {
                (true, "hcenter") => Some("vdistribute"),
                (true, "vcenter") => Some("hdistribute"),
                _ => None,
            };
            if let Some(mode) = also {
                let mut again = parameters.clone();
                again["mode"] = json!(mode);
                app.run("graph.align", again);
            }
        }
    }
    if !embedded {
        let r = ui.checkbox(&mut app.ui.stack_align, tr("Stack"));
        crate::widgets::name(&r, egui::Role::CheckBox, "Stack");
        r.on_hover_text(tr("Aligning the centers also closes the gaps between the nodes"));
    }
    ui.separator();
    if crate::widgets::icon_button(ui, icon::GRID_FOUR, tr("Snap to grid"), app.ui.prefs.snap_to_grid, 22.0).clicked() {
        crate::menus::invoke(app, ui.ctx(), "ui.graph.snapToGrid");
    }
    frame
}

fn thumbnails(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let thumbnails = app.ui.prefs.thumbnails;
    if crate::widgets::icon_button(ui, icon::IMAGE, tr("Show node thumbnails"), thumbnails, 22.0).clicked() {
        app.ui.prefs.thumbnails = !thumbnails;
    }
}

/// "Parent size: [1024 ▾] × [1024 ▾]"
fn parent_size(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Ok(g) = app.session.graph() else { return };

    let [w, h] = g.size;
    let mut nw = w;
    let mut nh = h;
    let combo = |ui: &mut Ui, id: &str, v: &mut u32| {
        let r = egui::ComboBox::from_id_salt(id).width(64.0).selected_text(format!("{}", 1u32 << *v)).show_ui(ui, |ui| {
            for l in 4..=13u32 {
                ui.selectable_value(v, l, format!("{}", 1u32 << l));
            }
        });
        crate::widgets::name(&r.response, egui::Role::ComboBox, if id == "parent-w" { "Parent width" } else { "Parent height" });
    };

    if crate::widgets::icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE, tr("Reset the parent size to 1024 x 1024"), false, 20.0).clicked() {
        nw = 10;
        nh = 10;
    }
    combo(ui, "parent-h", &mut nh);
    let link = app.ui.parent_link;
    if crate::widgets::icon_button(ui, if link { icon::LINK } else { icon::LINK_BREAK }, tr("Link width and height"), link, 20.0).clicked() {
        app.ui.parent_link = !link;
    }
    combo(ui, "parent-w", &mut nw);
    ui.label(egui::RichText::new(tr("Parent size:")).color(Tokens::get(ui.ctx()).text_dim));
    if app.ui.parent_link {
        if nw != w {
            nh = nw;
        } else if nh != h {
            nw = nh;
        }
    }
    if (nw, nh) != (w, h) {
        app.run("graph.size", json!({"log2": [nw, nh]}));
    }
}

fn function_frame(app: &MaterialCraftApp, selection: &[u64]) -> serde_json::Value {
    let camera = super::GraphCamera { origin: app.graph.view.min, pan: app.ui.graph_camera.pan.into(), zoom: app.ui.graph_camera.zoom };
    let bounds = app.graph.rects.iter().filter(|(id, _)| selection.contains(&id.0)).fold(egui::Rect::NOTHING, |bounds, (_, rect)| bounds.union(*rect));
    if !bounds.is_finite() {
        return json!({});
    }

    let start = camera.to_graph(bounds.min);
    let size = bounds.size() / camera.zoom;
    json!({"rect": [start.x - 24.0, start.y - 44.0, size.x + 48.0, size.y + 68.0]})
}
