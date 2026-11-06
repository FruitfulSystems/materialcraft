//! # materialcraft-formats
//!
//! File formats, L2: SBS packages (`.sbs`, the main project format, see [`sbs`]), the older native `.mcraft` package (a zip holding `package.json` and the embedded
//! `resources/`), bitmap decoding for Bitmap nodes, and texture export (PNG 8/16, JPEG, TGA,
//! TIFF 8/16, BMP, WEBP, EXR 32F, HDR) with the output naming pattern, and meshes for the 3D view
//! (OBJ, glTF/GLB), and portable materials (`.mcmat`, see [`material`]). Byte-based APIs work
//! everywhere, including the browser; path helpers are native only.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod material;
pub mod mesh;
pub mod sbs;

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use materialcraft_core::{OutputFormat, Package, ResourceBytes};
use materialcraft_image::{Image, Kind, MAX_SIDE, linear_to_srgb, srgb_to_linear};

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not a MaterialCraft package: {0}")]
    BadPackage(String),
    #[error("image: {0}")]
    Image(String),
    #[error("I/O: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, FormatError>;

/// Largest `package.json` and resource accepted when reading (allocation caps).
const MAX_JSON: u64 = 64 << 20;
const MAX_RESOURCE: u64 = 256 << 20;
/// Most resource records a package may list.
const MAX_RESOURCES: usize = 4096;
/// Most decompressed bytes a package may hold across `package.json` and all its resources.
const MAX_PACKAGE_BYTES: u64 = 2 << 30;

/// Export file types: (extension, label).
pub const EXPORT_FORMATS: &[(&str, &str)] =
    &[("png", "PNG"), ("jpg", "JPEG"), ("tga", "TGA"), ("tif", "TIFF"), ("bmp", "BMP"), ("webp", "WEBP"), ("exr", "OpenEXR"), ("hdr", "Radiance HDR")];

/// Image file extensions [`decode`] reads.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "tif", "tiff", "bmp", "webp", "exr", "hdr"];

/// Write a package to `.mcraft` bytes. Resources whose sanitized file names collide get a numeric
/// suffix (`x.png`, `x-2.png`), recorded in the saved `package.json` so they load back.
pub fn save_package(pkg: &Package) -> Result<Vec<u8>> {
    let mut pkg = pkg.clone();
    if pkg.resources.len() > MAX_RESOURCES {
        return Err(FormatError::BadPackage("too many resources to save".into()));
    }
    let mut used: HashMap<String, (String, ResourceBytes)> = HashMap::new();
    for r in &mut pkg.resources {
        let base = safe_file_name(&r.file);
        let mut name = base.clone();
        let mut n = 2u32;
        while let Some((existing, bytes)) = used.get(&name.to_lowercase()) {
            if bytes == &r.bytes {
                name = existing.clone();
                break;
            }
            name = match base.rsplit_once('.') {
                Some((stem, ext)) if !stem.is_empty() => format!("{stem}-{n}.{ext}"),
                _ => format!("{base}-{n}"),
            };
            n = n.saturating_add(1);
        }
        used.entry(name.to_lowercase()).or_insert_with(|| (name.clone(), r.bytes.clone()));
        r.file = name;
    }
    let json = serde_json::to_vec_pretty(&pkg).map_err(|e| FormatError::Io(e.to_string()))?;
    let total = used.values().try_fold(json.len() as u64, |total, (_, bytes)| {
        if bytes.len() as u64 > MAX_RESOURCE {
            return None;
        }
        total.checked_add(bytes.len() as u64).filter(|sum| *sum <= MAX_PACKAGE_BYTES)
    });
    if json.len() as u64 > MAX_JSON || total.is_none() {
        return Err(FormatError::BadPackage("package is too large to save".into()));
    }
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let io = |e: zip::result::ZipError| FormatError::Io(e.to_string());
        z.start_file("package.json", opts).map_err(io)?;
        z.write_all(&json).map_err(|e| FormatError::Io(e.to_string()))?;
        let mut written = std::collections::HashSet::new();
        for r in &pkg.resources {
            if !written.insert(&r.file) {
                continue;
            }
            // Images are already compressed.
            let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file(format!("resources/{}", safe_file_name(&r.file)), stored).map_err(io)?;
            z.write_all(r.bytes.as_slice()).map_err(|e| FormatError::Io(e.to_string()))?;
        }
        z.finish().map_err(io)?;
    }
    Ok(buf.into_inner())
}

