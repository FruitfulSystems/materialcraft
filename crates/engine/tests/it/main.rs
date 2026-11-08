//! One integration-test executable avoids linking the same dependencies for every feature.
//! Keep feature tests in their own modules; the standalone session.rs target remains available.

mod annotations;
mod atomics;
mod delete_relink;
mod embedded_edit;
mod export_names;
mod exposed_inputs;
mod function_graph;
mod function_instances;
mod function_math;
mod function_output;
mod graph_alignment;
mod graph_edit;
mod graph_interaction;
mod linked_instances;
mod material_preview;
mod normal_maps;
mod open_documents;
mod optimization_perf;
mod pattern_graph;
mod perf;
mod reset_drivers;
mod sbs_functions;
mod subgraph_seed;
mod view_settings;
mod wire_swap_types;
