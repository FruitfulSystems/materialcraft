mod preference_tab;
pub use preference_tab::PreferenceTab;

use egui::{Ui, vec2};

use crate::{
    MaterialCraftApp,
    i18n::tr,
    state::Prefs,
    theme::{Accent, Brightness},
};

pub(crate) fn show(app: &mut MaterialCraftApp, context: &egui::Context) {
    let mut open = true;
    crate::windows::modal(context, "prefs", tr("Preferences"), &mut open, vec2(560.0, 360.0), |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(110.0);
                for tab in PreferenceTab::ALL {
                    let response = ui.selectable_value(&mut app.ui.preferences_tab, tab, tr(tab.label()));
                    crate::widgets::name(&response, egui::Role::Tab, tab.label());
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(410.0);
                match app.ui.preferences_tab {
                    PreferenceTab::General => general(&mut app.ui.prefs, ui),
                    PreferenceTab::Graph => graph(&mut app.ui.prefs, ui),
                    PreferenceTab::View3d => view3d(&mut app.ui.prefs, ui),
                }
            });
        });
    });
    if !open {
        app.ui.windows.preferences = false;
    }
}

fn general(preferences: &mut Prefs, ui: &mut Ui) {
    let current_language = crate::i18n::current();
    crate::widgets::combo(ui, tr("Language"), current_language.name, |ui| {
        for language in &crate::i18n::LANGUAGES {
            if ui.selectable_label(preferences.language == language.code, language.name).clicked() {
                preferences.language = language.code.to_owned();
                crate::i18n::set_language(&preferences.language);
            }
        }
    });
    crate::widgets::combo(ui, tr("Brightness"), tr(preferences.brightness.label()), |ui| {
        for brightness in Brightness::ALL {
            ui.selectable_value(&mut preferences.brightness, brightness, tr(brightness.label()));
        }
    });
    crate::widgets::combo(ui, tr("Accent color"), tr(preferences.accent.label()), |ui| {
        for accent in Accent::ALL {
            ui.selectable_value(&mut preferences.accent, accent, tr(accent.label()));
        }
    });
    crate::widgets::slider(ui, tr("Interface scale"), &mut preferences.ui_scale, 0.75..=2.0, 2);
    let mut autosave_minutes = preferences.autosave_minutes as f32;
    if crate::widgets::slider(ui, tr("Autosave every (minutes, 0 = off)"), &mut autosave_minutes, 0.0..=60.0, 0).changed() {
        preferences.autosave_minutes = autosave_minutes.round() as u32;
    }
    ui.checkbox(&mut preferences.show_home, tr("Show at startup"));
}

fn graph(preferences: &mut Prefs, ui: &mut Ui) {
    crate::widgets::combo(ui, tr("Preview size while editing"), crate::preview_resolution::label(preferences.preview_size), |ui| {
        crate::preview_resolution::choices(ui, &mut preferences.preview_size);
    });
    ui.checkbox(&mut preferences.thumbnails, tr("Show node thumbnails"));
    ui.checkbox(&mut preferences.snap_to_grid, tr("Snap to grid"));
}

fn view3d(preferences: &mut Prefs, ui: &mut Ui) {
    crate::widgets::slider(ui, tr("3D pan limit (mesh radii)"), &mut preferences.camera.pan_limit, 0.0..=20.0, 1);
    crate::widgets::slider(ui, tr("3D pan speed"), &mut preferences.camera.pan_speed, 0.1..=4.0, 2);
}
