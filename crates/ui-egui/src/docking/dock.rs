use craft_ui::docking::{Action, PanelState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Panel, Workspace, default_workspace, permissions, validate};

/// Saved workspace and transient panel presentation
#[derive(Clone, Debug)]
pub struct Dock {
    pub(super) workspace: Workspace,
    pub(super) raise: Option<Panel>,
    maximized: Option<Panel>,
}

impl Default for Dock {
    fn default() -> Self {
        Self { workspace: default_workspace(), raise: None, maximized: None }
    }
}

impl PartialEq for Dock {
    fn eq(&self, other: &Self) -> bool {
        self.workspace == other.workspace
    }
}

impl Serialize for Dock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.workspace.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Dock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_value(Value::deserialize(deserializer)?))
    }
}

impl Dock {
    /// Invalid layouts fall back to the default workspace
    pub fn from_value(value: Value) -> Self {
        let default = default_workspace();
        let loaded = serde_json::from_value::<Workspace>(value).ok();
        let Some(mut workspace) = loaded.filter(|workspace| validate(workspace).is_ok()) else {
            return Self::default();
        };

        workspace.reconcile(|_| true, permissions, &default);
        if validate(&workspace).is_err() {
            return Self::default();
        }
        Self { workspace, raise: None, maximized: None }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn maximized(&self) -> Option<Panel> {
        self.maximized
    }

    /// Maximization never alters the saved layout or opens hidden panels
    pub fn maximize(&mut self, panel: Panel) -> Result<(), String> {
        if self.workspace.layout.panel_state(&panel) != PanelState::Front {
            return Err(format!("{} must be an open, active panel before maximizing", panel.name()));
        }
        self.maximized = Some(panel);
        Ok(())
    }

    pub fn restore(&mut self) {
        self.maximized = None;
    }

    pub fn is_open(&self, panel: Panel) -> bool {
        self.workspace.layout.contains(&panel)
    }

    pub fn is_visible(&self, panel: Panel) -> bool {
        self.maximized.map_or_else(|| self.workspace.layout.panel_state(&panel) == PanelState::Front, |maximized| maximized == panel)
    }

    /// Hide a visible panel or bring it into view
    pub fn toggle(&mut self, panel: Panel) {
        if self.is_visible(panel) { self.close(panel) } else { self.focus(panel) }
    }

    pub fn close(&mut self, panel: Panel) {
        match self.workspace.set_visible(panel, false, None, permissions) {
            Ok(()) if self.maximized == Some(panel) => self.restore(),
            Ok(()) => {}
            Err(error) => log::warn!("panel {panel:?} was not closed: {error}"),
        }
    }

    /// Reveal the requested panel in its existing location
    pub fn focus(&mut self, panel: Panel) {
        let mut next = self.workspace.clone();
        match next.set_visible(panel, true, None, permissions) {
            Ok(()) if validate(&next).is_ok() => {
                self.workspace = next;
                self.raise = Some(panel);
                if self.maximized != Some(panel) {
                    self.restore();
                }
            }
            Ok(()) => log::warn!("panel {panel:?} was not opened: it would break the graph's rules"),
            Err(error) => log::warn!("panel {panel:?} was not opened: {error}"),
        }
    }

    /// Rejected gestures leave the layout intact
    pub(super) fn apply(&mut self, action: Action<Panel>) {
        let mut next = self.workspace.clone();
        match next.apply(action, permissions) {
            Ok(_) if validate(&next).is_ok() => self.workspace = next,
            Ok(_) => log::warn!("a panel move would break the graph's rules"),
            Err(error) => log::warn!("a panel move was not applied: {error}"),
        }
    }
}
