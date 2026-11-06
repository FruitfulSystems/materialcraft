//! Compact widgets matching the app's look: labelled slim sliders, icon buttons, section
//! headers, channel toggle chips and color swatches.

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use egui::{Color32, CornerRadius, Response, RichText, Sense, Stroke, Ui, vec2};

use crate::theme::Tokens;

/// Give a custom widget an accessible name and role (screen readers, AccessKit, tests).
pub fn name(resp: &Response, role: egui::Role, label: &str) {
    let label = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(role, true, label.clone()));
}

/// The drag-and-drop payload of type `T` released on `resp` this frame. Unlike
/// [`Response::dnd_release_payload`] (which clears a payload of any type), it leaves payloads
/// of other types for the next drop target.
pub fn dnd_release<T: std::any::Any + Send + Sync>(resp: &Response) -> Option<std::sync::Arc<T>> {
    resp.dnd_hover_payload::<T>().and_then(|_| resp.dnd_release_payload::<T>())
}

/// A labelled slider in two rows: "Label ........ value" over a slim track with a round knob.
/// Returns the track response (`.changed()` on a drag or valid numeric edit).
pub fn slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, decimals: usize) -> Response {
    let tk = Tokens::get(ui.ctx());
    let (lo, hi) = (*range.start(), *range.end());
    let w = ui.available_width();
    let mut edited = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(tk.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let id = ui.next_auto_id();
            // Keep incomplete input (such as "-") across frames while editing.
            let draft = id.with("numeric-draft");
            let editing = ui.memory(|m| m.has_focus(id) || m.had_focus_last_frame(id));
            let mut text = editing.then(|| ui.data(|d| d.get_temp::<String>(draft))).flatten().unwrap_or_else(|| format!("{:.*}", decimals, value));
            let r = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(44.0).align(egui::Align2::RIGHT_CENTER).frame(egui::Frame::NONE));
            name(&r, egui::Role::TextInput, label);
            if (r.changed() || r.lost_focus())
                && !ui.input(|i| i.key_pressed(egui::Key::Escape))
                && let Ok(v) = text.trim().parse::<f32>()
                && v.is_finite()
            {
                let v = v.clamp(lo, hi);
                edited = v != *value;
                *value = v;
            }
            let focused = r.has_focus();
            ui.data_mut(|d| {
                if focused {
                    d.insert_temp(draft, text);
                } else {
                    d.remove::<String>(draft);
                }
            });
        });
    });
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 14.0), Sense::click_and_drag());
    if edited {
        resp.mark_changed();
    }
    let (v, l) = (*value as f64, label.to_string());
    resp.widget_info(move || egui::WidgetInfo::slider(true, v, l.clone()));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let value_at = |x: f32| {
            let t = ((x - rect.left() - 5.0) / (rect.width() - 10.0).max(1.0)).clamp(0.0, 1.0);
            f64::from(lo) + (f64::from(hi) - f64::from(lo)) * f64::from(t)
        };
        // Use the same round-value aiming as egui's standard sliders, within one physical pixel.
        let radius = ui.input(|i| i.aim_radius());
        let v = egui::emath::smart_aim::best_in_range_f64(value_at(p.x - radius), value_at(p.x + radius)) as f32;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    track(ui, &resp, rect.left() + 5.0..=rect.right() - 5.0, rect.center().y, t, 2.0, 4.5);
    resp
}

/// A small inline slider for the top tool bar: "Label value" above a short track.
pub fn bar_slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> Response {
    let tk = Tokens::get(ui.ctx());
    let (lo, hi) = (*range.start(), *range.end());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click_and_drag());
    let (v, l) = (*value as f64, label.to_string());
    resp.widget_info(move || egui::WidgetInfo::slider(true, v, l.clone()));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let t = ((p.x - rect.left() - 4.0) / (rect.width() - 8.0).max(1.0)).clamp(0.0, 1.0);
        let v = lo + (hi - lo) * t;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    let font = egui::FontId::proportional(11.0);
    p.text(rect.left_top() + vec2(2.0, 2.0), egui::Align2::LEFT_TOP, label, font.clone(), tk.text);
    p.text(rect.right_top() + vec2(-2.0, 2.0), egui::Align2::RIGHT_TOP, format!("{:.0}", value), font, tk.text_strong);
    let t = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    track(ui, &resp, rect.left() + 4.0..=rect.right() - 4.0, rect.bottom() - 7.0, t, 1.5, 4.0);
    resp
}

