#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod material;
pub mod mesh;
pub mod project;
pub mod sbs;
mod svg;

mod dds_image;
mod depth;
mod file;
mod format_error;
mod guard;
mod image;

pub use depth::Depth;
#[cfg(not(target_arch = "wasm32"))]
pub use file::write_atomic;
pub use file::{output_name, safe_file_name};
pub use format_error::FormatError;
pub(crate) use guard::guard;
pub use image::{decode, encode};

pub type Result<T> = std::result::Result<T, FormatError>;
pub(crate) const MAX_JSON: u64 = 64 << 20;
pub(crate) const MAX_RESOURCE: u64 = 256 << 20;
pub const EXPORT_FORMATS: &[(&str, &str)] =
    &[("png", "PNG"), ("jpg", "JPEG"), ("tga", "TGA"), ("tif", "TIFF"), ("bmp", "BMP"), ("webp", "WEBP"), ("exr", "OpenEXR"), ("hdr", "Radiance HDR")];
pub const VECTOR_EXTENSIONS: &[&str] = &["svg"];
pub const BITMAP_EXTENSIONS: &[&str] = &["dds", "webp", "gif", "hdr", "exr", "jpg", "jif", "jpeg", "jpe", "pgm", "png", "tif", "tiff", "tga", "targa", "bmp"];
pub const BITMAP_FORMATS: &[(&str, &[&str])] = &[
    ("DDS", &["dds"]),
    ("WebP", &["webp"]),
    ("GIF", &["gif"]),
    ("Radiance HDR", &["hdr"]),
    ("OpenEXR", &["exr"]),
    ("JPEG", &["jpg", "jif", "jpeg", "jpe"]),
    ("PGM (ASCII or binary)", &["pgm"]),
    ("PNG", &["png"]),
    ("TIFF", &["tif", "tiff"]),
    ("Targa", &["tga", "targa"]),
    ("Bitmap", &["bmp"]),
];
pub const IMAGE_EXTENSIONS: &[&str] =
    &["svg", "dds", "webp", "gif", "hdr", "exr", "jpg", "jif", "jpeg", "jpe", "pgm", "png", "tif", "tiff", "tga", "targa", "bmp"];

#[cfg(test)]
mod tests;
