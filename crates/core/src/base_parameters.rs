use serde::{Deserialize, Serialize};

use crate::{OutputFormat, ParameterInheritance, SizeMode};

/// Parameters every node has ("Base parameters")
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BaseParameters {
    pub size_mode: SizeMode,
    /// log2 offset per axis (relative modes) or log2 size (absolute)
    pub size: [i32; 2],
    pub format: OutputFormat,
    /// Added to the graph's random seed for this node
    pub seed: i32,
    /// Whether sampling wraps at the edges (horizontal, vertical)
    pub tiling: [bool; 2],
    pub tiling_mode: ParameterInheritance,
    /// SBS pixel ratio: zero stretches the image; one keeps pixels square.
    pub pixel_ratio: i32,
    pub pixel_ratio_mode: ParameterInheritance,
}

impl Default for BaseParameters {
    fn default() -> Self {
        BaseParameters {
            size_mode: SizeMode::RelativeToInput,
            size: [0, 0],
            format: OutputFormat::Inherit,
            seed: 0,
            tiling: [true, true],
            tiling_mode: ParameterInheritance::default(),
            pixel_ratio: 0,
            pixel_ratio_mode: ParameterInheritance::default(),
        }
    }
}
