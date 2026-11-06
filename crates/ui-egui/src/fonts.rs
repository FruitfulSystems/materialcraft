//! Arabic fallback fonts for the UI. egui's default fonts have no Arabic glyphs, so translated
//! UIs need fallback fonts from the operating system.
//!
//! Windows: Segoe UI, Tahoma, Arial
//! macOS: SF Arabic, Geeza Pro, Arial Unicode
//! Linux: Noto Sans Arabic, DejaVu Sans

use std::sync::Arc;

use egui::{FontData, FontFamily};

/// Candidate system font files: (path, face index in a collection).
#[cfg(not(target_arch = "wasm32"))]
fn system_files() -> Vec<(std::path::PathBuf, u32)> {
    use std::path::PathBuf;
    let mut v: Vec<(PathBuf, u32)> = Vec::new();
    if cfg!(windows) {
        let fonts = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        let files: &[&str] = &["segoeui.ttf", "tahoma.ttf", "arial.ttf"];
        v.extend(files.iter().map(|f| (fonts.join(f), 0)));
    } else if cfg!(target_os = "macos") {
        let files: &[&str] = &["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/Supplemental/GeezaPro.ttc"];
        v.extend(files.iter().map(|f| (PathBuf::from(f), 0)));
        v.push((PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"), 0));
    } else {
        for f in [
            "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansArabic-Regular.ttf",
            "/usr/local/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/local/share/fonts/dejavu/DejaVuSans.ttf",
        ] {
            v.push((PathBuf::from(f), 0));
        }
    }
    v
}

/// Fallback fonts manager for the egui context.
#[derive(Default)]
pub struct UiFonts {
    installed: bool,
    data: Option<Arc<FontData>>,
}

impl UiFonts {
    /// Update fallback fonts according to active language and whether the preferences dialog is open.
    pub fn update(&mut self, ctx: &egui::Context, language: &str, preferences_open: bool) {
        let want_arabic = language == "ar" || preferences_open;
        if !want_arabic || self.installed {
            return;
        }
        let Some(data) = self.load_arabic() else { return };
        let mut defs = crate::theme::font_definitions();
        let name = "script-Arab".to_string();
        defs.font_data.insert(name.clone(), data);
        defs.families.entry(FontFamily::Proportional).or_default().push(name.clone());
        defs.families.entry(FontFamily::Monospace).or_default().push(name);
        ctx.set_fonts(defs);
        self.installed = true;
    }

    /// Whether an Arabic fallback font has been found and loaded.
    pub fn covers_arabic(&self) -> bool {
        self.data.is_some()
    }

    fn load_arabic(&mut self) -> Option<Arc<FontData>> {
        if let Some(d) = &self.data {
            return Some(d.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        for (path, index) in system_files() {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() || meta.len() == 0 || meta.len() > (128 << 20) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                let arc = Arc::new(data);
                self.data = Some(arc.clone());
                log::info!("UI font fallback for Arabic: {}", path.display());
                return Some(arc);
            }
        }
        log::warn!("no font found for Arabic text; system fonts may show boxes");
        None
    }
}
