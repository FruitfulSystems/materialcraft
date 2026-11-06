//! The session: the open package, the graph being edited, selection, views, snapshot undo and
//! the cook cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use materialcraft_core::{Graph, NodeId, Package, ResourceBytes};
use materialcraft_eval::{Cache, Cooked, Options};
use materialcraft_image::Image;
use serde::{Deserialize, Serialize};

use crate::{EngineError, Result};

/// Undo keeps this many steps unless the session says otherwise.
pub const MAX_UNDO: usize = 200;

/// The 3D view's scene and look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View3d {
    /// Built-in mesh: `rounded_cube`, `cube`, `sphere`, `cylinder`, `plane`, `torus`, `material_ball`.
    pub scene: String,
    /// UV repeats of the material on the mesh.
    pub tiling: f32,
    /// Camera yaw and pitch (degrees) and distance (mesh radii).
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Where the camera orbits: offset from the mesh center (mesh radii, world axes).
    pub target: [f32; 3],
    /// Environment: `studio`, `sunset`, `overcast`, `night`.
    pub environment: String,
    pub env_rotation: f32,
    pub exposure: f32,
    /// How far the height output raises the surface (white above black), in mesh radii; 0 = off.
    pub height_scale: f32,
    /// The height (0..1) that stays on the surface: 0 = the relief rises out of the mesh, 0.5 = centered on it.
    pub height_level: f32,
    /// Flip green when reading DirectX normal maps; false matches the Normal node's OpenGL default.
    pub normal_y_flip: bool,
    pub show_axis: bool,
    pub show_grid: bool,
    /// See through the mesh where the material's alpha (opacity output, else base color alpha)
    /// is below one.
    pub transparency: bool,
    /// Show the material's alpha channel on the mesh instead of the lit material.
    pub alpha_view: bool,
    /// Camera ▸ Edit properties.
    pub camera: Camera3d,
    /// Lights ▸ Edit properties.
    pub lights: Lights3d,
    /// Environment ▸ Edit properties: the environment lights the scene.
    pub env_enabled: bool,
    /// Renderer ▸ Edit settings.
    pub renderer: Renderer3d,
    /// Display menu toggles.
    pub display: Display3d,
    /// Materials ▸ Default ▸ Edit properties.
    pub material: Material3d,
}

/// The 3D view's camera properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Camera3d {
    /// Millimetres (a 36 mm frame): 35 mm ≈ 35° vertical field of view on a 3:2 frame.
    pub focal_length: f32,
    /// `perspective` or `orthographic`.
    pub projection: String,
    /// `backdrop`, `color` or `environment`.
    pub background: String,
    /// Linear RGBA.
    pub background_color: [f32; 4],
    /// Render at a fixed resolution (Save Render) instead of the viewport's.
    pub override_resolution: bool,
    pub resolution: [u32; 2],
    /// Multiplier on the render resolution.
    pub resolution_scale: f32,
    /// Clip planes from the scene size; else `near`/`far` in mesh radii from the camera.
    pub auto_clip: bool,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Camera3d {
            focal_length: 35.0,
            projection: "perspective".into(),
            background: "backdrop".into(),
            background_color: [0.016, 0.016, 0.016, 1.0],
            override_resolution: false,
            resolution: [1920, 1080],
            resolution_scale: 1.0,
            auto_clip: true,
            near: 0.1,
            far: 100.0,
        }
    }
}

impl Camera3d {
    /// Vertical field of view in degrees for the focal length (36 × 24 mm frame).
    pub fn fov(&self) -> f32 {
        (2.0 * (12.0 / self.focal_length.max(1.0)).atan()).to_degrees().clamp(5.0, 120.0)
    }

