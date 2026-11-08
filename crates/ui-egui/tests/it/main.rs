//! One integration-test executable avoids linking the same dependencies for every feature.
//! Keep feature tests in their own modules; the standalone snapshot.rs target remains available.

mod dialogs;
mod interface_scale;
mod language_switching;
mod normal_maps;
mod numeric_input;
mod preview_resolution;
mod resize;
mod resolution;
mod search;
mod slider_round_values;
mod unsaved_changes;
