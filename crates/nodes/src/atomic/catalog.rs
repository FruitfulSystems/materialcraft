use super::*;
use crate::NodeDefinition;

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        bitmap::definition(),
        blend::definition(),
        blur::definition(),
        channels_shuffle::definition(),
        curve::definition(),
        directional_blur::definition(),
        directional_warp::definition(),
        distance::definition(),
        emboss::definition(),
        pattern_map::definition(),
        gradient_dynamic::definition(),
        gradient_map::definition(),
        grayscale_conversion::definition(),
        hsl::definition(),
        levels::definition(),
        normal::definition(),
        pixel_processor::definition(),
        svg::definition(),
        sharpen::definition(),
        text::definition(),
        transformation::definition(),
        uniform::definition(),
        value_processor::definition(),
        warp::definition(),
        input_color::definition(),
        input_grayscale::definition(),
        input_value::definition(),
        output::definition(),
        dot::definition(),
    ]
}
