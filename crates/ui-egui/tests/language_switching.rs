//! Isolate process-global language switching from tests that construct the UI.
use materialcraft_ui_egui::i18n::{current, is_rtl, set_language, tr, trf};

#[test]
fn language_switching_and_arabic_translations_work() {
    set_language("en");
    assert_eq!(current().code, "en");
    assert_eq!(tr("Properties"), "Properties");
    assert!(!is_rtl());

    set_language("ar");
    assert_eq!(current().code, "ar");
    assert_eq!(tr("Properties"), "الخصائص");
    assert_eq!(tr("Preferences"), "التفضيلات");
    assert_eq!(tr("New graph"), "مخطط جديد");
    assert_eq!(tr("Export outputs"), "تصدير المخرجات");
    assert_eq!(tr("Welcome to MaterialCraft"), "مرحباً بك في MaterialCraft");
    assert!(is_rtl());

    // Live formatting
    let formatted = trf("Hello {name}", &[("name", &"World")]);
    assert_eq!(formatted, "Hello World");

    // Fallback for unknown text
    assert_eq!(tr("Unknown untranslated string 1234"), "Unknown untranslated string 1234");

    // Reset to English
    set_language("en");
}
