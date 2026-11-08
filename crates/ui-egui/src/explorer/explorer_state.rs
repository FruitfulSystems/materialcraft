use super::ExplorerTarget;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExplorerState {
    pub selected: Option<ExplorerTarget>,
    pub rename: Option<(ExplorerTarget, String)>,
}
