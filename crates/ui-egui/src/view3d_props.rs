//! The 3D view's "Edit properties" pages in the Properties panel: camera, lights, environment,
//! renderer and the material's common values. Every change is a `view.3d` command.

use egui::{RichText, Ui, vec2};
use materialcraft_engine::View3d;
use serde_json::{Value as J, json};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::view3d::Page3d;

/// The panel title for an open page.
pub fn title(page: Page3d) -> String {
    format!("{} - {}", tr(page.title()), tr("Properties"))
}

/// A section header like the node pages.
fn section(ui: &mut Ui, id: &str, title: &str, body: impl FnOnce(&mut Ui)) {
    crate::widgets::section(ui, id, title, true, body);
}

/// A labelled row: the label on top, the control below.
fn labelled(ui: &mut Ui, label: &str, body: impl FnOnce(&mut Ui)) {
    let tk = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    ui.label(RichText::new(label).strong().color(tk.text));
    body(ui);
    ui.add_space(2.0);
}

/// A common-value row: a checkbox (use the map when one exists), the label and "Common".
fn common_label(ui: &mut Ui, label: &str, use_map: Option<&mut bool>) -> bool {
    let tk = Tokens::get(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        match use_map {
            Some(flag) => {
                let r = ui.add(egui::Checkbox::without_text(flag)).on_hover_text(tr("Use the map output when the graph has one (off: the common value below)"));
                changed = r.changed();
            }
            None => {
                ui.add_space(22.0);
            }
        }
        ui.label(RichText::new(label).strong().color(tk.text));
        ui.label(RichText::new(tr("Common")).color(tk.text_dim));
    });
    changed
}

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui, page: Page3d) {
    let tk = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr(page.title())).heading());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(tr("Back to the graph")).clicked() {
                app.props3d = None;
            }
        });
    });
    ui.label(RichText::new(tr("3D view properties: changes show in the view at once and are kept with the session.")).small().color(tk.text_dim));
    ui.add_space(4.0);
    let v = app.session.view3d.clone();
    let patch: Option<J> = match page {
        Page3d::Camera => camera(ui, &v),
        Page3d::Lights => lights(ui, &v),
        Page3d::Environment => environment(app, ui, &v),
        Page3d::Renderer => renderer(ui, &v),
        Page3d::Material => material(ui, &v),
    };
    if let Some(p) = patch {
        app.run("view.3d", p);
    }
}

