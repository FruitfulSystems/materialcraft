//! The library: a category tree on the left, the matching entries as preview tiles (or a list)
//! on the right, with search and favorites. Drag a tile into the graph, or double-click it to
//! add it next to the selection. Entries are nodes, graph items (frame, comment, pin), function
//! nodes (for the open function graph), the package's own graphs and loaded plugin graphs.

use egui::{Color32, Rect, RichText, Sense, Ui, vec2};
use egui_phosphor::regular as icon;

use crate::MaterialCraftApp;
use crate::graph_view::{LibraryDrag, header_color};
use crate::i18n::tr;
use crate::theme::Tokens;

/// The favorites pseudo-category.
pub const FAVORITES: &str = "@favorites";

/// Where an entry comes from, and so what adding it does.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A node of the catalog (`id` is its def).
    Node,
    /// A frame, comment or navigation pin: `id` is the engine command.
    GraphItem,
    /// A function node for the open function graph: `id` is the op.
    FnOp,
    /// A graph of the open package: `id` is its name.
    PackageGraph,
    /// A graph of a loaded plugin: `id` is the graph, `package` the file it lives in.
    PluginGraph { package: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub label: String,
    pub category: String,
    pub kind: Kind,
}

impl Entry {
    fn new(id: impl Into<String>, label: impl Into<String>, category: impl Into<String>, kind: Kind) -> Entry {
        Entry { id: id.into(), label: label.into(), category: category.into(), kind }
    }

    /// The key favorites and drags use.
    pub fn key(&self) -> String {
        match &self.kind {
            Kind::Node => self.id.clone(),
            Kind::GraphItem => format!("!{}", self.id),
            Kind::FnOp => format!("fn:{}", self.id),
            Kind::PackageGraph => format!("@{}", self.id),
            Kind::PluginGraph { package } => format!("+{package}\n{}", self.id),
        }
    }

    /// The def whose header colour the tile borrows.
    fn color_def(&self) -> &str {
        match self.kind {
            Kind::Node => &self.id,
            Kind::GraphItem => "dot",
            Kind::FnOp => "pixel_processor",
            Kind::PackageGraph | Kind::PluginGraph { .. } => "graph_instance",
        }
    }
}

/// The tree in display order: (top level, sub-categories as full paths). Groups the catalog
/// has no nodes for yet are listed too, greyed with a zero count.
pub fn tree() -> Vec<(String, Vec<String>)> {
    let planned: [(&str, &[&str]); 13] = [
        ("Graph items", &[]),
        ("Atomic nodes", &[]),
        ("FxMap nodes", &[]),
        ("Function nodes", &[]),
        ("Texture generators", &[]),
        ("Filters", &[]),
        ("Material filters", &["Material filters/1-click", "Material filters/Effects", "Material filters/Transforms", "Material filters/Scan processing"]),
        ("Mesh-based generators", &[]),
        ("Spline & Path tools", &[]),
        ("SDF functions", &[]),
        ("Functions", &[]),
        ("PBR materials", &["PBR materials/This package", "PBR materials/Plugins"]),
        ("Values", &["Values/Constant"]),
    ];
    let mut out: Vec<(String, Vec<String>)> = planned.iter().map(|(t, subs)| (t.to_string(), subs.iter().map(|s| s.to_string()).collect())).collect();
    let mut place = |cat: String| {
        let top = cat.split('/').next().unwrap_or(&cat).to_string();
        match out.iter_mut().find(|(t, _)| *t == top) {
            Some((_, subs)) => {
                if cat != top && !subs.contains(&cat) {
                    // Groups the catalog fills go before the planned (empty) ones.
                    let at = subs.iter().position(|s| planned.iter().any(|(_, p)| p.contains(&s.as_str()))).unwrap_or(subs.len());
                    subs.insert(at, cat);
                }
            }
            None => out.push((top.clone(), if cat != top { vec![cat] } else { Vec::new() })),
        }
    };
    for (cat, _) in materialcraft_engine::nodes::categories() {
        place(cat.to_string());
    }
    for (group, _) in fn_groups() {
        place(fn_path(group));
    }
    out
}

/// Function node groups in catalog order: (group, ops).
/// One function node operation of the catalog: (id, label, inputs, group).
type FnOp = &'static (&'static str, &'static str, usize, &'static str);