/// A slider track from `x` at height `y`: the rail, the accent up to fraction `t` and a round knob.
fn track(ui: &Ui, resp: &Response, x: std::ops::RangeInclusive<f32>, y: f32, t: f32, width: f32, knob: f32) {
    let tk = Tokens::get(ui.ctx());
    let p = ui.painter();
    let (x0, x1) = (*x.start(), *x.end());
    let xk = x0 + (x1 - x0) * t;
    p.line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], Stroke::new(width, tk.button_border));
    p.line_segment([egui::pos2(x0, y), egui::pos2(xk, y)], Stroke::new(width, tk.accent));
    let ring = if resp.hovered() || resp.dragged() { tk.text_strong } else { tk.text };
    p.circle(egui::pos2(xk, y), knob, tk.panel, Stroke::new(1.5, ring));
}

/// One line of `text` cut with "…" to fit `max_width`, for list rows that shrink with their panel.
pub fn elided(painter: &egui::Painter, text: impl Into<String>, size: f32, color: Color32, max_width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.into(), egui::FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    painter.layout_job(job)
}

/// A square icon button (Phosphor glyph) in a rounded tool well: `selected` sits in the accent
/// tinted well (the shared Craft icon button, `craft_ui::buttons`).
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool, size: f32) -> Response {
    let tk = Tokens::get(ui.ctx());
    let well = |fill, icon| ButtonVisuals { fill, stroke: Stroke::NONE, icon };
    let style = IconButtonStyle {
        normal: well(Color32::TRANSPARENT, tk.text),
        hovered: well(tk.row_hover, tk.text_strong),
        pressed: Some(well(tk.tool_active, tk.text_strong)),
        selected: well(tk.tool_active, tk.text_strong),
        corner_radius: CornerRadius::same(6),
        focus_stroke: Stroke::new(1.0, tk.accent),
    };
    IconButton::new(tooltip, vec2(size, size), &style)
        .selected(selected)
        .show(ui, |ui, rect, color| {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.62), color);
        })
        .on_hover_text(tooltip)
}

/// A flat button filled with the accent, white lettering (a dialog's main action).
pub fn primary_button<'a>(ui: &Ui, text: &str) -> egui::Button<'a> {
    let tk = Tokens::get(ui.ctx());
    egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(tk.accent_strong).stroke(Stroke::NONE)
}

/// A labelled dropdown on one row: the label, then the box filling the rest of the row, so a
/// narrow panel shortens the box (its text truncated) instead of pushing the row past its edge.
pub fn combo(ui: &mut Ui, label: &str, selected: impl Into<egui::WidgetText>, menu: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(label);
        let r = egui::ComboBox::from_id_salt(label)
            .width(ui.available_width().max(60.0))
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(selected)
            .show_ui(ui, menu);
        name(&r.response, egui::Role::ComboBox, label);
    });
}

/// A collapsible section header with a chevron ("> Size").
pub fn section(ui: &mut Ui, id: &str, title: &str, default_open: bool, body: impl FnOnce(&mut Ui)) {
    let tk = Tokens::get(ui.ctx());
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    let open = state.is_open();
    let resp = ui
        .horizontal(|ui| {
            let chev = if open { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
            ui.label(RichText::new(chev).size(10.0).color(tk.text_dim));
            ui.label(RichText::new(title).strong().color(tk.text_strong));
        })
        .response
        .interact(Sense::click());
    if resp.clicked() {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
        ui.indent(id.with("in"), body);
    });
}

/// A channel chip ("color", "metal", "rough"…): toggled = the accent-tinted well.
pub fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let tk = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(text.into(), egui::FontId::proportional(11.0), tk.text);
    let size = vec2(galley.size().x + 14.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    name(&resp, egui::Role::Button, text);
    let p = ui.painter();
    let fill = if on {
        tk.tool_active
    } else if resp.hovered() {
        tk.row_hover
    } else {
        tk.row
    };
    p.rect_filled(rect, 5.0, fill);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), if on { tk.text_strong } else { tk.text_dim });
    resp
}

