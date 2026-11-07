//! Portable materials (`.mcmat`): a cooked material other apps (TextureCraft) apply without a
//! graph engine, plus the package it came from so an app that can run `materialcraft-cli`
//! re-cooks it with other parameter values, sizes and mesh maps.
//!
//! A zip: `material.json` ([`Manifest`]), `maps/<usage>.png` (8-bit sRGB for color usages,
//! 16-bit linear otherwise; normals in the OpenGL convention) and `source.mcraft`.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use materialcraft_core::{ExposedParam, Value};
use serde::{Deserialize, Serialize};

use crate::{FormatError, MAX_JSON, MAX_RESOURCE, Result};

/// The portable material extension.
pub const MATERIAL_EXT: &str = "mcmat";
/// `format` of every manifest.
pub const FORMAT: &str = "craft-material";
/// Newest manifest version this build reads and writes.
pub const VERSION: u32 = 1;
const MANIFEST: &str = "material.json";
const SOURCE: &str = "source.mcraft";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub name: String,
    /// The graph of the source package the maps were cooked from.
    pub graph: String,
    /// Size of the maps in pixels.
    pub size: [u32; 2],
    pub maps: Vec<MapEntry>,
    /// Parameters other apps may change (scalar, vector, color, toggle and option inputs).
    pub parameters: Vec<Parameter>,
    /// Image inputs of the graph, fed from mesh maps by identifier.
    pub inputs: Vec<ImageInput>,
    #[serde(default)]
    pub presets: Vec<PresetEntry>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapEntry {
    /// Usage id: `baseColor`, `normal`, `roughness`, `metallic`, `height`, `ambientOcclusion`,
    /// `emissive`, `opacity`, `specular`.
    pub usage: String,
    pub file: String,
    /// `srgb` or `linear`.
    pub color_space: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    #[serde(flatten)]
    pub param: ExposedParam,
    /// The value the maps were cooked with.
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageInput {
    pub identifier: String,
    /// `gray` or `color`.
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetEntry {
    pub name: String,
    pub values: BTreeMap<String, Value>,
}

/// Whether maps of `usage` hold colors (stored sRGB-encoded) rather than data (linear).
pub fn srgb_usage(usage: &str) -> bool {
    matches!(usage, "baseColor" | "emissive" | "specular")
}

/// Whether other apps can set a value of this type from the manifest.
pub fn portable(v: &Value) -> bool {
    matches!(
        v,
        Value::Float(_)
            | Value::Float2(_)
            | Value::Float3(_)
            | Value::Float4(_)
            | Value::Int(_)
            | Value::Int2(_)
            | Value::Bool(_)
            | Value::Color(_)
            | Value::Enum(_)
    )
}

/// Write a portable material: `manifest` (its `source` is set here), the encoded maps named by
/// `manifest.maps[].file`, and the source package bytes.
pub fn write_material(manifest: &Manifest, maps: &[(String, Vec<u8>)], source: &[u8]) -> Result<Vec<u8>> {
    let io = |e: zip::result::ZipError| FormatError::Io(e.to_string());
    let manifest = Manifest { source: SOURCE.into(), ..manifest.clone() };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| FormatError::Io(e.to_string()))?;
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // Maps and the package are already compressed.
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file(MANIFEST, deflate).map_err(io)?;
        z.write_all(&json).map_err(|e| FormatError::Io(e.to_string()))?;
        for (file, bytes) in maps {
            z.start_file(file.as_str(), stored).map_err(io)?;
            z.write_all(bytes).map_err(|e| FormatError::Io(e.to_string()))?;
        }
        z.start_file(SOURCE, stored).map_err(io)?;
        z.write_all(source).map_err(|e| FormatError::Io(e.to_string()))?;
        z.finish().map_err(io)?;
    }
    Ok(buf.into_inner())
}

/// Read a portable material's manifest and source package bytes.
pub fn read_material(bytes: &[u8]) -> Result<(Manifest, Vec<u8>)> {
    let bad = |m: String| FormatError::BadPackage(format!("portable material: {m}"));
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(e.to_string()))?;
    let mut read = |name: &str, cap: u64| -> Result<Vec<u8>> {
        let mut data = Vec::new();
        z.by_name(name).map_err(|_| bad(format!("{name} is missing")))?.take(cap).read_to_end(&mut data).map_err(|e| FormatError::Io(e.to_string()))?;
        Ok(data)
    };
    let manifest: Manifest = serde_json::from_slice(&read(MANIFEST, MAX_JSON)?).map_err(|e| bad(e.to_string()))?;
    if manifest.format != FORMAT || manifest.version > VERSION {
        return Err(bad(format!("unsupported {} version {}", manifest.format, manifest.version)));
    }
    let source = read(&manifest.source, MAX_RESOURCE)?;
    Ok((manifest, source))
}

/// Whether `bytes` look like a portable material (a zip with `material.json`).
pub fn is_material(bytes: &[u8]) -> bool {
    zip::ZipArchive::new(Cursor::new(bytes)).is_ok_and(|mut z| z.by_name(MANIFEST).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        let param = ExposedParam {
            id: "rough".into(),
            label: "Roughness".into(),
            group: String::new(),
            default: Value::Float(0.5),
            min: 0.0,
            max: 1.0,
            options: Vec::new(),
        };
        Manifest {
            format: FORMAT.into(),
            version: VERSION,
            name: "Bricks".into(),
            graph: "Bricks".into(),
            size: [64, 64],
            maps: vec![MapEntry { usage: "baseColor".into(), file: "maps/baseColor.png".into(), color_space: "srgb".into() }],
            parameters: vec![Parameter { param, value: Value::Float(0.25) }],
            inputs: vec![ImageInput { identifier: "curvature".into(), kind: "gray".into() }],
            presets: Vec::new(),
            source: String::new(),
        }
    }

    #[test]
    fn round_trips_and_rejects_newer_versions() {
        let m = manifest();
        let bytes = write_material(&m, &[("maps/baseColor.png".into(), vec![1, 2, 3])], b"pkg").unwrap();
        assert!(is_material(&bytes));
        let (back, source) = read_material(&bytes).unwrap();
        assert_eq!(back, Manifest { source: SOURCE.into(), ..m.clone() }, "the writer names the source entry");
        assert_eq!(source, b"pkg");
        // Parameters are flat objects with the value in the API's typed form.
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["parameters"][0]["id"], "rough");
        assert_eq!(json["parameters"][0]["value"], serde_json::json!({"t": "float", "v": 0.25}));

        let newer = Manifest { version: VERSION + 1, ..m };
        assert!(read_material(&write_material(&newer, &[], b"").unwrap()).is_err());
        assert!(!is_material(b"not a zip"));
        assert!(read_material(b"garbage").is_err());
    }
}
