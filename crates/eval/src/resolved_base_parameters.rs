use materialcraft_core::{BaseParameters, Graph};

#[derive(Clone, Copy)]
pub(crate) struct ResolvedBaseParameters {
    pub tiling: [bool; 2],
    pub pixel_ratio: i32,
}

impl ResolvedBaseParameters {
    pub fn graph(graph: &Graph, options: &crate::Options<'_>) -> Self {
        Self {
            tiling: graph.tiling_mode.resolve(graph.tiling, options.parent_tiling, options.parent_tiling.unwrap_or(graph.tiling)),
            pixel_ratio: graph.pixel_ratio_mode.resolve(graph.pixel_ratio, options.parent_pixel_ratio, options.parent_pixel_ratio.unwrap_or(graph.pixel_ratio)),
        }
    }

    pub fn resolve(base: &BaseParameters, input: Option<Self>, parent: Self) -> Self {
        Self {
            tiling: base.tiling_mode.resolve(base.tiling, input.map(|input| input.tiling), parent.tiling),
            pixel_ratio: base.pixel_ratio_mode.resolve(base.pixel_ratio, input.map(|input| input.pixel_ratio), parent.pixel_ratio),
        }
    }
}
