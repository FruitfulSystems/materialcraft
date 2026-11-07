//! Display menu overlays painted over the frame: bounding box, light gizmos, resolution, scene
//! statistics and FPS

use egui::{Color32, Rect, pos2, vec2};
use materialcraft_engine::geom::glam::Vec3;

use super::settings::settings_of;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

pub(super) fn paint(app: &mut MaterialCraftApp, p: &egui::Painter, rect: Rect) {
    let tk = Tokens::get(p.ctx());
    let v = app.session.view3d.clone();
    let d = &v.display;
    if d.bounding_box || d.lights {
        let (_, scene) = super::base_scene(app);
        let settings = settings_of(&v, app.view3d.show_env, 1);
        let cam = materialcraft_render::camera(&scene, &settings);
        let aspect = rect.width() / rect.height().max(1.0);
        let to_screen = |w: Vec3| {
            cam.project(w, aspect).map(|ndc| pos2(rect.left() + (ndc.x * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - ndc.y * 0.5) * rect.height()))
        };
        if d.bounding_box {
            let (lo, hi) = scene.mesh.bounds();
            let corners: Vec<Vec3> = (0..8)
                .map(|i| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }))
                .collect();
            let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xff, 0xc0, 0x40, 200));
            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                if let (Some(pa), Some(pb)) = (to_screen(corners[a]), to_screen(corners[b])) {
                    p.line_segment([pa, pb], stroke);
                }
            }
        }
        if d.lights {
            for (i, l) in v.lights.point.iter().enumerate() {
                let w = scene.center + Vec3::from(l.position) * scene.radius;
                if let Some(q) = to_screen(w) {
                    let c = if l.on { Color32::from_rgb(0xff, 0xe0, 0x60) } else { Color32::from_gray(0x80) };
                    p.circle(q, 6.0, c, egui::Stroke::new(1.0, Color32::BLACK));
                    p.text(q + vec2(9.0, 0.0), egui::Align2::LEFT_CENTER, format!("{} {}", tr("Light"), i + 1), egui::FontId::proportional(10.5), c);
                    if let Some(center) = to_screen(scene.center) {
                        p.line_segment([q, center], egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xff, 0xe0, 0x60, 90)));
                    }
                }
            }
        }
    }
    let mut lines: Vec<String> = Vec::new();
    if d.resolution {
        let (w, h) = app.view3d.shown.as_ref().map_or((0, 0), |(w, h, _)| (*w, *h));
        lines.push(format!("{}: {w} x {h}", tr("Resolution")));
    }
    if d.stats {
        let (_, scene) = super::base_scene(app);
        let tris = app.view3d.displaced.as_ref().map_or(scene.mesh.triangle_count(), |(_, s)| s.mesh.triangle_count());
        let maps = app.session.material_maps().len();
        lines.push(format!("{}: {tris} {}, {maps} {}", tr("Scene"), tr("triangles"), tr("maps")));
    }
    if d.fps {
        lines.push(format!("{:.0} FPS ({} {})", app.fps, app.view3d.renders, tr("frames")));
    }
    for (i, line) in lines.iter().enumerate() {
        p.text(
            rect.left_bottom() + vec2(8.0, -8.0 - 14.0 * (lines.len() - 1 - i) as f32),
            egui::Align2::LEFT_BOTTOM,
            line,
            egui::FontId::proportional(10.5),
            tk.viewport_text,
        );
    }
}