fn camera(ui: &mut Ui, v: &View3d) -> Option<J> {
    let mut c = v.camera.clone();
    let mut changed = false;
    section(ui, "cam", tr("Camera Component"), |ui| {
        labelled(ui, tr("Focal Length (mm)"), |ui| {
            changed |= crate::widgets::slider(ui, "", &mut c.focal_length, 4.0..=300.0, 0).changed();
        });
        labelled(ui, tr("Projection"), |ui| {
            let label = if c.projection == "orthographic" { tr("Orthographic") } else { tr("Perspective") };
            let r = egui::ComboBox::from_id_salt("cam-proj").width(ui.available_width()).selected_text(label).show_ui(ui, |ui| {
                for (id, label) in [("perspective", "Perspective"), ("orthographic", "Orthographic")] {
                    if ui.selectable_label(c.projection == id, tr(label)).clicked() {
                        c.projection = id.into();
                        changed = true;
                    }
                }
            });
            crate::widgets::name(&r.response, egui::Role::ComboBox, "Projection");
        });
        labelled(ui, tr("Background Type"), |ui| {
            let labels = [("backdrop", "Studio backdrop"), ("color", "Uniform Color"), ("environment", "Environment")];
            let cur = labels.iter().find(|(id, _)| *id == c.background).map_or("Studio backdrop", |(_, l)| l);
            let r = egui::ComboBox::from_id_salt("cam-bg").width(ui.available_width()).selected_text(tr(cur)).show_ui(ui, |ui| {
                for (id, label) in labels {
                    if ui.selectable_label(c.background == id, tr(label)).clicked() {
                        c.background = id.into();
                        changed = true;
                    }
                }
            });
            crate::widgets::name(&r.response, egui::Role::ComboBox, "Background type");
        });
        if c.background == "color" {
            labelled(ui, tr("Background Color"), |ui| {
                let mut rgb = [c.background_color[0], c.background_color[1], c.background_color[2]];
                let mut a = c.background_color[3];
                if crate::widgets::color_strip(ui, "cam-bg-color", &mut rgb, Some(&mut a)) {
                    c.background_color = [rgb[0], rgb[1], rgb[2], a];
                    changed = true;
                }
            });
        }
        changed |= crate::widgets::bool_button(ui, tr("Override Render Resolution"), &mut c.override_resolution);
        ui.add_enabled_ui(c.override_resolution, |ui| {
            let mut w = c.resolution[0] as f32;
            let mut h = c.resolution[1] as f32;
            labelled(ui, tr("Render resolution"), |ui| {
                if crate::widgets::slider(ui, tr("Width"), &mut w, 16.0..=8192.0, 0).changed()
                    | crate::widgets::slider(ui, tr("Height"), &mut h, 16.0..=8192.0, 0).changed()
                {
                    c.resolution = [w as u32, h as u32];
                    changed = true;
                }
            });
            labelled(ui, tr("Render resolution scale"), |ui| {
                let r = egui::ComboBox::from_id_salt("cam-scale")
                    .width(ui.available_width())
                    .selected_text(format!("{:.0} %", c.resolution_scale * 100.0))
                    .show_ui(ui, |ui| {
                        for s in [0.25f32, 0.5, 1.0, 1.5, 2.0] {
                            if ui.selectable_label((c.resolution_scale - s).abs() < 1e-3, format!("{:.0} %", s * 100.0)).clicked() {
                                c.resolution_scale = s;
                                changed = true;
                            }
                        }
                    });
                crate::widgets::name(&r.response, egui::Role::ComboBox, "Render resolution scale");
            });
        });
        changed |= crate::widgets::bool_button(ui, tr("Automatic Clip Planes"), &mut c.auto_clip);
        ui.add_enabled_ui(!c.auto_clip, |ui| {
            labelled(ui, tr("Near Clip Plane"), |ui| {
                changed |= crate::widgets::slider(ui, "", &mut c.near, 0.001..=20.0, 3).changed();
            });
            labelled(ui, tr("Far Clip Plane"), |ui| {
                changed |= crate::widgets::slider(ui, "", &mut c.far, 0.01..=200.0, 2).changed();
            });
        });
        ui.label(RichText::new(tr("Clip distances are in mesh radii from the camera.")).small().color(Tokens::get(ui.ctx()).text_dim));
    });
    changed.then(|| json!({"camera": c}))
}

fn lights(ui: &mut Ui, v: &View3d) -> Option<J> {
    let mut l = v.lights.clone();
    let mut changed = false;
    section(ui, "ambient", tr("Ambient Light"), |ui| {
        changed |= crate::widgets::bool_button(ui, tr("Created"), &mut l.ambient_on);
        ui.add_enabled_ui(l.ambient_on, |ui| {
            labelled(ui, tr("Color"), |ui| {
                changed |= crate::widgets::color_strip(ui, "ambient-color", &mut l.ambient, None);
            });
        });
    });
    for i in 0..2 {
        section(ui, &format!("point{i}"), &format!("{} {}", tr("Point Light"), i + 1), |ui| {
            let pl = &mut l.point[i];
            changed |= crate::widgets::bool_button(ui, tr("Created"), &mut pl.on);
            ui.add_enabled_ui(pl.on, |ui| {
                labelled(ui, tr("Color"), |ui| {
                    changed |= crate::widgets::color_strip(ui, &format!("light{i}-color"), &mut pl.color, None);
                });
                labelled(ui, tr("Intensity"), |ui| {
                    changed |= crate::widgets::slider(ui, "", &mut pl.intensity, 0.0..=20.0, 2).changed();
                });
                labelled(ui, tr("Position (mesh radii from the center)"), |ui| {
                    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        changed |= crate::widgets::slider(ui, axis, &mut pl.position[k], -10.0..=10.0, 2).changed();
                    }
                });
            });
        });
    }
    changed.then(|| json!({"lights": l}))
}