/// A color swatch button for a linear RGB value; opens a picker on click. Returns true when
/// the color changed.
pub fn color_swatch(ui: &mut Ui, id: &str, linear: &mut [f32; 3], width: f32) -> bool {
    let tk = Tokens::get(ui.ctx());
    let srgb = linear.map(|v| (materialcraft_engine::image::linear_to_srgb(v) * 255.0 + 0.5) as u8);
    let mut c = Color32::from_rgb(srgb[0], srgb[1], srgb[2]);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
    name(&resp, egui::Role::ColorWell, id);
    ui.painter().rect(rect, 4.0, c, Stroke::new(1.0, tk.field_border), egui::StrokeKind::Inside);
    let popup = ui.make_persistent_id(id);
    if resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), popup);
    }
    let mut changed = false;
    // Open state lives in memory (`toggle_id` above); `from_response` alone shows it always.
    egui::Popup::from_response(&resp).id(popup).open_memory(None).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            let lin = [c.r(), c.g(), c.b()].map(|v| materialcraft_engine::image::srgb_to_linear(v as f32 / 255.0));
            *linear = lin;
            changed = true;
        }
    });
    changed
}

/// Panel title row with optional trailing content.
pub fn panel_header(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.label(crate::theme::caption(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
    });
}

/// How the colour strip's channel sliders read: sRGB 0..255, linear floats or HSV.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ColorMode {
    #[default]
    Srgb,
    Float,
    Hsv,
}