/// Read a package from `.mcraft` bytes. Each distinct resource entry is read once and shared by
/// every resource naming it; a missing entry, an entry over the size cap, too many resources or
/// too many total bytes is an error (never silently empty or truncated data).
pub fn load_package(bytes: &[u8]) -> Result<Package> {
    load_package_within(bytes, MAX_PACKAGE_BYTES)
}

/// [`load_package`] with an explicit cap on the total decompressed bytes.
fn load_package_within(bytes: &[u8], max_total: u64) -> Result<Package> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| FormatError::BadPackage(e.to_string()))?;
    let mut json = Vec::new();
    let mut entry = z.by_name("package.json").map_err(|_| FormatError::BadPackage("package.json is missing".into()))?;
    let cap = MAX_JSON.min(max_total);
    if entry.size() > cap {
        return Err(FormatError::BadPackage("package.json is too large".into()));
    }
    entry.by_ref().take(cap + 1).read_to_end(&mut json).map_err(|e| FormatError::Io(e.to_string()))?;
    if json.len() as u64 > cap {
        return Err(FormatError::BadPackage("package.json is too large".into()));
    }
    drop(entry);
    let mut pkg: Package = serde_json::from_slice(&json).map_err(|e| FormatError::BadPackage(e.to_string()))?;
    if pkg.resources.len() > MAX_RESOURCES {
        return Err(FormatError::BadPackage(format!("{} resources (limit {MAX_RESOURCES})", pkg.resources.len())));
    }
    let mut total = json.len() as u64;
    let mut read: HashMap<String, ResourceBytes> = HashMap::new();
    for r in &mut pkg.resources {
        let name = format!("resources/{}", safe_file_name(&r.file));
        if let Some(shared) = read.get(&name) {
            r.bytes = shared.clone();
            continue;
        }
        let mut data = Vec::new();
        let mut f = z.by_name(&name).map_err(|_| FormatError::BadPackage(format!("resource `{}` ({name}) is missing from the package", r.name)))?;
        let cap = MAX_RESOURCE.min(max_total.saturating_sub(total));
        if f.size() > cap {
            return Err(FormatError::BadPackage(format!("package resources exceed the size limit ({max_total} bytes)")));
        }
        f.by_ref().take(cap + 1).read_to_end(&mut data).map_err(|e| FormatError::Io(e.to_string()))?;
        if data.len() as u64 > cap {
            return Err(FormatError::BadPackage(format!("package resources exceed the size limit ({max_total} bytes)")));
        }
        total = total.saturating_add(data.len() as u64);
        if total > max_total {
            return Err(FormatError::BadPackage(format!("package resources exceed {max_total} bytes")));
        }
        let data = ResourceBytes::new(data);
        read.insert(name, data.clone());
        r.bytes = data;
    }
    Ok(pkg)
}

/// Last-resort guard around third-party parsers: a panic inside them becomes an error.
pub(crate) fn guard<T>(f: impl FnOnce() -> Result<T>, err: impl FnOnce() -> FormatError) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err(err()))
}

/// A file name with no path parts (resources live flat in `resources/`).
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() { "resource".into() } else { cleaned }
}