fn environment(app: &mut MaterialCraftApp, ui: &mut Ui, v: &View3d) -> Option<J> {
    let mut patch = serde_json::Map::new();
    let mut enabled = v.env_enabled;
    if crate::widgets::bool_button(ui, tr("Enabled"), &mut enabled) {
        patch.insert("envEnabled".into(), json!(enabled));
    }
    labelled(ui, tr("Exposure"), |ui| {
        let mut e = v.exposure;
        if crate::widgets::slider(ui, "", &mut e, -4.0..=4.0, 2).changed() {
            patch.insert("exposure".into(), json!(e));
        }
    });
    labelled(ui, tr("Rotation Angle"), |ui| {
        let mut turns = v.env_rotation / 360.0;
        let mut degrees = v.env_rotation;
        ui.horizontal(|ui| {
            // The dial: drag around it to turn the environment.
            let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::click_and_drag());
            crate::widgets::name(&resp, egui::Role::Slider, "Rotation dial");
            let tk = Tokens::get(ui.ctx());
            let c = rect.center();
            ui.painter().circle(c, 24.0, tk.row, egui::Stroke::new(1.0, tk.field_border));
            let a = v.env_rotation.to_radians();
            ui.painter().line_segment([c, c + vec2(a.cos(), -a.sin()) * 22.0], egui::Stroke::new(2.0, tk.accent));
            if (resp.dragged() || resp.clicked())
                && let Some(p) = resp.interact_pointer_pos()
            {
                let d = p - c;
                let deg = (-d.y).atan2(d.x).to_degrees().rem_euclid(360.0);
                patch.insert("envRotation".into(), json!(deg));
            }
            ui.vertical(|ui| {
                if crate::widgets::slider(ui, tr("Turns"), &mut turns, 0.0..=1.0, 3).changed() {
                    patch.insert("envRotation".into(), json!(turns * 360.0));
                }
                if crate::widgets::slider(ui, tr("Degrees"), &mut degrees, 0.0..=360.0, 1).changed() {
                    patch.insert("envRotation".into(), json!(degrees));
                }
            });
        });
    });
    labelled(ui, tr("Environment"), |ui| {
        let cur = materialcraft_engine::view_cmds::ENVIRONMENTS.iter().find(|(id, _)| *id == v.environment).map_or("", |(_, l)| l);
        let r = egui::ComboBox::from_id_salt("env-pick").width(ui.available_width()).selected_text(tr(cur)).show_ui(ui, |ui| {
            for (id, label) in materialcraft_engine::view_cmds::ENVIRONMENTS {
                if ui.selectable_label(v.environment == *id, tr(label)).clicked() {
                    patch.insert("environment".into(), json!(id));
                }
            }
        });
        crate::widgets::name(&r.response, egui::Role::ComboBox, "Environment");
        // A strip of the environment: its blurred reflection, as the renderer sees it.
        let strip = app.thumbs.environment(ui.ctx(), &v.environment);
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, w * 0.5), egui::Sense::hover());
        match strip {
            Some(t) => {
                ui.painter().image(t, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(rect, 4.0, Tokens::get(ui.ctx()).row);
            }
        }
        let mut show_env = app.view3d.show_env;
        if ui.checkbox(&mut show_env, tr("Show the environment behind the mesh")).changed() {
            app.view3d.show_env = show_env;
        }
    });
    (!patch.is_empty()).then_some(J::Object(patch))
}