/// A colour row as the 3D view's property pages show it: a swatch (click opens the picker), a
/// hue strip (click or drag sets the hue), then sRGB / Float / HSV sliders for the channels.
/// `linear` is linear RGB; `alpha` adds a fourth slider. Returns true when the colour changed.
pub fn color_strip(ui: &mut Ui, id: &str, linear: &mut [f32; 3], alpha: Option<&mut f32>) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mode_id = ui.make_persistent_id((id, "color-mode"));
    let mut mode: ColorMode = ui.data(|d| d.get_temp(mode_id)).unwrap_or_default();
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= color_swatch(ui, id, linear, 22.0);
        // Hue strip.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(60.0), 18.0), Sense::click_and_drag());
        name(&resp, egui::Role::Slider, "Hue");
        let steps = 24;
        for i in 0..steps {
            let (a, b) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
            let r = egui::Rect::from_min_max(
                egui::pos2(rect.left() + rect.width() * a, rect.top()),
                egui::pos2(rect.left() + rect.width() * b + 0.5, rect.bottom()),
            );
            let c = egui::ecolor::Hsva::new(a + 0.5 / steps as f32, 1.0, 1.0, 1.0);
            ui.painter().rect_filled(r, 0.0, Color32::from(c));
        }
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, tk.field_border), egui::StrokeKind::Inside);
        let srgb = linear.map(materialcraft_engine::image::linear_to_srgb);
        let mut hsv = egui::ecolor::Hsva::from_rgb(srgb);
        let hx = rect.left() + rect.width() * hsv.h;
        ui.painter().line_segment([egui::pos2(hx, rect.top()), egui::pos2(hx, rect.bottom())], Stroke::new(2.0, Color32::WHITE));
        if (resp.clicked() || resp.dragged())
            && let Some(p) = resp.interact_pointer_pos()
        {
            hsv.h = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            if hsv.s <= 0.0 {
                hsv.s = 1.0;
            }
            if hsv.v <= 0.0 {
                hsv.v = 1.0;
            }
            let rgb = hsv.to_rgb();
            *linear = rgb.map(materialcraft_engine::image::srgb_to_linear);
            changed = true;
        }
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            for (m, label) in [(ColorMode::Srgb, "sRGB"), (ColorMode::Float, "Float"), (ColorMode::Hsv, "HSV")] {
                if ui.add(egui::Button::new(label).selected(mode == m).min_size(vec2(46.0, 18.0))).clicked() {
                    mode = m;
                }
            }
        });
        ui.vertical(|ui| {
            let srgb = linear.map(materialcraft_engine::image::linear_to_srgb);
            match mode {
                ColorMode::Srgb => {
                    let mut v = srgb.map(|c| (c * 255.0).round());
                    let mut dirty = false;
                    for (i, ch) in ["R", "G", "B"].iter().enumerate() {
                        dirty |= slider(ui, ch, &mut v[i], 0.0..=255.0, 0).changed();
                    }
                    if dirty {
                        *linear = v.map(|c| materialcraft_engine::image::srgb_to_linear(c / 255.0));
                        changed = true;
                    }
                }
                ColorMode::Float => {
                    let mut dirty = false;
                    for (i, ch) in ["R", "G", "B"].iter().enumerate() {
                        dirty |= slider(ui, ch, &mut linear[i], 0.0..=1.0, 3).changed();
                    }
                    changed |= dirty;
                }
                ColorMode::Hsv => {
                    let hsv = egui::ecolor::Hsva::from_rgb(srgb);
                    let mut v = [hsv.h * 360.0, hsv.s * 100.0, hsv.v * 100.0];
                    let mut dirty = false;
                    dirty |= slider(ui, "H", &mut v[0], 0.0..=360.0, 0).changed();
                    dirty |= slider(ui, "S", &mut v[1], 0.0..=100.0, 0).changed();
                    dirty |= slider(ui, "V", &mut v[2], 0.0..=100.0, 0).changed();
                    if dirty {
                        let rgb = egui::ecolor::Hsva::new(v[0] / 360.0, v[1] / 100.0, v[2] / 100.0, 1.0).to_rgb();
                        *linear = rgb.map(materialcraft_engine::image::srgb_to_linear);
                        changed = true;
                    }
                }
            }
            if let Some(a) = alpha {
                let mut v = if mode == ColorMode::Float { *a } else { (*a * 255.0).round() };
                let (hi, dec) = if mode == ColorMode::Float { (1.0, 3) } else { (255.0, 0) };
                if slider(ui, "A", &mut v, 0.0..=hi, dec).changed() {
                    *a = if mode == ColorMode::Float { v } else { v / 255.0 };
                    changed = true;
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(mode_id, mode));
    changed
}

/// A grey value row like [`color_strip`]: swatch, black-to-white strip and an "L" slider
/// (sRGB 0..255 or float). `value` is linear 0..1. Returns true when it changed.
pub fn gray_strip(ui: &mut Ui, id: &str, value: &mut f32) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mode_id = ui.make_persistent_id((id, "gray-mode"));
    let mut float_mode: bool = ui.data(|d| d.get_temp(mode_id)).unwrap_or(false);
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut rgb = [*value; 3];
        if color_swatch(ui, id, &mut rgb, 22.0) {
            *value = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
            changed = true;
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(60.0), 18.0), Sense::click_and_drag());
        name(&resp, egui::Role::Slider, "Value");
        let steps = 16;
        for i in 0..steps {
            let (a, b) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
            let r = egui::Rect::from_min_max(
                egui::pos2(rect.left() + rect.width() * a, rect.top()),
                egui::pos2(rect.left() + rect.width() * b + 0.5, rect.bottom()),
            );
            let g = (materialcraft_engine::image::linear_to_srgb(a + 0.5 / steps as f32) * 255.0) as u8;
            ui.painter().rect_filled(r, 0.0, Color32::from_gray(g));
        }
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, tk.field_border), egui::StrokeKind::Inside);
        let x = rect.left() + rect.width() * value.clamp(0.0, 1.0);
        ui.painter().line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(2.0, tk.accent));
        if (resp.clicked() || resp.dragged())
            && let Some(p) = resp.interact_pointer_pos()
        {
            *value = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            changed = true;
        }
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            if ui.add(egui::Button::new("sRGB").selected(!float_mode).min_size(vec2(46.0, 18.0))).clicked() {
                float_mode = false;
            }
            if ui.add(egui::Button::new("Float").selected(float_mode).min_size(vec2(46.0, 18.0))).clicked() {
                float_mode = true;
            }
        });
        ui.vertical(|ui| {
            if float_mode {
                changed |= slider(ui, "L", value, 0.0..=1.0, 3).changed();
            } else {
                let mut v = (materialcraft_engine::image::linear_to_srgb(*value) * 255.0).round();
                if slider(ui, "L", &mut v, 0.0..=255.0, 0).changed() {
                    *value = materialcraft_engine::image::srgb_to_linear(v / 255.0);
                    changed = true;
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(mode_id, float_mode));
    changed
}

/// A two-state "True / False" button like the 3D view property pages. Returns true when toggled.
pub fn bool_button(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    let tk = Tokens::get(ui.ctx());
    ui.label(RichText::new(label).color(tk.text));
    let text = if *value { "True" } else { "False" };
    let r = ui.add(egui::Button::new(text).min_size(vec2(ui.available_width().max(60.0), 20.0)));
    name(&r, egui::Role::CheckBox, label);
    if r.clicked() {
        *value = !*value;
        return true;
    }
    false
}