    fn sanitize(&mut self) {
        let d = Camera3d::default();
        let f = |v: f32, lo: f32, hi: f32, dv: f32| if v.is_finite() { v.clamp(lo, hi) } else { dv };
        self.focal_length = f(self.focal_length, 4.0, 1200.0, d.focal_length);
        if !["perspective", "orthographic"].contains(&self.projection.as_str()) {
            self.projection = d.projection;
        }
        if !["backdrop", "color", "environment"].contains(&self.background.as_str()) {
            self.background = d.background;
        }
        for (i, v) in self.background_color.iter_mut().enumerate() {
            *v = f(*v, 0.0, 1.0, d.background_color[i]);
        }
        self.resolution = self.resolution.map(|v| v.clamp(16, 8192));
        self.resolution_scale = f(self.resolution_scale, 0.1, 4.0, d.resolution_scale);
        self.near = f(self.near, 0.001, 1000.0, d.near);
        self.far = f(self.far, 0.01, 10_000.0, d.far).max(self.near * 1.01);
    }
}

/// The 3D view's lights: an ambient term and two point lights.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Lights3d {
    pub ambient_on: bool,
    /// Linear RGB.
    pub ambient: [f32; 3],
    pub point: [materialcraft_geom::preview::PointLight; 2],
}

impl Default for Lights3d {
    fn default() -> Self {
        Lights3d { ambient_on: true, ambient: [0.0; 3], point: materialcraft_geom::preview::PointLight::DEFAULTS }
    }
}

impl Lights3d {
    fn sanitize(&mut self) {
        for v in self.ambient.iter_mut() {
            *v = if v.is_finite() { v.clamp(0.0, 16.0) } else { 0.0 };
        }
        let d = materialcraft_geom::preview::PointLight::DEFAULTS;
        self.point = [self.point[0].sanitized(d[0]), self.point[1].sanitized(d[1])];
    }
}

/// Renderer ▸ Edit settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Renderer3d {
    pub use_viewport_resolution: bool,
    pub resolution: [u32; 2],
    /// Resolution scale of the viewport render (0.25..2).
    pub scale: f32,
    /// Supersampling of the resting frame (1..4 per axis).
    pub samples: u32,
    /// Ray-traced ambient occlusion rays per sample (0 = off).
    pub ao_rays: u32,
}

impl Default for Renderer3d {
    fn default() -> Self {
        Renderer3d { use_viewport_resolution: true, resolution: [1920, 1080], scale: 1.0, samples: 2, ao_rays: 0 }
    }
}

impl Renderer3d {
    fn sanitize(&mut self) {
        self.resolution = self.resolution.map(|v| v.clamp(16, 4096));
        self.scale = if self.scale.is_finite() { self.scale.clamp(0.25, 2.0) } else { 1.0 };
        self.samples = self.samples.clamp(1, 4);
        self.ao_rays = self.ao_rays.min(64);
    }
}

/// Display menu toggles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Display3d {
    pub wireframe: bool,
    pub backface_culling: bool,
    pub bounding_box: bool,
    /// Draw the point lights.
    pub lights: bool,
    /// Overlay the render resolution.
    pub resolution: bool,
    /// Overlay the scene statistics (triangles, maps).
    pub stats: bool,
    /// Overlay the frame rate.
    pub fps: bool,
}

/// Materials ▸ Default: the shading model and what each channel is without a map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Material3d {
    pub shader: materialcraft_geom::preview::Shader,
    pub common: materialcraft_geom::preview::Common,
    pub use_map: materialcraft_geom::preview::MapUse,
    /// Displacement subdivision budget multiplier (0.1..4).
    pub tessellation: f32,
    /// Extra UV scale of the material on the mesh.
    pub uv_scale: [f32; 2],
}

impl Default for Material3d {
    fn default() -> Self {
        Material3d {
            shader: materialcraft_geom::preview::Shader::OpenPbr,
            common: materialcraft_geom::preview::Common::default(),
            use_map: materialcraft_geom::preview::MapUse::default(),
            tessellation: 1.0,
            uv_scale: [1.0, 1.0],
        }
    }
}

impl Material3d {
    fn sanitize(&mut self) {
        self.common = self.common.sanitized();
        self.tessellation = if self.tessellation.is_finite() { self.tessellation.clamp(0.1, 4.0) } else { 1.0 };
        self.uv_scale = self.uv_scale.map(|v| if v.is_finite() { v.clamp(0.01, 64.0) } else { 1.0 });
    }
}

