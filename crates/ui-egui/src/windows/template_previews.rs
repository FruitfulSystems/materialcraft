use std::{collections::HashMap, sync::mpsc::Receiver};

use egui::TextureHandle;

/// Procedural template thumbnails owned by one app and its texture manager
#[derive(Default)]
pub struct TemplatePreviews {
    textures: HashMap<String, TextureHandle>,
    receiver: Option<Receiver<crate::thumbs::Previews>>,
    started: bool,
}

const PREVIEW_PIXELS: u32 = 96;

fn render_templates() -> crate::thumbs::Previews {
    let lighting = materialcraft_render::Lighting::new("studio");
    let scene = materialcraft_render::Scene::builtin("sphere");
    let settings = materialcraft_render::Settings { distance: 2.7, supersample: 2, ..Default::default() };
    materialcraft_engine::samples::TEMPLATES
        .iter()
        .filter_map(|(identifier, _)| {
            let mut package = materialcraft_engine::core::Package::new("Template preview");
            package.graphs.push(materialcraft_engine::samples::template(identifier, "Template preview", 8).ok()?);
            let mut session = materialcraft_engine::Session::new(package);
            session.preview_size = Some(256);
            session.cook();
            let maps = materialcraft_render::Maps::from_usages(session.material_maps());
            Some((identifier.to_string(), materialcraft_render::render(&scene, &maps, &lighting, &settings, PREVIEW_PIXELS, PREVIEW_PIXELS)))
        })
        .collect()
}

impl TemplatePreviews {
    pub(super) fn get(&mut self, context: &egui::Context, identifier: &str) -> Option<egui::TextureId> {
        if !self.started {
            self.started = true;
            let (sender, receiver) = std::sync::mpsc::channel();
            #[cfg(not(target_arch = "wasm32"))]
            {
                let context = context.clone();
                let _ = std::thread::Builder::new().name("materialcraft-templates".into()).spawn(move || {
                    let _ = sender.send(render_templates());
                    context.request_repaint();
                });
            }
            #[cfg(target_arch = "wasm32")]
            let _ = sender.send(render_templates());
            self.receiver = Some(receiver);
        }
        if let Some(receiver) = &self.receiver
            && let Ok(previews) = receiver.try_recv()
        {
            for (identifier, pixels) in previews {
                let image = egui::ColorImage::from_rgba_unmultiplied([PREVIEW_PIXELS as usize, PREVIEW_PIXELS as usize], &pixels);
                self.textures.insert(identifier.clone(), context.load_texture(format!("tpl-{identifier}"), image, egui::TextureOptions::LINEAR));
            }
            self.receiver = None;
        }
        self.textures.get(identifier).map(TextureHandle::id)
    }
}