fn fn_groups() -> Vec<(&'static str, Vec<FnOp>)> {
    let mut out: Vec<(&'static str, Vec<FnOp>)> = Vec::new();
    for op in materialcraft_engine::nodes::function::FN_OPS {
        match out.iter_mut().find(|(c, _)| *c == op.3) {
            Some((_, ops)) => ops.push(op),
            None => out.push((op.3, vec![op])),
        }
    }
    out
}

/// The library path of a function node group: constants are values, the rest function nodes.
fn fn_path(group: &str) -> String {
    if group == "Constant" { "Values/Constant".to_string() } else { format!("Function nodes/{group}") }
}

fn category_icon(top: &str) -> &'static str {
    match top {
        "Atomic nodes" => icon::CUBE,
        "Graph items" => icon::FLOW_ARROW,
        "FxMap nodes" => icon::DICE_FIVE,
        "Function nodes" | "Values" => icon::FUNCTION,
        "Texture generators" => icon::WAVES,
        "Filters" => icon::FADERS,
        "Material filters" => icon::SPHERE,
        "Mesh-based generators" => icon::CUBE_TRANSPARENT,
        "Spline & Path tools" => icon::BEZIER_CURVE,
        "SDF functions" => icon::CIRCLES_THREE,
        "Functions" => icon::GRAPH,
        "PBR materials" => icon::PAINT_BUCKET,
        _ => icon::FOLDER_OPEN,
    }
}

/// Every entry the library can show (unfiltered).
pub fn all_entries(app: &mut MaterialCraftApp) -> Vec<Entry> {
    let mut out: Vec<Entry> = vec![
        Entry::new("graph.comment.add", tr("Comment"), "Graph items", Kind::GraphItem),
        Entry::new("graph.frame.add", tr("Frame"), "Graph items", Kind::GraphItem),
        Entry::new("graph.pin.add", tr("Pin"), "Graph items", Kind::GraphItem),
    ];
    for d in materialcraft_engine::nodes::catalog() {
        if d.id == "graph_instance" {
            continue;
        }
        let category = if d.id == "fx_map" { "FxMap nodes" } else { d.category };
        out.push(Entry::new(d.id, d.label, category, Kind::Node));
    }
    for (group, ops) in fn_groups() {
        let path = fn_path(group);
        for op in ops {
            out.push(Entry::new(op.0, op.1, path.clone(), Kind::FnOp));
        }
    }
    let open = app.session.graph;
    for (i, g) in app.session.pkg.graphs.iter().enumerate() {
        if i == open {
            continue;
        }
        let cat = match g.kind {
            materialcraft_engine::core::GraphKind::Function => "Functions",
            materialcraft_engine::core::GraphKind::Material => "PBR materials/This package",
        };
        out.push(Entry::new(&g.name, &g.name, cat, Kind::PackageGraph));
    }
    for (plugin, package, graph) in app.plugin_graphs() {
        out.push(Entry::new(&graph, format!("{graph}  ·  {plugin}"), "PBR materials/Plugins", Kind::PluginGraph { package }));
    }
    out
}

fn in_category(e: &Entry, cat: &str) -> bool {
    cat.is_empty() || e.category == cat || e.category.starts_with(&format!("{cat}/"))
}

/// Library entries for the current filter.
fn entries(app: &mut MaterialCraftApp, all: &[Entry]) -> Vec<Entry> {
    let search = app.ui.library_search.to_ascii_lowercase();
    let cat = app.ui.library_category.clone();
    let favs = &app.ui.library_favorites;
    all.iter()
        .filter(|e| if cat == FAVORITES { favs.contains(&e.key()) } else { in_category(e, &cat) })
        .filter(|e| search.is_empty() || e.label.to_ascii_lowercase().contains(&search) || e.id.to_ascii_lowercase().contains(&search))
        .cloned()
        .collect()
}

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(tk.text_dim));
        let r = ui.add(egui::TextEdit::singleline(&mut app.ui.library_search).hint_text(tr("Search")).desired_width((ui.available_width() - 130.0).max(40.0)));
        crate::widgets::name(&r, egui::Role::SearchInput, "Search the library");
        if !app.ui.library_search.is_empty() && crate::widgets::icon_button(ui, icon::X, tr("Clear search"), false, 20.0).clicked() {
            app.ui.library_search.clear();
        }
        let fav = app.ui.library_category == FAVORITES;
        if crate::widgets::icon_button(ui, icon::STAR, tr("Favorites"), fav, 20.0).clicked() {
            app.ui.library_category = if fav { String::new() } else { FAVORITES.into() };
        }
        if crate::widgets::icon_button(ui, icon::SQUARES_FOUR, tr("Icons"), !app.ui.prefs.library_list, 20.0).clicked() {
            app.ui.prefs.library_list = false;
        }
        if crate::widgets::icon_button(ui, icon::LIST, tr("List"), app.ui.prefs.library_list, 20.0).clicked() {
            app.ui.prefs.library_list = true;
        }
        if !app.ui.prefs.library_list {
            ui.add(egui::Slider::new(&mut app.ui.prefs.library_tile, 56.0..=160.0).show_value(false)).on_hover_text(tr("Tile size"));
        }
    });
    ui.add_space(2.0);
    let all = all_entries(app);
    let avail = ui.available_size();
    let tree_w = (avail.x * 0.36).clamp(90.0, 190.0);
    let top_down = egui::Layout::top_down(egui::Align::Min);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(tree_w, avail.y), top_down, |ui| {
            egui::ScrollArea::vertical().id_salt("lib-tree").auto_shrink([false, false]).show(ui, |ui| category_tree(app, ui, &all));
        });
        ui.separator();
        ui.allocate_ui_with_layout(vec2(ui.available_width(), avail.y), top_down, |ui| {
            egui::ScrollArea::vertical().id_salt("lib-items").auto_shrink([false, false]).show(ui, |ui| items(app, ui, &all));
        });
    });
}