impl View3d {
    /// Every nested value brought into its range (after a command merged JSON in).
    pub fn sanitize(&mut self) {
        self.camera.sanitize();
        self.lights.sanitize();
        self.renderer.sanitize();
        self.material.sanitize();
    }
}

impl Default for View3d {
    fn default() -> Self {
        View3d {
            scene: "rounded_cube".into(),
            tiling: 1.0,
            yaw: 35.0,
            pitch: 25.0,
            distance: 3.2,
            target: [0.0; 3],
            environment: "studio".into(),
            env_rotation: 0.0,
            exposure: 0.0,
            height_scale: 0.04,
            height_level: 0.0,
            normal_y_flip: false,
            show_axis: false,
            show_grid: false,
            transparency: true,
            alpha_view: false,
            camera: Camera3d::default(),
            lights: Lights3d::default(),
            env_enabled: true,
            renderer: Renderer3d::default(),
            display: Display3d::default(),
            material: Material3d::default(),
        }
    }
}

/// The 3D view's camera preferences. The app keeps them in its preferences (Edit ▸ Preferences)
/// and pushes them in with [`Session::set_camera_prefs`]; headless sessions use the defaults.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CameraPrefs {
    /// How far the orbit target may pan from the mesh center, in mesh radii.
    pub pan_limit: f32,
    /// Pan speed: 1 keeps the mesh under the pointer.
    pub pan_speed: f32,
}

impl Default for CameraPrefs {
    fn default() -> Self {
        CameraPrefs { pan_limit: 5.0, pan_speed: 1.0 }
    }
}

impl CameraPrefs {
    /// These preferences with any non-finite or negative value replaced by its default.
    pub fn sanitized(self) -> Self {
        let d = CameraPrefs::default();
        let or = |v: f32, default: f32| if v.is_finite() && v >= 0.0 { v } else { default };
        CameraPrefs { pan_limit: or(self.pan_limit, d.pan_limit), pan_speed: or(self.pan_speed, d.pan_speed) }
    }

    /// An orbit target held within the pan limit (non-finite parts become 0).
    pub fn clamp_target(&self, t: [f32; 3]) -> [f32; 3] {
        let limit = self.sanitized().pan_limit;
        t.map(|v| if v.is_finite() { v.clamp(-limit, limit) } else { 0.0 })
    }
}

/// What the 2D view shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View2d {
    pub node: Option<NodeId>,
    pub port: usize,
    /// Repeat the image 3×3 to check tiling.
    pub tiled: bool,
    /// `rgba`, `r`, `g`, `b`, `a`.
    pub channel: String,
    /// Overlay the luminance/channel histogram.
    pub histogram: bool,
}

#[derive(Clone)]
struct Snapshot {
    pkg: Package,
    graph: usize,
    selection: Vec<NodeId>,
    label: String,
    modified: bool,
    dirty: bool,
    cook_epoch: u64,
}

/// Nodes and connections copied to the clipboard (as JSON, so other sessions can paste them).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub nodes: Vec<materialcraft_core::Node>,
    pub connections: Vec<materialcraft_core::Connection>,
}

/// The prefix of warning lines in [`Session::log`].
pub const WARNING_PREFIX: &str = "Warning: ";

