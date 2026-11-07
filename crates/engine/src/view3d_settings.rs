//! The 3D view's property pages as session state: camera, lights, renderer, display toggles and
//! the material's common values. Every field is clamped by `sanitize` after a command merges JSON in.

use serde::{Deserialize, Serialize};

pub use materialcraft_geom::preview::{Common, MapUse, PointLight, Shader};

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

    pub(crate) fn sanitize(&mut self) {
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
    pub point: [PointLight; 2],
}

impl Default for Lights3d {
    fn default() -> Self {
        Lights3d { ambient_on: true, ambient: [0.0; 3], point: PointLight::DEFAULTS }
    }
}

impl Lights3d {
    pub(crate) fn sanitize(&mut self) {
        for v in self.ambient.iter_mut() {
            *v = if v.is_finite() { v.clamp(0.0, 16.0) } else { 0.0 };
        }
        let d = PointLight::DEFAULTS;
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
    pub(crate) fn sanitize(&mut self) {
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
    pub shader: Shader,
    pub common: Common,
    pub use_map: MapUse,
    /// Displacement subdivision budget multiplier (0.1..4).
    pub tessellation: f32,
    /// Extra UV scale of the material on the mesh.
    pub uv_scale: [f32; 2],
}

impl Default for Material3d {
    fn default() -> Self {
        Material3d { shader: Shader::OpenPbr, common: Common::default(), use_map: MapUse::default(), tessellation: 1.0, uv_scale: [1.0, 1.0] }
    }
}

impl Material3d {
    pub(crate) fn sanitize(&mut self) {
        self.common = self.common.sanitized();
        self.tessellation = if self.tessellation.is_finite() { self.tessellation.clamp(0.1, 4.0) } else { 1.0 };
        self.uv_scale = self.uv_scale.map(|v| if v.is_finite() { v.clamp(0.01, 64.0) } else { 1.0 });
    }
}