/// Decode to float pixels, preserving data-map values when `linear` is true.
/// Otherwise 8/16-bit color is sRGB-decoded; float and grayscale files keep their values.
pub fn decode(bytes: &[u8], linear: bool) -> Result<Image> {
    let open = || -> Result<image::ImageReader<Cursor<&[u8]>>> {
        let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| FormatError::Image(e.to_string()))?;
        if reader.format().is_none() {
            reader.set_format(image::ImageFormat::Tga);
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(1 << 30);
        reader.limits(limits);
        Ok(reader)
    };
    let failed = || FormatError::Image("the decoder failed on this file".into());
    let image_error = |e| match e {
        image::ImageError::Limits(_) => FormatError::Image(format!("image is larger than {MAX_SIDE} pixels")),
        e => FormatError::Image(e.to_string()),
    };
    let (w, h) = guard(|| open()?.into_dimensions().map_err(image_error), failed)?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(FormatError::Image(format!("{w}x{h} is outside 1..={MAX_SIDE} pixels")));
    }
    let img = guard(|| open()?.decode().map_err(image_error), failed)?;
    let gray = matches!(img.color(), image::ColorType::L8 | image::ColorType::L16 | image::ColorType::La8 | image::ColorType::La16);
    let float = matches!(img.color(), image::ColorType::Rgb32F | image::ColorType::Rgba32F);
    if gray {
        let l = img.into_luma16();
        let data: Vec<f32> = l.pixels().map(|p| p.0[0] as f32 / 65535.0).collect();
        return Ok(Image { w, h, kind: Kind::Gray, data: Arc::new(data) });
    }
    let mut data = img.into_rgba32f().into_raw();
    if !(float || linear) {
        for px in data.chunks_exact_mut(4) {
            for c in px.iter_mut().take(3) {
                *c = srgb_to_linear(*c);
            }
        }
    }
    Ok(Image { w, h, kind: Kind::Color, data: Arc::new(data) })
}

/// Bit depth for export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    U8,
    U16,
    F32,
}

impl Depth {
    pub fn from_format(f: OutputFormat) -> Depth {
        match f {
            OutputFormat::U8 => Depth::U8,
            OutputFormat::F16 | OutputFormat::F32 => Depth::F32,
            _ => Depth::U16,
        }
    }
}

/// Encode `img` as `ext` (see [`EXPORT_FORMATS`]). `srgb`: color values are written through the
/// sRGB curve (base color); data maps (normal, roughness…) are written linear.
pub fn encode(img: &Image, ext: &str, depth: Depth, srgb: bool) -> Result<Vec<u8>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let enc = |v: f32| if srgb && img.kind == Kind::Color { linear_to_srgb(v) } else { v };
    let (w, h) = (img.w, img.h);
    let color = img.kind == Kind::Color;
    let px = |i: usize| -> [f32; 4] {
        if color {
            let s = img.data.get(i * 4..i * 4 + 4).unwrap_or(&[0.0, 0.0, 0.0, 1.0]);
            [enc(s[0]), enc(s[1]), enc(s[2]), s[3]]
        } else {
            let v = img.data.get(i).copied().unwrap_or(0.0);
            [v, v, v, 1.0]
        }
    };
    let n = (w * h) as usize;
    let fmt = match ext.as_str() {
        "png" => image::ImageFormat::Png,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "tga" => image::ImageFormat::Tga,
        "tif" | "tiff" => image::ImageFormat::Tiff,
        "bmp" => image::ImageFormat::Bmp,
        "webp" => image::ImageFormat::WebP,
        "exr" => image::ImageFormat::OpenExr,
        "hdr" => image::ImageFormat::Hdr,
        other => return Err(FormatError::Image(format!("unsupported export format `{other}`"))),
    };
    let float_only = matches!(fmt, image::ImageFormat::OpenExr | image::ImageFormat::Hdr);
    let sixteen_ok = matches!(fmt, image::ImageFormat::Png | image::ImageFormat::Tiff);
    let q8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let q16 = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
    let dynimg: image::DynamicImage = if float_only {
        let data: Vec<f32> = (0..n)
            .flat_map(|i| {
                let p = px(i);
                [p[0], p[1], p[2]]
            })
            .collect();
        let buf = image::Rgb32FImage::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?;
        buf.into()
    } else if depth != Depth::U8 && sixteen_ok {
        if color {
            let data: Vec<u16> = (0..n).flat_map(|i| px(i).map(q16)).collect();
            image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?.into()
        } else {
            let data: Vec<u16> = (0..n).map(|i| q16(px(i)[0])).collect();
            image::ImageBuffer::<image::Luma<u16>, _>::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?.into()
        }
    } else if color && !matches!(fmt, image::ImageFormat::Jpeg) {
        let data: Vec<u8> = (0..n).flat_map(|i| px(i).map(q8)).collect();
        image::RgbaImage::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?.into()
    } else if color {
        let data: Vec<u8> = (0..n)
            .flat_map(|i| {
                let p = px(i);
                [q8(p[0]), q8(p[1]), q8(p[2])]
            })
            .collect();
        image::RgbImage::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?.into()
    } else {
        let data: Vec<u8> = (0..n).map(|i| q8(px(i)[0])).collect();
        image::GrayImage::from_raw(w, h, data).ok_or_else(|| FormatError::Image("bad buffer".into()))?.into()
    };
    let mut out = Cursor::new(Vec::new());
    dynimg.write_to(&mut out, fmt).map_err(|e| FormatError::Image(e.to_string()))?;
    Ok(out.into_inner())
}