pub struct Session {
    pub pkg: Package,
    /// Where the package was opened from or last saved to.
    pub path: Option<String>,
    /// Index of the graph being edited.
    pub graph: usize,
    pub selection: Vec<NodeId>,
    pub view2d: View2d,
    pub view3d: View3d,
    /// Cap on node output size while editing (`None` = full size).
    pub preview_size: Option<u32>,
    /// 3D view camera preferences (set with [`Session::set_camera_prefs`]).
    pub camera: CameraPrefs,
    pub modified: bool,
    pub log: Vec<String>,
    /// What the last opened SBS package held that MaterialCraft couldn't read; empty after a clean open.
    pub load_warnings: Vec<String>,
    pub clipboard: Option<Clip>,
    /// The last cook of the open graph.
    pub cooked: Cooked,
    /// Set when the graph changed and needs cooking.
    pub dirty: bool,
    /// Bumped by every synchronous [`Session::cook`]; a background cook that started before the
    /// bump holds stale results and must not replace `cooked`.
    pub cook_epoch: u64,
    /// Cooked node outputs, shared with background cooks.
    pub cache: Arc<Mutex<Cache>>,
    /// A mesh loaded for the 3D view (`scene: "custom"`): its file name and geometry.
    pub custom_mesh: Option<(String, Arc<materialcraft_geom::Mesh>)>,
    /// Undo steps kept (Preferences ▸ History stack size).
    pub max_undo: usize,
    /// Cap on the larger side of every cook, previews and exports alike (Preferences ▸ Cooking size limit).
    pub cook_limit: Option<u32>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Edits with the same key coalesce into one undo step (slider drags).
    merge_key: Option<String>,
    /// Snapshot and successful-edit flag for the current command group.
    group: Option<(Snapshot, bool)>,
    decoded: Arc<Mutex<HashMap<(u64, bool), Image>>>,
}