fn category_tree(app: &mut MaterialCraftApp, ui: &mut Ui, all: &[Entry]) {
    let tk = Tokens::get(ui.ctx());
    let count = |cat: &str| all.iter().filter(|e| in_category(e, cat)).count();
    let mut select: Option<String> = None;
    let mut toggle: Option<String> = None;
    let mut row = |ui: &mut Ui, text: String, path: &str, indent: f32, n: Option<usize>, arrow: Option<bool>| {
        let on = app.ui.library_category == path;
        let empty = n == Some(0);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
        crate::widgets::name(&resp, egui::Role::TreeItem, &text);
        if on {
            ui.painter().rect_filled(r, 4.0, tk.row_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, tk.row_hover);
        }
        let color = if empty {
            tk.text_dim
        } else if on {
            tk.text_strong
        } else {
            tk.text
        };
        let mut arrow_w = 0.0;
        if let Some(open) = arrow {
            arrow_w = 14.0;
            let a = Rect::from_min_size(r.min + vec2(indent - 2.0, 0.0), vec2(14.0, r.height()));
            ui.painter().text(
                a.center(),
                egui::Align2::CENTER_CENTER,
                if open { icon::CARET_DOWN } else { icon::CARET_RIGHT },
                egui::FontId::proportional(10.0),
                tk.text_dim,
            );
            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| a.contains(p)) {
                toggle = Some(path.to_string());
            } else if resp.clicked() {
                select = Some(path.to_string());
            }
        } else if resp.clicked() {
            select = Some(path.to_string());
        }
        let label = match n {
            Some(n) if n == 0 || indent > 10.0 => format!("{text}  ({n})"),
            _ => text,
        };
        let g = crate::widgets::elided(ui.painter(), label, 11.5, color, r.width() - indent - arrow_w - 4.0);
        ui.painter().galley(r.left_center() + vec2(indent + arrow_w, -g.size().y * 0.5), g, color);
        if resp.double_clicked() && arrow.is_some() {
            toggle = Some(path.to_string());
        }
    };
    row(ui, format!("{}  {}", icon::SQUARES_FOUR, tr("All nodes")), "", 4.0, None, None);
    row(ui, format!("{}  {}", icon::STAR, tr("Favorites")), FAVORITES, 4.0, Some(app.ui.library_favorites.len()), None);
    for (top, subs) in tree() {
        let open = !app.ui.library_collapsed.contains(&top);
        let n = count(&top);
        row(ui, format!("{}  {}", category_icon(&top), tr(&top)), &top, 4.0, Some(n), if subs.is_empty() { None } else { Some(open) });
        if open {
            for sub in subs {
                let name = sub.rsplit('/').next().unwrap_or(&sub);
                row(ui, tr(name).to_string(), &sub, 24.0, Some(count(&sub)), None);
            }
        }
    }
    if let Some(p) = select {
        app.ui.library_category = p;
    }
    if let Some(t) = toggle {
        if app.ui.library_collapsed.contains(&t) {
            app.ui.library_collapsed.retain(|c| *c != t);
        } else {
            app.ui.library_collapsed.push(t);
        }
    }
}

