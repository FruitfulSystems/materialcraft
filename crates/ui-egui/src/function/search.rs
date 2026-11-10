//! The node search of Function canvases (Space, Tab, a double-click, or a wire dropped on empty
//! canvas): the operations the canvas takes, then the library's functions. A search opened from a
//! wire connects the new node to it.

use super::FunctionEditor;
use super::library::FunctionLibraryDrag;
use crate::{MaterialCraftApp, graph::GraphCamera, i18n::tr, theme::Tokens};
use egui::{Pos2, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::{FunctionEdit, LibraryItem};

/// The wire a search was opened from
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchWire {
    /// Dragged from this node's output: the new node takes it on its first input
    From(u32),
    /// Dragged from this node's input slot: the new node's output feeds it
    To(u32, usize),
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionSearch {
    /// Where the new node goes, in canvas coordinates
    pub(crate) at: Pos2,
    pub(crate) text: String,
    pub(crate) sel: usize,
    pub(crate) wire: Option<SearchWire>,
    pub(crate) focus: bool,
}

impl FunctionSearch {
    pub(crate) fn at(at: Pos2, wire: Option<SearchWire>) -> Self {
        Self { at, text: String::new(), sel: 0, wire, focus: true }
    }
}

/// One search result: its label, what it is (shown dimmed on the right) and what adding it does
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) label: String,
    pub(crate) group: String,
    pub(crate) entry: FunctionLibraryDrag,
}

/// Every operation the canvas takes, then the library's functions, matching all words of `text`
/// (label, id or group), names that start with the first word first
pub(crate) fn candidates(app: &MaterialCraftApp, editor: &FunctionEditor, text: &str) -> Vec<Candidate> {
    let Some(target) = editor.command_target() else { return Vec::new() };
    let mut all: Vec<(String, Candidate)> = target
        .available_operations(&app.session)
        .map(|operation| {
            let candidate = Candidate {
                label: tr(operation.label).to_string(),
                group: tr(operation.category).to_string(),
                entry: FunctionLibraryDrag::Operation(operation.id.into()),
            };
            (operation.id.to_string(), candidate)
        })
        .collect();
    for entry in app.session.library_entries() {
        if let LibraryItem::FunctionInstance { package, graph } = &entry.item
            && entry.missing.is_none()
        {
            let group = entry.category.rsplit('/').next().unwrap_or(&entry.category).to_string();
            let candidate =
                Candidate { label: entry.label.clone(), group, entry: FunctionLibraryDrag::Instance { package: package.clone(), graph: graph.clone() } };
            all.push((graph.clone(), candidate));
        }
    }
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    let mut found: Vec<Candidate> = all
        .into_iter()
        .filter(|(id, candidate)| {
            let haystack = format!("{} {} {}", candidate.label.to_lowercase(), id.to_lowercase(), candidate.group.to_lowercase());
            words.iter().all(|word| haystack.contains(word.as_str()))
        })
        .map(|(_, candidate)| candidate)
        .collect();
    if let Some(first) = words.first() {
        found.sort_by_key(|candidate| !candidate.label.to_lowercase().starts_with(first.as_str()));
    }
    found
}

/// Add `candidate` at the search position and connect it to the wire the search came from
pub(crate) fn pick(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, search: &FunctionSearch, candidate: &Candidate) {
    let before = editor.selection.clone();
    let at = [search.at.x - crate::graph::NODE_W * 0.5, search.at.y - 20.0];
    super::library::insert(app, editor, &candidate.entry, at);
    let Some(&id) = editor.selection.first().filter(|_| editor.selection != before && editor.selection.len() == 1) else { return };

    match search.wire {
        Some(SearchWire::From(from)) => {
            // Nodes without inputs (constants, variables) take no wire
            let inputs = editor.current(app).and_then(|graph| graph.node(id)).map_or(0, |node| node.inputs.len());
            if inputs > 0 {
                editor.edit(app, FunctionEdit::Connect { from, to: id, input: 0 }, false);
            }
        }
        Some(SearchWire::To(to, input)) => editor.edit(app, FunctionEdit::Connect { from: id, to, input }, false),
        None => {}
    }
    editor.selection = vec![id];
    editor.inspected_node = Some(id);
}

pub(super) fn show(app: &mut MaterialCraftApp, editor: &mut FunctionEditor, ui: &mut Ui, camera: &GraphCamera) {
    let blocked = crate::shell::modality::Modality::capture(app, ui.ctx()).is_blocked();
    let Some(mut search) = editor.search.take() else { return };

    let tokens = Tokens::get(ui.ctx());
    let list = candidates(app, editor, &search.text);
    let area = egui::Id::unique("function-node-search");
    let mut close = false;
    let mut chosen = None;
    egui::Area::new(area).fixed_pos(camera.to_screen(search.at)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(280.0);
            if blocked {
                ui.disable();
                search.focus = true;
            }
            let mut reveal = search.focus;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon::MAGNIFYING_GLASS).color(tokens.text_dim));
                let field = ui.add(egui::TextEdit::singleline(&mut search.text).hint_text(tr("Search functions...")).desired_width(240.0));
                crate::widgets::name(&field, egui::Role::SearchInput, "Search functions");
                if search.focus && !blocked {
                    field.request_focus();
                    search.focus = false;
                }
                if field.changed() {
                    search.sel = 0;
                    reveal = true;
                }
            });
            // The list reflects the text typed this frame on the next one
            let (up, down, enter, escape) = ui.input(|input| {
                let pressed = |key| !blocked && input.key_pressed(key);
                (pressed(egui::Key::ArrowUp), pressed(egui::Key::ArrowDown), pressed(egui::Key::Enter), pressed(egui::Key::Escape))
            });
            if down {
                search.sel = (search.sel + 1).min(list.len().saturating_sub(1));
            }
            if up {
                search.sel = search.sel.saturating_sub(1);
            }
            close |= escape;
            if enter {
                chosen = list.get(search.sel).cloned();
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (index, candidate) in list.iter().enumerate() {
                    let row = ui.add(
                        egui::Button::selectable(index == search.sel, format!("{}   ", candidate.label))
                            .min_size(egui::vec2(ui.available_width(), 0.0))
                            .right_text(egui::RichText::new(&candidate.group).size(10.0).color(tokens.text_dim)),
                    );
                    if index == search.sel && (up || down || reveal) {
                        row.scroll_to_me(None);
                    }
                    if row.clicked() {
                        chosen = Some(candidate.clone());
                    }
                }
                if list.is_empty() {
                    ui.label(egui::RichText::new(tr("No matching node")).color(tokens.text_dim));
                }
            });
        });
    });
    let clicked_outside = !blocked
        && ui.input(|input| input.pointer.any_pressed())
        && ui.ctx().pointer_interact_pos().is_some_and(|pointer| !ui.ctx().layer_id_at(pointer).is_some_and(|layer| layer.id == area));
    if let Some(candidate) = chosen.filter(|_| !blocked) {
        pick(app, editor, &search, &candidate);
    } else if !close && !clicked_outside {
        editor.search = Some(search);
    }
}
