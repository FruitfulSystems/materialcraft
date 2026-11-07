use crate::GraphPort;
use serde::{Deserialize, Serialize};

/// An SBS graph reference and its last known interface, without a copy of its nodes
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphInstance {
    /// Empty for a graph in the same package
    pub package: String,
    pub graph: String,
    pub inputs: Vec<GraphPort>,
    pub outputs: Vec<GraphPort>,
}