/// The file stem of an exported output: `pattern` with `$(graph)`, `$(identifier)` and
/// `$(usage)` replaced, sanitized for file systems.
pub fn output_name(pattern: &str, graph: &str, identifier: &str, usage: &str) -> String {
    let s = pattern.replace("$(graph)", graph).replace("$(identifier)", identifier).replace("$(usage)", usage);
    safe_file_name(&s)
}

/// Write `bytes` to `path` through a temporary file and a rename, so a failed write never leaves
/// a truncated file behind.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    write_atomic_counted(path, bytes, &COUNTER)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_atomic_counted(path: &std::path::Path, bytes: &[u8], counter: &std::sync::atomic::AtomicU64) -> Result<()> {
    for _ in 0..32 {
        let unique = format!(".{}.{}.tmp", std::process::id(), counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "out".into());
        name.push(unique);
        let tmp = path.with_file_name(name);
        let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(FormatError::Io(format!("{}: {e}", tmp.display()))),
        };
        let result = f.write_all(bytes).and_then(|()| f.sync_all());
        drop(f);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(FormatError::Io(format!("{}: {e}", tmp.display())));
        }
        return std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            FormatError::Io(format!("{}: {e}", path.display()))
        });
    }
    Err(FormatError::Io("could not create an exclusive temporary save file after 32 attempts".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_core::{Graph, Resource};

    #[test]
    fn package_round_trip() {
        let mut p = Package::new("Test");
        let mut g = Graph::new("Bricks");
        let a = g.add_node("brick_generator", [0.0, 0.0]);
        let o = g.add_node("output", [200.0, 0.0]);
        g.connect(a, "output", o, "input").unwrap();
        p.graphs.push(g);
        let png = encode(&Image::filled(4, 4, Kind::Color, [1.0, 0.5, 0.0, 1.0]), "png", Depth::U8, true).unwrap();
        p.resources.push(Resource::new("tex", "../../evil/tex.png", ResourceBytes::new(png)));
        let bytes = save_package(&p).unwrap();
        let q = load_package(&bytes).unwrap();
        assert_eq!(q.graphs, p.graphs);
        assert_eq!(q.resources[0].bytes, p.resources[0].bytes);
        let img = decode(q.resources[0].bytes.as_slice(), false).unwrap();
        assert!((img.get(0, 0, false)[1] - 0.5).abs() < 0.01);
        assert!(load_package(b"not a zip").is_err());
    }

    fn zip_resources(resources: &[(&str, &str)], entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut p = Package::new("T");
        for (n, f) in resources {
            p.resources.push(Resource::new(*n, *f, ResourceBytes::default()));
        }
        let json = serde_json::to_vec(&p).unwrap();
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("package.json", o).unwrap();
            z.write_all(&json).unwrap();
            for (n, b) in entries {
                z.start_file(*n, o).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn many_resources_naming_one_entry_are_capped_and_shared() {
        let many: Vec<(String, String)> = (0..MAX_RESOURCES + 1).map(|i| (format!("r{i}"), "big.png".to_string())).collect();
        let refs: Vec<(&str, &str)> = many.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert!(load_package(&zip_resources(&refs, &[("resources/big.png", &[1u8; 16])])).is_err());
        let few = [("a", "big.png"), ("b", "big.png"), ("c", "big.png")];
        let p = load_package(&zip_resources(&few, &[("resources/big.png", &[1u8; 1024])])).unwrap();
        assert!(p.resources[0].bytes.ptr_eq(&p.resources[2].bytes));
    }

    #[test]
    fn saving_shared_aliases_writes_the_resource_only_once() {
        let aliases: Vec<_> = (0..1000).map(|i| (format!("r{i}"), "same.png".to_string())).collect();
        let refs: Vec<_> = aliases.iter().map(|(n, f)| (n.as_str(), f.as_str())).collect();
        let p = load_package(&zip_resources(&refs, &[("resources/same.png", &[42; 16])])).unwrap();
        let saved = save_package(&p).unwrap();
        assert_eq!(zip::ZipArchive::new(Cursor::new(&saved)).unwrap().len(), 2);
        let q = load_package(&saved).unwrap();
        assert_eq!(q.resources.len(), 1000);
        assert!(q.resources[0].bytes.ptr_eq(&q.resources[999].bytes));
    }

    #[test]
    fn missing_resource_entry_is_an_error() {
        assert!(load_package(&zip_resources(&[("a", "gone.png")], &[])).is_err());
    }

    #[test]
    fn colliding_resource_file_names_save_and_load() {
        let mut p = Package::new("T");
        for (n, f, b) in [("a", "a/x.png", 1u8), ("b", "b/x.png", 2)] {
            p.resources.push(Resource::new(n, f, ResourceBytes::new(vec![b; 8])));
        }
        let q = load_package(&save_package(&p).unwrap()).unwrap();
        assert_eq!(q.resources[0].bytes.as_slice()[0], 1);
        assert_eq!(q.resources[1].bytes.as_slice()[0], 2);
        assert_ne!(q.resources[0].file, q.resources[1].file);
    }

    #[test]
    fn export_every_format() {
        let img = Image::generate(8, 8, Kind::Color, |x, y| [x as f32 / 7.0, y as f32 / 7.0, 0.5, 1.0]);
        let gray = img.to_gray();
        for (ext, _) in EXPORT_FORMATS {
            for depth in [Depth::U8, Depth::U16, Depth::F32] {
                for i in [&img, &gray] {
                    let b = encode(i, ext, depth, true).unwrap_or_else(|e| panic!("{ext}: {e}"));
                    let back = decode(&b, false).unwrap_or_else(|e| panic!("{ext} decode: {e}"));
                    assert_eq!((back.w, back.h), (8, 8));
                }
            }
        }
        let png16 = encode(&gray, "png", Depth::U16, false).unwrap();
        assert!((decode(&png16, false).unwrap().get(7, 0, false)[0] - gray.get(7, 0, false)[0]).abs() < 1e-3);
    }

    #[test]
    fn linear_decode_keeps_data_map_values() {
        let normal = Image::filled(2, 2, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
        let png = encode(&normal, "png", Depth::U8, false).unwrap();
        let data = decode(&png, true).unwrap();
        assert!((data.get(0, 0, false)[0] - 0.5).abs() < 0.01);
        let color = decode(&png, false).unwrap();
        assert!((color.get(0, 0, false)[0] - 0.214).abs() < 0.01);
    }

    /// A valid 1-bit grayscale PNG of `side` x `side` zero pixels (a few hundred KB when deflated).
    fn blank_png(side: u32) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            out.extend(kind);
            out.extend(data);
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            out.extend(crc.sum().to_be_bytes());
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let row = vec![0u8; 1 + (side as usize).div_ceil(8)];
        for _ in 0..side {
            z.write_all(&row).unwrap();
        }
        let mut ihdr = Vec::new();
        ihdr.extend(side.to_be_bytes());
        ihdr.extend(side.to_be_bytes());
        ihdr.extend([1, 0, 0, 0, 0]);
        let mut png = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'].to_vec();
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &z.finish().unwrap());
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn oversized_images_are_rejected_from_the_header_before_decoding() {
        // 30000 x 30000 is 900 MB as 8-bit gray: the size message (not an allocation failure
        // from the decoder) proves the header was checked before any pixels were decoded.
        let e = decode(&blank_png(30_000), false).unwrap_err().to_string();
        assert!(e.contains("pixels"), "{e}");
        assert!(decode(&blank_png(64), false).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_never_follows_or_removes_a_planted_temp_symlink() {
        let dir = std::env::temp_dir().join(format!("mc-atomic-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mcraft");
        let victim = dir.join("victim");
        std::fs::write(&victim, b"keep").unwrap();
        let planted = dir.join(format!("a.mcraft.{}.0.tmp", std::process::id()));
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        write_atomic_counted(&path, b"saved", &std::sync::atomic::AtomicU64::new(0)).unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        assert!(planted.is_symlink());
        assert_eq!(std::fs::read(&path).unwrap(), b"saved");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_atomic_leaves_no_temp_file_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("mc-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mcraft");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(output_name("$(graph)_$(usage)", "Bricks", "out", "baseColor"), "Bricks_baseColor");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name(".."), "resource");
    }

    fn zip_with(json: &serde_json::Value, entries: &[(&str, usize)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            z.start_file("package.json", opts).unwrap();
            z.write_all(json.to_string().as_bytes()).unwrap();
            for (name, len) in entries {
                z.start_file(format!("resources/{name}"), opts).unwrap();
                z.write_all(&vec![0u8; *len]).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn package_json(files: &[&str]) -> serde_json::Value {
        let base = serde_json::to_value(Package::new("T")).unwrap();
        let mut v = base;
        v["resources"] = files.iter().map(|f| serde_json::json!({ "name": "r", "file": f })).collect();
        v
    }

    #[test]
    fn many_records_share_one_entry() {
        let files = vec!["big.bin"; 1000];
        let bytes = zip_with(&package_json(&files), &[("big.bin", 1 << 20)]);
        let pkg = load_package(&bytes).unwrap();
        assert_eq!(pkg.resources.len(), 1000);
        assert_eq!(pkg.resources[0].bytes.len(), 1 << 20);
        assert!(pkg.resources[0].bytes.ptr_eq(&pkg.resources[999].bytes));
    }

    #[test]
    fn too_many_resources_is_an_error() {
        let files = vec!["a.bin"; MAX_RESOURCES + 1];
        let bytes = zip_with(&package_json(&files), &[("a.bin", 4)]);
        assert!(load_package(&bytes).is_err());
    }

    #[test]
    fn total_budget_is_enforced() {
        let bytes = zip_with(&package_json(&["a.bin", "b.bin"]), &[("a.bin", 1 << 20), ("b.bin", 1 << 20)]);
        assert!(load_package_within(&bytes, 3 << 19).is_err());
        assert!(load_package_within(&bytes, 3 << 20).is_ok());
    }

    #[test]
    fn oversized_images_are_refused_before_decoding() {
        let mut png = Cursor::new(Vec::new());
        image::GrayImage::new(8193, 1).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let err = decode(png.get_ref(), false).unwrap_err().to_string();
        assert!(err.contains("8192"), "{err}");
    }

    #[test]
    fn tga_still_decodes() {
        let mut tga = Cursor::new(Vec::new());
        image::RgbaImage::new(2, 2).write_to(&mut tga, image::ImageFormat::Tga).unwrap();
        let img = decode(tga.get_ref(), false).unwrap();
        assert_eq!((img.w, img.h, img.kind), (2, 2, Kind::Color));
    }

    #[test]
    fn decode_matches_previous_values() {
        let mut png = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(2, 2, image::Rgb([128, 128, 128])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let img = decode(png.get_ref(), false).unwrap();
        let want = srgb_to_linear(128.0 / 255.0);
        assert!((img.get(1, 1, false)[0] - want).abs() < 1e-4);
    }
}
