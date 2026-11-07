//! Native function primitives, compilation and bounded evaluation

mod arithmetic;
mod catalog;
mod compile;
mod evaluation_context;
mod evaluation_state;
mod evaluation_step;
mod function_call;
mod function_definition;
mod inferred_type;
mod instruction;
mod node_interface;
mod node_types;
mod operation;
mod output;
mod processor;
mod program;
mod resolve_instances;
pub mod value_type;
mod vector;
mod warnings;

pub use catalog::{FUNCTIONS, default_value, definition};
pub use compile::compile;
pub use evaluation_context::EvaluationContext;
pub use evaluation_state::EvaluationState;
pub use function_definition::FunctionDefinition;
pub use node_interface::{graph_ports, input_identifiers, node_label};
pub use node_types::node_types;
pub use output::{can_be_output, output_node};
pub use processor::default_pixel_function;
pub(crate) use processor::pixel_processor;
pub use program::Program;
pub use resolve_instances::resolve_instances;
pub use warnings::warnings;

#[cfg(test)]
mod tests;
