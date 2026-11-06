//! The 3D preview's plain-data description shared by the engine (where it is session state)
//! and the renderer: what shows behind the mesh, the lights, the shading model and the
//! material's common values.

use serde::{Deserialize, Serialize};

/// What shows where a camera ray misses the mesh.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Background {
    /// The studio vignette.
    Backdrop,
    /// A flat color (linear RGB).
    Color([f32; 3]),
    /// The (blurred) environment map.
    Environment,
}

/// A point light in the scene (position in scene radii from the center).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PointLight {
    pub on: bool,
    /// Linear RGB.
    pub color: [f32; 3],
    pub intensity: f32,
    pub position: [f32; 3],
}

impl PointLight {
    /// The two default lights: key from the upper left, fill from the right and behind.
    pub const DEFAULTS: [PointLight; 2] = [
        PointLight { on: false, color: [1.0; 3], intensity: 1.0, position: [-2.5, 3.0, 2.5] },
        PointLight { on: false, color: [0.2; 3], intensity: 1.0, position: [3.0, 1.0, -2.0] },
    ];

    /// Every value brought into its range; non-finite values become the default.
    pub fn sanitized(mut self, default: PointLight) -> PointLight {
        for (v, d) in self.color.iter_mut().zip(default.color) {
            *v = if v.is_finite() { v.clamp(0.0, 64.0) } else { d };
        }
        self.intensity = if self.intensity.is_finite() { self.intensity.clamp(0.0, 100.0) } else { default.intensity };
        for (v, d) in self.position.iter_mut().zip(default.position) {
            *v = if v.is_finite() { v.clamp(-50.0, 50.0) } else { d };
        }
        self
    }
}

impl Default for PointLight {
    fn default() -> Self {
        PointLight::DEFAULTS[0]
    }
}

/// The shading model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shader {
    /// Metallic/roughness with coat, fuzz, emission, thin film, transmission and subsurface terms.
    OpenPbr,
    /// Plain metallic/roughness.
    MetallicRoughness,
    /// Metallic/roughness with the coat layer.
    Coated,
    /// Metallic/roughness with the subsurface term.
    Subsurface,
    /// Specular color / glossiness maps instead of metallic / roughness.
    SpecularGlossiness,
    /// Lambert diffuse plus a Blinn-Phong highlight from the lights.
    Blinn,
    /// Diffuse only.
    Lambert,
    /// The base color as is.
    Unlit,
    /// Normals as colors (surface inspection).
    MeshInfo,
}

impl Shader {
    pub const ALL: [(Shader, &'static str); 9] = [
        (Shader::OpenPbr, "openPbr"),
        (Shader::MetallicRoughness, "metallicRoughness"),
        (Shader::Coated, "coated"),
        (Shader::Subsurface, "subsurface"),
        (Shader::SpecularGlossiness, "specularGlossiness"),
        (Shader::Blinn, "blinn"),
        (Shader::Lambert, "lambert"),
        (Shader::Unlit, "unlit"),
        (Shader::MeshInfo, "meshInfo"),
    ];

    pub fn id(self) -> &'static str {
        Self::ALL.iter().find(|(s, _)| *s == self).map_or("openPbr", |(_, id)| id)
    }

    pub fn by_id(id: &str) -> Option<Shader> {
        Self::ALL.iter().find(|(_, i)| *i == id).map(|(s, _)| *s)
    }
}

/// Which maps drive the material (off = the common value is used even when the map exists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MapUse {
    pub base_color: bool,
    pub roughness: bool,
    pub metallic: bool,
    pub normal: bool,
    pub height: bool,
    pub ao: bool,
    pub emissive: bool,
    pub opacity: bool,
    pub specular: bool,
}

impl Default for MapUse {
    fn default() -> Self {
        MapUse { base_color: true, roughness: true, metallic: true, normal: true, height: true, ao: true, emissive: true, opacity: true, specular: true }
    }
}