impl Session {
    pub fn new(pkg: Package) -> Session {
        Session {
            pkg,
            path: None,
            graph: 0,
            selection: Vec::new(),
            view2d: View2d { channel: "rgba".into(), ..View2d::default() },
            view3d: View3d::default(),
            preview_size: Some(512),
            camera: CameraPrefs::default(),
            modified: false,
            log: Vec::new(),
            load_warnings: Vec::new(),
            clipboard: None,
            cooked: Cooked::default(),
            dirty: true,
            cook_epoch: 0,
            cache: Arc::new(Mutex::new(Cache::default())),
            custom_mesh: None,
            max_undo: MAX_UNDO,
            cook_limit: None,
            undo: Vec::new(),
            redo: Vec::new(),
            merge_key: None,
            group: None,
            decoded: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The graph being edited.
    pub fn graph(&self) -> Result<&Graph> {
        self.pkg.graph(self.graph).map_err(EngineError::from)
    }

    pub fn graph_mut(&mut self) -> Result<&mut Graph> {
        self.pkg.graph_mut(self.graph).map_err(EngineError::from)
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            pkg: self.pkg.clone(),
            graph: self.graph,
            selection: self.selection.clone(),
            label: label.into(),
            modified: self.modified,
            dirty: self.dirty,
            cook_epoch: self.cook_epoch,
        }
    }

    /// Run `f` as one undoable step. On error the session is restored.
    pub fn edit<T>(&mut self, label: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        self.merge_key = None;
        self.edit_inner(label, f)
    }

    /// Like [`Session::edit`], but consecutive edits with the same `key` (dragging one slider)
    /// make a single undo step.
    pub fn edit_merged<T>(&mut self, key: &str, label: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        if self.group.is_some() {
            return self.edit_inner(label, f);
        }
        if self.merge_key.as_deref() == Some(key) && !self.undo.is_empty() {
            // The merged step keeps its original snapshot; restore the session if `f` fails.
            let snap = self.snapshot(label);
            let r = f(self);
            match &r {
                Ok(_) => self.changed(),
                Err(_) => self.restore(snap),
            }
            return r;
        }
        let r = self.edit_inner(label, f);
        if r.is_ok() {
            self.merge_key = Some(key.into());
        }
        r
    }

    /// End the current coalescing run: the next [`Session::edit_merged`] starts a new undo step.
    /// Call it when a gesture ends (pointer release) and once per control or MCP request.
    pub fn end_merge(&mut self) {
        self.merge_key = None;
    }

    /// Start grouping: every edit until [`Session::end_group`] becomes one undo entry labelled
    /// `label` (a gesture that runs several commands, such as rewiring a link). Nothing is
    /// recorded if no edit ran. A group started inside a group is ignored.
    pub fn begin_group(&mut self, label: &str) {
        if self.group.is_none() {
            self.merge_key = None;
            self.group = Some((self.snapshot(label), false));
        }
    }

    /// Close the group opened by [`Session::begin_group`], recording one undo entry if any edit ran.
    pub fn end_group(&mut self) {
        if let Some((snap, edited)) = self.group.take()
            && edited
        {
            self.undo.push(snap);
            self.trim_undo();
            self.redo.clear();
        }
    }

    /// Run `f` as one undo entry, however many edits it makes. On error everything `f` did is
    /// undone and nothing is recorded.
    pub fn group<T>(&mut self, label: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        if self.group.is_some() {
            let snap = self.snapshot(label);
            let edited = self.group.as_ref().is_some_and(|(_, edited)| *edited);
            let result = f(self);
            if result.is_err() {
                self.restore(snap);
                if let Some((_, flag)) = self.group.as_mut() {
                    *flag = edited;
                }
            }
            return result;
        }
        self.begin_group(label);
        let r = f(self);
        match r {
            Ok(_) => self.end_group(),
            Err(_) => {
                if let Some((snap, _)) = self.group.take() {
                    self.restore(snap);
                }
            }
        }
        r
    }

    fn edit_inner<T>(&mut self, label: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        let snap = self.snapshot(label);
        if self.group.is_some() {
            // Only successful edits make the group an undo entry.
            return match f(self) {
                Ok(v) => {
                    if let Some((_, edited)) = self.group.as_mut() {
                        *edited = true;
                    }
                    self.changed();
                    Ok(v)
                }
                Err(e) => {
                    self.restore(snap);
                    Err(e)
                }
            };
        }
        match f(self) {
            Ok(v) => {
                self.undo.push(snap);
                self.trim_undo();
                self.redo.clear();
                self.changed();
                Ok(v)
            }
            Err(e) => {
                self.restore(snap);
                Err(e)
            }
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.pkg = s.pkg;
        self.graph = s.graph.min(self.pkg.graphs.len().saturating_sub(1));
        self.selection = s.selection;
        self.modified = s.modified;
        self.dirty = s.dirty || self.cook_epoch != s.cook_epoch;
    }

    /// Mark the document modified and in need of a cook.
    pub fn changed(&mut self) {
        self.modified = true;
        self.dirty = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Labels of the undo stack, oldest first.
    pub fn history(&self) -> Vec<String> {
        self.undo.iter().map(|s| s.label.clone()).collect()
    }

    /// Drop the oldest undo steps past `max_undo`.
    pub fn trim_undo(&mut self) {
        let max = self.max_undo.max(1);
        if self.undo.len() > max {
            let extra = self.undo.len() - max;
            self.undo.drain(..extra);
        }
    }

    /// The preview cap and the cooking size limit, whichever is smaller.
    pub fn cook_size(&self, preview: Option<u32>) -> Option<u32> {
        match (preview, self.cook_limit) {
            (Some(p), Some(l)) => Some(p.min(l)),
            (p, l) => p.or(l),
        }
    }

    pub fn undo(&mut self) -> Option<String> {
        self.end_group();
        let s = self.undo.pop()?;
        let label = s.label.clone();
        let cur = self.snapshot(&label);
        self.redo.push(cur);
        self.restore(s);
        self.merge_key = None;
        self.changed();
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.end_group();
        let s = self.redo.pop()?;
        let label = s.label.clone();
        let cur = self.snapshot(&label);
        self.undo.push(cur);
        self.restore(s);
        self.merge_key = None;
        self.changed();
        Some(label)
    }

    /// Forget undo history (after opening a package).
    pub fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.merge_key = None;
        self.group = None;
    }

    /// A decoder for the package's bitmap resources (memoized, shareable with worker threads).
    pub fn resource_decoder(&self) -> impl Fn(&str) -> Option<Image> + Sync + Send + 'static {
        let res: Vec<(String, ResourceBytes)> = self.pkg.resources.iter().map(|r| (r.name.clone(), r.bytes.clone())).collect();
        let memo = self.decoded.clone();
        // Free images of resources that were deleted or replaced.
        memo.lock().unwrap_or_else(PoisonError::into_inner).retain(|(k, _), _| res.iter().any(|(_, b)| b.content_hash() == *k));
        move |requested: &str| {
            let (name, linear) = materialcraft_image::split_resource_key(requested);
            let (_, bytes) = res.iter().find(|(n, _)| n == name)?;
            let key = (bytes.content_hash(), linear);
            if let Some(i) = memo.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
                return Some(i.clone());
            }
            let img = materialcraft_formats::decode(bytes.as_slice(), linear).ok()?;
            memo.lock().unwrap_or_else(PoisonError::into_inner).insert(key, img.clone());
            Some(img)
        }
    }