fn items(app: &mut MaterialCraftApp, ui: &mut Ui, all: &[Entry]) {
    let tk = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let list = entries(app, all);
    if list.is_empty() {
        let hint = if app.ui.library_category == FAVORITES {
            tr("No favorites yet: right-click a tile and choose Add to favorites")
        } else if app.ui.library_category.starts_with("PBR materials/Plugins") {
            tr("No plugin graphs: load a plugin in Tools ▸ Plugin Manager")
        } else if app.ui.library_search.is_empty() {
            tr("Nothing here yet")
        } else {
            tr("Nothing matches")
        };
        ui.label(RichText::new(hint).color(tk.text_dim));
        return;
    }
    let as_list = app.ui.prefs.library_list;
    let side = app.ui.prefs.library_tile.clamp(56.0, 160.0);
    let tile = vec2(side, side + 20.0);
    let cols = if as_list { 1 } else { ((ui.available_width() + 4.0) / (tile.x + 4.0)).floor().max(1.0) as usize };
    let mut add: Option<Entry> = None;
    let mut star: Option<(String, bool)> = None;
    for chunk in list.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for e in chunk {
                let size = if as_list { vec2(ui.available_width(), 26.0) } else { tile };
                let (r, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
                crate::widgets::name(&resp, egui::Role::ListItem, &e.label);
                let def_id = e.color_def();
                if resp.hovered() {
                    ui.painter().rect_filled(r, 6.0, tk.row_hover);
                }
                let tex = if e.kind == Kind::Node { app.thumbs.library(&ctx, &e.id) } else { None };
                let img_rect = if as_list {
                    Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2(22.0, 22.0))
                } else {
                    Rect::from_min_size(r.min + vec2(5.0, 4.0), vec2(side - 10.0, side - 10.0))
                };
                match tex {
                    Some(t) => {
                        ui.painter().image(t, img_rect, Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        ui.painter().rect_filled(img_rect, 4.0, header_color(def_id).gamma_multiply(0.7));
                        let glyph = match &e.kind {
                            Kind::Node => icon::CUBE,
                            Kind::GraphItem => icon::NOTE_PENCIL,
                            Kind::FnOp => icon::FUNCTION,
                            Kind::PackageGraph => icon::TREE_STRUCTURE,
                            Kind::PluginGraph { .. } => icon::PUZZLE_PIECE,
                        };
                        ui.painter().text(
                            img_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            egui::FontId::proportional(img_rect.height() * 0.4),
                            Color32::WHITE,
                        );
                    }
                }
                // The category color as a strip, like the node headers.
                let strip = Rect::from_min_size(img_rect.left_bottom() - vec2(0.0, 3.0), vec2(img_rect.width(), 3.0));
                ui.painter().rect_filled(strip, 0.0, header_color(def_id));
                let key = e.key();
                let starred = app.ui.library_favorites.contains(&key);
                if starred {
                    ui.painter().text(
                        img_rect.right_top() + vec2(-3.0, 3.0),
                        egui::Align2::RIGHT_TOP,
                        icon::STAR,
                        egui::FontId::proportional(11.0),
                        Color32::from_rgb(0xf5, 0xc5, 0x42),
                    );
                }
                if as_list {
                    let g = crate::widgets::elided(ui.painter(), e.label.clone(), 11.5, tk.text, r.width() - 34.0);
                    ui.painter().galley(r.left_center() + vec2(30.0, -g.size().y * 0.5), g, tk.text);
                } else {
                    let galley = ui.painter().layout(e.label.clone(), egui::FontId::proportional(10.5), tk.text, tile.x - 4.0);
                    let pos = egui::pos2(r.center().x - galley.size().x * 0.5, img_rect.bottom() + 3.0);
                    ui.painter().galley(pos, galley, tk.text);
                }
                let hint = match &e.kind {
                    Kind::FnOp => format!("{}\n{}\n{}", e.label, e.category, tr("Function node: for the open function graph")),
                    Kind::PluginGraph { package } => format!("{}\n{}\n{package}", e.label, e.category),
                    _ => format!("{}\n{}", e.label, e.category),
                };
                let resp = resp.on_hover_text(hint);
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(&ctx, LibraryDrag(key.clone()));
                }
                if resp.double_clicked() {
                    add = Some(e.clone());
                }
                resp.context_menu(|ui| {
                    if ui.button(tr("Add to graph")).clicked() {
                        add = Some(e.clone());
                        ui.close();
                    }
                    let label = if starred { tr("Remove from favorites") } else { tr("Add to favorites") };
                    if ui.button(label).clicked() {
                        star = Some((key.clone(), !starred));
                        ui.close();
                    }
                });
            }
        });
    }
    if let Some((key, on)) = star {
        app.ui.library_favorites.retain(|k| *k != key);
        if on {
            app.ui.library_favorites.push(key);
            app.ui.library_favorites.truncate(500);
        }
    }
    if let Some(e) = add {
        add_entry(app, &e, None);
    }
}