/// The material's common values: what each channel is where no map drives it, and the layer
/// terms that have no map at all. Colors are linear RGB; weights 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Common {
    pub base_weight: f32,
    pub base_color: [f32; 3],
    pub metalness: f32,
    pub diffuse_roughness: f32,
    pub specular_weight: f32,
    pub specular_color: [f32; 3],
    pub specular_roughness: f32,
    pub ior: f32,
    pub transmission_weight: f32,
    pub transmission_color: [f32; 3],
    pub subsurface_weight: f32,
    pub subsurface_color: [f32; 3],
    pub coat_weight: f32,
    pub coat_color: [f32; 3],
    pub coat_roughness: f32,
    pub coat_ior: f32,
    pub coat_darkening: f32,
    pub fuzz_weight: f32,
    pub fuzz_color: [f32; 3],
    pub emission_luminance: f32,
    pub emission_color: [f32; 3],
    pub thin_film_weight: f32,
    /// Nanometres.
    pub thin_film_thickness: f32,
    pub thin_film_ior: f32,
    pub opacity: f32,
    pub thin_walled: bool,
    pub ambient_occlusion: f32,
    pub height: f32,
}

impl Default for Common {
    fn default() -> Self {
        Common {
            base_weight: 1.0,
            base_color: [0.5; 3],
            metalness: 0.0,
            diffuse_roughness: 0.0,
            specular_weight: 1.0,
            specular_color: [1.0; 3],
            specular_roughness: 0.5,
            ior: 1.5,
            transmission_weight: 0.0,
            transmission_color: [1.0; 3],
            subsurface_weight: 0.0,
            subsurface_color: [0.8; 3],
            coat_weight: 0.0,
            coat_color: [1.0; 3],
            coat_roughness: 0.0,
            coat_ior: 1.6,
            coat_darkening: 1.0,
            fuzz_weight: 0.0,
            fuzz_color: [1.0; 3],
            emission_luminance: 0.0,
            emission_color: [1.0; 3],
            thin_film_weight: 0.0,
            thin_film_thickness: 500.0,
            thin_film_ior: 1.4,
            opacity: 1.0,
            thin_walled: false,
            ambient_occlusion: 1.0,
            height: 0.5,
        }
    }
}

impl Common {
    /// Every value brought into its range; non-finite values become the default.
    pub fn sanitized(mut self) -> Common {
        let d = Common::default();
        let unit = |v: &mut f32, dv: f32| *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { dv };
        let col = |c: &mut [f32; 3], dc: [f32; 3]| {
            for (v, dv) in c.iter_mut().zip(dc) {
                *v = if v.is_finite() { v.clamp(0.0, 64.0) } else { dv };
            }
        };
        unit(&mut self.base_weight, d.base_weight);
        col(&mut self.base_color, d.base_color);
        unit(&mut self.metalness, d.metalness);
        unit(&mut self.diffuse_roughness, d.diffuse_roughness);
        unit(&mut self.specular_weight, d.specular_weight);
        col(&mut self.specular_color, d.specular_color);
        unit(&mut self.specular_roughness, d.specular_roughness);
        self.ior = if self.ior.is_finite() { self.ior.clamp(1.0, 3.0) } else { d.ior };
        unit(&mut self.transmission_weight, d.transmission_weight);
        col(&mut self.transmission_color, d.transmission_color);
        unit(&mut self.subsurface_weight, d.subsurface_weight);
        col(&mut self.subsurface_color, d.subsurface_color);
        unit(&mut self.coat_weight, d.coat_weight);
        col(&mut self.coat_color, d.coat_color);
        unit(&mut self.coat_roughness, d.coat_roughness);
        self.coat_ior = if self.coat_ior.is_finite() { self.coat_ior.clamp(1.0, 3.0) } else { d.coat_ior };
        unit(&mut self.coat_darkening, d.coat_darkening);
        unit(&mut self.fuzz_weight, d.fuzz_weight);
        col(&mut self.fuzz_color, d.fuzz_color);
        self.emission_luminance = if self.emission_luminance.is_finite() { self.emission_luminance.clamp(0.0, 100.0) } else { d.emission_luminance };
        col(&mut self.emission_color, d.emission_color);
        unit(&mut self.thin_film_weight, d.thin_film_weight);
        self.thin_film_thickness = if self.thin_film_thickness.is_finite() { self.thin_film_thickness.clamp(0.0, 2000.0) } else { d.thin_film_thickness };
        self.thin_film_ior = if self.thin_film_ior.is_finite() { self.thin_film_ior.clamp(1.0, 3.0) } else { d.thin_film_ior };
        unit(&mut self.opacity, d.opacity);
        unit(&mut self.ambient_occlusion, d.ambient_occlusion);
        unit(&mut self.height, d.height);
        self
    }
}