fn renderer(ui: &mut Ui, v: &View3d) -> Option<J> {
    let mut r = v.renderer.clone();
    let mut changed = false;
    section(ui, "res", tr("Resolution"), |ui| {
        changed |= crate::widgets::bool_button(ui, tr("Use viewport resolution"), &mut r.use_viewport_resolution);
        ui.add_enabled_ui(!r.use_viewport_resolution, |ui| {
            let mut w = r.resolution[0] as f32;
            let mut h = r.resolution[1] as f32;
            labelled(ui, tr("Render resolution"), |ui| {
                if crate::widgets::slider(ui, tr("Width"), &mut w, 16.0..=4096.0, 0).changed()
                    | crate::widgets::slider(ui, tr("Height"), &mut h, 16.0..=4096.0, 0).changed()
                {
                    r.resolution = [w as u32, h as u32];
                    changed = true;
                }
            });
        });
        labelled(ui, tr("Render resolution scale"), |ui| {
            let cb =
                egui::ComboBox::from_id_salt("rend-scale").width(ui.available_width()).selected_text(format!("{:.0} %", r.scale * 100.0)).show_ui(ui, |ui| {
                    for s in [0.25f32, 0.5, 0.75, 1.0, 1.5, 2.0] {
                        if ui.selectable_label((r.scale - s).abs() < 1e-3, format!("{:.0} %", s * 100.0)).clicked() {
                            r.scale = s;
                            changed = true;
                        }
                    }
                });
            crate::widgets::name(&cb.response, egui::Role::ComboBox, "Render resolution scale");
        });
    });
    section(ui, "quality", tr("Quality"), |ui| {
        labelled(ui, tr("Samples per pixel (resting frame)"), |ui| {
            let mut s = r.samples as f32;
            if crate::widgets::slider(ui, "", &mut s, 1.0..=4.0, 0).changed() {
                r.samples = s as u32;
                changed = true;
            }
            ui.label(RichText::new(tr("n × n rays per pixel once the camera rests")).small().color(Tokens::get(ui.ctx()).text_dim));
        });
        labelled(ui, tr("Ambient occlusion rays"), |ui| {
            let mut a = r.ao_rays as f32;
            if crate::widgets::slider(ui, "", &mut a, 0.0..=32.0, 0).changed() {
                r.ao_rays = a as u32;
                changed = true;
            }
            ui.label(RichText::new(tr("Ray-traced occlusion in the relief's crevices (0 = off)")).small().color(Tokens::get(ui.ctx()).text_dim));
        });
    });
    changed.then(|| json!({"renderer": r}))
}