/// Add a library entry to the graph (double-click, context menu), or at `at` (a drop).
pub fn add_entry(app: &mut MaterialCraftApp, e: &Entry, at: Option<[f32; 2]>) {
    match &e.kind {
        Kind::Node => match at {
            Some(at) => {
                app.run("graph.node.add", serde_json::json!({"def": e.id, "at": at}));
            }
            None => crate::graph_view::add_node_near_selection(app, &e.id),
        },
        Kind::GraphItem => add_graph_item(app, &e.id, at),
        Kind::FnOp => {
            if !crate::function_editor::add_op(app, &e.id) {
                app.toast(tr("Function nodes go into a function graph: open one from a Pixel Processor or a function parameter first"), true);
            }
        }
        Kind::PackageGraph => {
            let mut p = serde_json::json!({"def": "graph_instance", "params": {"graph": e.id}});
            if let Some(at) = at {
                p["at"] = serde_json::json!(at);
            }
            app.run("graph.node.add", p);
        }
        Kind::PluginGraph { package } => app.add_plugin_graph(package, &e.id, at),
    }
}

/// A frame (around the selection, or an empty one at `at`), a comment or a pin.
fn add_graph_item(app: &mut MaterialCraftApp, cmd: &str, at: Option<[f32; 2]>) {
    let c = crate::graph_view::view_center(app);
    let at = at.unwrap_or([c.x - 60.0, c.y - 20.0]);
    let params = match cmd {
        "graph.frame.add" if app.session.selection.is_empty() => serde_json::json!({"rect": [at[0], at[1], 320.0, 200.0]}),
        "graph.frame.add" => serde_json::json!({}),
        _ => serde_json::json!({"at": at}),
    };
    app.run(cmd, params);
}

/// Add whatever a library drag key names at `at` (the graph view's drop handler).
pub fn add_key(app: &mut MaterialCraftApp, key: &str, at: [f32; 2]) {
    if let Some(cmd) = key.strip_prefix('!') {
        add_graph_item(app, cmd, Some(at));
    } else if let Some(op) = key.strip_prefix("fn:") {
        if !crate::function_editor::add_op(app, op) {
            app.toast(tr("Function nodes go into a function graph: open one from a Pixel Processor or a function parameter first"), true);
        }
    } else if let Some(plugin) = key.strip_prefix('+') {
        if let Some((pkg, graph)) = plugin.split_once('\n') {
            app.add_plugin_graph(pkg, graph, Some(at));
        }
    } else if let Some(graph) = key.strip_prefix('@') {
        app.run("graph.node.add", serde_json::json!({"def": "graph_instance", "at": at, "params": {"graph": graph}}));
    } else {
        app.run("graph.node.add", serde_json::json!({"def": key, "at": at}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_lists_every_catalog_category_once() {
        let t = tree();
        for (cat, _) in materialcraft_engine::nodes::categories() {
            let top = cat.split('/').next().unwrap_or(cat);
            let hits = t.iter().filter(|(c, subs)| c == top && (cat == top || subs.iter().any(|s| s == cat))).count();
            assert_eq!(hits, 1, "{cat} appears once");
        }
        let tops: Vec<_> = t.iter().map(|(c, _)| c.clone()).collect();
        let mut dedup = tops.clone();
        dedup.dedup();
        assert_eq!(tops, dedup, "no duplicate top-level groups");
    }

    #[test]
    fn keys_round_trip_through_the_drop_handler_prefixes() {
        let plugin = Entry::new("Rust", "Rust", "PBR materials/Plugins", Kind::PluginGraph { package: "C:/p/x.sbs".into() });
        assert_eq!(plugin.key(), "+C:/p/x.sbs\nRust");
        assert_eq!(Entry::new("blur", "Blur", "Filters/Blurs", Kind::Node).key(), "blur");
        assert_eq!(Entry::new("Base", "Base", "PBR materials/This package", Kind::PackageGraph).key(), "@Base");
        assert_eq!(Entry::new("add", "Add", "Function nodes/Operator", Kind::FnOp).key(), "fn:add");
        assert_eq!(Entry::new("graph.pin.add", "Pin", "Graph items", Kind::GraphItem).key(), "!graph.pin.add");
    }
}
