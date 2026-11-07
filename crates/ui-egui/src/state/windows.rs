use serde::{Deserialize, Serialize};

/// Which dialogs are open
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Windows {
    pub new_graph: bool,
    pub export: bool,
    pub about: bool,
    pub preferences: bool,
    pub shortcuts: bool,
    pub home: bool,
    pub plugin_manager: bool,
    pub switch_engine: bool,
    /// The Dependency Manager window (Windows menu)
    pub dependencies: bool,
    #[serde(skip_deserializing)]
    pub unsaved: Option<crate::unsaved::Guarded>,
}