fn material(ui: &mut Ui, v: &View3d) -> Option<J> {
    let tk = Tokens::get(ui.ctx());
    let mut m = v.material.clone();
    let mut changed = false;
    let mut extra = serde_json::Map::new();
    ui.label(RichText::new(format!("{} / {}", tr("OpenPBR"), tr("Tessellation + Displacement"))).color(tk.text_dim));
    let weight = |ui: &mut Ui, id: &str, label: &str, value: &mut f32, use_map: Option<&mut bool>, changed: &mut bool| {
        *changed |= common_label(ui, label, use_map);
        *changed |= crate::widgets::gray_strip(ui, id, value);
    };
    let color = |ui: &mut Ui, id: &str, label: &str, value: &mut [f32; 3], use_map: Option<&mut bool>, changed: &mut bool| {
        *changed |= common_label(ui, label, use_map);
        *changed |= crate::widgets::color_strip(ui, id, value, None);
    };
    let scalar = |ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, changed: &mut bool| {
        common_label(ui, label, None);
        *changed |= crate::widgets::slider(ui, "", value, range, 2).changed();
    };
    section(ui, "uvs", tr("UVs"), |ui| {
        labelled(ui, tr("Tiling"), |ui| {
            let mut t = v.tiling;
            if crate::widgets::slider(ui, "", &mut t, 0.25..=16.0, 2).changed() {
                extra.insert("tiling".into(), json!(t));
            }
        });
        labelled(ui, tr("UV scale"), |ui| {
            changed |= crate::widgets::slider(ui, "U", &mut m.uv_scale[0], 0.1..=8.0, 2).changed();
            changed |= crate::widgets::slider(ui, "V", &mut m.uv_scale[1], 0.1..=8.0, 2).changed();
        });
    });
    let c = &mut m.common;
    let u = &mut m.use_map;
    section(ui, "base", tr("Base"), |ui| {
        weight(ui, "base-weight", tr("Weight"), &mut c.base_weight, None, &mut changed);
        color(ui, "base-color", tr("Color"), &mut c.base_color, Some(&mut u.base_color), &mut changed);
        weight(ui, "metalness", tr("Metalness"), &mut c.metalness, Some(&mut u.metallic), &mut changed);
        weight(ui, "diffuse-rough", tr("Diffuse roughness"), &mut c.diffuse_roughness, None, &mut changed);
    });
    section(ui, "specular", tr("Specular"), |ui| {
        weight(ui, "spec-weight", tr("Weight"), &mut c.specular_weight, Some(&mut u.specular), &mut changed);
        color(ui, "spec-color", tr("Color"), &mut c.specular_color, None, &mut changed);
        weight(ui, "spec-rough", tr("Roughness"), &mut c.specular_roughness, Some(&mut u.roughness), &mut changed);
        scalar(ui, tr("IOR"), &mut c.ior, 1.0..=3.0, &mut changed);
    });
    section(ui, "transmission", tr("Transmission"), |ui| {
        weight(ui, "trans-weight", tr("Weight"), &mut c.transmission_weight, None, &mut changed);
        color(ui, "trans-color", tr("Color"), &mut c.transmission_color, None, &mut changed);
    });
    section(ui, "subsurface", tr("Subsurface"), |ui| {
        weight(ui, "sss-weight", tr("Weight"), &mut c.subsurface_weight, None, &mut changed);
        color(ui, "sss-color", tr("Color"), &mut c.subsurface_color, None, &mut changed);
    });
    section(ui, "coat", tr("Coat"), |ui| {
        weight(ui, "coat-weight", tr("Weight"), &mut c.coat_weight, None, &mut changed);
        color(ui, "coat-color", tr("Color"), &mut c.coat_color, None, &mut changed);
        weight(ui, "coat-rough", tr("Roughness"), &mut c.coat_roughness, None, &mut changed);
        scalar(ui, tr("IOR"), &mut c.coat_ior, 1.0..=3.0, &mut changed);
        weight(ui, "coat-dark", tr("Darkening"), &mut c.coat_darkening, None, &mut changed);
    });
    section(ui, "fuzz", tr("Fuzz"), |ui| {
        weight(ui, "fuzz-weight", tr("Weight"), &mut c.fuzz_weight, None, &mut changed);
        color(ui, "fuzz-color", tr("Color"), &mut c.fuzz_color, None, &mut changed);
    });
    section(ui, "emission", tr("Emission"), |ui| {
        common_label(ui, tr("Luminance"), Some(&mut u.emissive));
        changed |= crate::widgets::slider(ui, "", &mut c.emission_luminance, 0.0..=10.0, 2).changed();
        color(ui, "emit-color", tr("Color"), &mut c.emission_color, None, &mut changed);
    });
    section(ui, "thinfilm", tr("Thin-film"), |ui| {
        weight(ui, "film-weight", tr("Weight"), &mut c.thin_film_weight, None, &mut changed);
        common_label(ui, tr("Thickness (nm)"), None);
        changed |= crate::widgets::slider(ui, "", &mut c.thin_film_thickness, 0.0..=2000.0, 0).changed();
        scalar(ui, tr("IOR"), &mut c.thin_film_ior, 1.0..=3.0, &mut changed);
    });
    section(ui, "geometry", tr("Geometry"), |ui| {
        weight(ui, "opacity", tr("Opacity"), &mut c.opacity, Some(&mut u.opacity), &mut changed);
        common_label(ui, tr("Thin walled"), None);
        let mut tw = c.thin_walled;
        if crate::widgets::bool_button(ui, "", &mut tw) {
            c.thin_walled = tw;
            changed = true;
        }
        common_label(ui, tr("DirectX normal"), Some(&mut u.normal));
        let mut flip = v.normal_y_flip;
        if crate::widgets::bool_button(ui, "", &mut flip) {
            extra.insert("normalYFlip".into(), json!(flip));
        }
        weight(ui, "ao", tr("Ambient occlusion"), &mut c.ambient_occlusion, Some(&mut u.ao), &mut changed);
    });
    section(ui, "height", tr("Height"), |ui| {
        weight(ui, "height", tr("Height"), &mut c.height, Some(&mut u.height), &mut changed);
        common_label(ui, tr("Height scale"), None);
        let mut hs = v.height_scale * 100.0;
        if crate::widgets::slider(ui, "", &mut hs, 0.0..=50.0, 1).changed() {
            extra.insert("heightScale".into(), json!(hs / 100.0));
        }
        ui.label(RichText::new(tr("% of the mesh size")).small().color(tk.text_dim));
        common_label(ui, tr("Height level"), None);
        let mut hl = v.height_level;
        if crate::widgets::slider(ui, "", &mut hl, 0.0..=1.0, 2).changed() {
            extra.insert("heightLevel".into(), json!(hl));
        }
        common_label(ui, tr("Tessellation factor"), None);
        changed |= crate::widgets::slider(ui, "", &mut m.tessellation, 0.1..=4.0, 2).changed();
        ui.label(RichText::new(tr("Multiplies the displaced mesh's triangle budget")).small().color(tk.text_dim));
    });
    if changed {
        extra.insert("material".into(), json!(m));
    }
    (!extra.is_empty()).then_some(J::Object(extra))
}
