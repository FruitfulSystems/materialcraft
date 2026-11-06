//! Textures for node thumbnails (from cooked outputs) and the library's node previews (each node
//! cooked at a small size on sample inputs: the library pictures are made by MaterialCraft itself,
//! no icon files).

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc::Receiver;

use egui::{ColorImage, TextureHandle, TextureOptions};
use materialcraft_engine::image::{Image, Kind};
use materialcraft_engine::nodes::{Ctx, catalog, resolve_params};

/// Rendered previews by id: (id, RGBA8 pixels).
pub type Previews = Vec<(String, Vec<u8>)>;

/// Side of node thumbnails and library previews, in pixels.
pub const THUMB: u32 = 96;
const LIB: u32 = 72;

#[derive(Default)]
pub struct Thumbs {
    /// (cook key, port) → texture and the frame it was last used.
    nodes: HashMap<(u64, usize), (TextureHandle, u64)>,
    library: HashMap<String, TextureHandle>,
    /// Environment previews by id (the equirectangular map, tone mapped).
    environments: HashMap<String, TextureHandle>,
    library_rx: Option<Receiver<Previews>>,
    library_started: bool,
    frame: u64,
}

impl Thumbs {
    /// A small picture of a 3D view environment (rendered once per id).
    pub fn environment(&mut self, ctx: &egui::Context, id: &str) -> Option<egui::TextureId> {
        if let Some(t) = self.environments.get(id) {
            return Some(t.id());
        }
        let env = materialcraft_render::ibl::environment(materialcraft_render::ibl::by_id(id), 256);
        let (w, h) = (env.width as usize, env.height as usize);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for px in &env.data {
            for c in px {
                // Exposure down a stop and a gamma: bright skies stay readable.
                let v = materialcraft_engine::image::linear_to_srgb((c * 0.5).min(1.0));
                rgba.push((v * 255.0 + 0.5) as u8);
            }
            rgba.push(255);
        }
        let img = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        let t = ctx.load_texture(format!("env-{id}"), img, TextureOptions::LINEAR);
        let tid = t.id();
        self.environments.insert(id.to_string(), t);
        Some(tid)
    }
}

/// An image as an egui texture image (sRGB display, `side` px).
pub fn color_image(img: &Image, side: u32) -> ColorImage {
    let small = img.resize(side, side, true);
    ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &small.to_rgba8_display())
}

impl Thumbs {
    /// The thumbnail of a node output, uploaded on first use.
    pub fn node(&mut self, ctx: &egui::Context, key: u64, port: usize, img: &Image) -> egui::TextureId {
        let frame = self.frame;
        if let Some((t, f)) = self.nodes.get_mut(&(key, port)) {
            *f = frame;
            return t.id();
        }
        let tex = ctx.load_texture(format!("node-{key}-{port}"), color_image(img, THUMB), TextureOptions::LINEAR);
        let id = tex.id();
        self.nodes.insert((key, port), (tex, frame));
        id
    }

    /// Once per frame: forget thumbnails unused for a few seconds; collect library previews.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        let f = self.frame;
        if self.nodes.len() > 256 {
            self.nodes.retain(|_, (_, used)| f - *used < 120);
        }
        if let Some(rx) = &self.library_rx
            && let Ok(list) = rx.try_recv()
        {
            for (id, rgba) in list {
                let img = ColorImage::from_rgba_unmultiplied([LIB as usize, LIB as usize], &rgba);
                self.library.insert(id.clone(), ctx.load_texture(format!("lib-{id}"), img, TextureOptions::LINEAR));
            }
            self.library_rx = None;
        }
    }

    /// Drop the library previews and generate them again (Preferences ▸ Library ▸ Rebuild Thumbnails).
    pub fn rebuild_library(&mut self) {
        self.library.clear();
        self.library_rx = None;
        self.library_started = false;
    }

    /// The library preview of node `def` (generated in the background on first use).
    pub fn library(&mut self, ctx: &egui::Context, def: &str) -> Option<egui::TextureId> {
        if !self.library_started {
            self.library_started = true;
            self.library_rx = Some(spawn_library_previews(ctx.clone()));
        }
        self.library.get(def).map(TextureHandle::id)
    }
}

/// Sample inputs for previews: a cloud height, a colored version and its normal map.
fn sample_inputs() -> (Image, Image, Image) {
    let gray = Image::generate_uv(LIB, LIB, Kind::Gray, |u, v| [materialcraft_engine::nodes::noise::fbm(u, v, 3.0, 5, 0.5, 11), 0.0, 0.0, 0.0]);
    let color = gray.map(Kind::Color, |p| [0.15 + 0.7 * p[0], 0.35 + 0.3 * p[0], 0.8 - 0.5 * p[0], 1.0]);
    let normal = materialcraft_engine::nodes::atomic::height_to_normal(&gray, 4.0, false, true, false);
    (gray, color, normal)
}

fn previews() -> Previews {
    let (gray, color, normal) = sample_inputs();
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let mut out = Vec::new();
    for def in catalog() {
        if def.id == "graph_instance" {
            continue;
        }
        let inputs: Vec<Option<Image>> = def
            .inputs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // Masks and secondary inputs stay empty so the main effect shows.
                if i > 0 && !matches!(def.id, "blend" | "height_blend" | "normal_blend" | "rgba_merge") {
                    return None;
                }
                Some(if p.id.contains("normal") || def.category.contains("Normal") && p.kind == materialcraft_engine::nodes::PortKind::Color {
                    normal.clone()
                } else if p.kind == materialcraft_engine::nodes::PortKind::Color {
                    color.clone()
                } else if i == 1 {
                    gray.map(Kind::Gray, |v| [1.0 - v[0], 0.0, 0.0, 0.0])
                } else {
                    gray.clone()
                })
            })
            .collect();
        let params = resolve_params(def, &BTreeMap::new());
        let ctx = Ctx { w: LIB, h: LIB, seed: 3, tile: true, inputs: &inputs, params: &params, resource: &no_res, vars: &vars };
        if let Ok(imgs) = (def.cook)(&ctx)
            && let Some(img) = imgs.first()
        {
            out.push((def.id.to_string(), img.resize(LIB, LIB, true).to_rgba8_display()));
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_library_previews(ctx: egui::Context) -> Receiver<Previews> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("materialcraft-library".into()).spawn(move || {
        let _ = tx.send(previews());
        ctx.request_repaint();
    });
    rx
}

#[cfg(target_arch = "wasm32")]
fn spawn_library_previews(_ctx: egui::Context) -> Receiver<Previews> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = tx.send(previews());
    rx
}