    /// Forget every decoded resource image (the package was replaced).
    pub(crate) fn clear_decoded(&self) {
        self.decoded.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    /// Cook the open graph now (at the preview size), on this thread.
    pub fn cook(&mut self) -> &Cooked {
        let dec = self.resource_decoder();
        let mut opts = Options::new(&dec);
        opts.max_size = self.cook_size(self.preview_size);
        self.cooked = materialcraft_eval::cook(&self.pkg, self.graph, &opts, &self.cache);
        self.dirty = false;
        self.cook_epoch = self.cook_epoch.wrapping_add(1);
        &self.cooked
    }

    /// Cook graph `gi` at full size, without touching the preview results.
    pub fn cook_full(&self, gi: usize) -> Cooked {
        let dec = self.resource_decoder();
        let mut opts = Options::new(&dec);
        opts.max_size = self.cook_limit;
        materialcraft_eval::cook(&self.pkg, gi, &opts, &self.cache)
    }

    /// Cook if the graph changed since the last cook.
    pub fn cook_if_dirty(&mut self) {
        if self.dirty {
            self.cook();
        }
    }

    /// The image the 2D view shows: the chosen node's port, else the first output node, else the
    /// last selected node.
    pub fn view2d_image(&self) -> Option<(NodeId, &Image)> {
        let g = self.graph().ok()?;
        let id = self
            .view2d
            .node
            .filter(|id| g.node(*id).is_ok())
            .or_else(|| self.selection.last().copied())
            .or_else(|| g.nodes.iter().find(|n| n.def == "output").map(|n| n.id))?;
        let port = if Some(id) == self.view2d.node { self.view2d.port } else { 0 };
        self.cooked.image(id, port).or_else(|| self.cooked.image(id, 0)).map(|i| (id, i))
    }

    /// Use new 3D camera preferences, pulling the orbit target back within a lowered pan limit.
    pub fn set_camera_prefs(&mut self, prefs: CameraPrefs) {
        self.camera = prefs.sanitized();
        self.view3d.target = self.camera.clamp_target(self.view3d.target);
    }

    /// The open graph's material channels for the 3D view: usage id → image.
    pub fn material_maps(&self) -> Vec<(&'static str, &Image)> {
        let Ok(g) = self.graph() else { return Vec::new() };
        materialcraft_eval::outputs(g, &self.cooked).into_iter().filter_map(|(_, usage, _, img)| Some((usage, img?))).collect()
    }

    /// Show a mesh file in the 3D view (OBJ, glTF, GLB).
    pub fn load_mesh_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let mesh = materialcraft_formats::mesh::load_mesh(name, bytes)?;
        self.custom_mesh = Some((name.to_string(), Arc::new(mesh)));
        self.view3d.scene = "custom".into();
        Ok(())
    }

    /// Nodes of the open graph whose last cook failed, with the error, and how many placeholders for missing SBS nodes failed besides.
    pub fn cook_failures(&self) -> (Vec<(NodeId, &str)>, usize) {
        let mut failed = Vec::new();
        let mut placeholders = 0;
        for node in self.graph().map(|g| g.nodes.as_slice()).unwrap_or_default() {
            let Some(error) = self.cooked.nodes.get(&node.id).and_then(|r| r.error.as_deref()) else { continue };
            if node.def == materialcraft_core::MISSING_NODE {
                placeholders += 1;
            } else {
                failed.push((node.id, error));
            }
        }
        (failed, placeholders)
    }

    /// Log a warning, shown highlighted in the Console.
    pub fn warn(&mut self, msg: impl Into<String>) {
        self.log(format!("{WARNING_PREFIX}{}", msg.into()));
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }
}

#[cfg(test)]
#[path = "session.test.rs"]
mod tests;
