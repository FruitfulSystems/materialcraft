use materialcraft_core::FunctionGraph;
use std::collections::BTreeMap;

pub fn warnings(graph: &FunctionGraph) -> Vec<String> {
    let Some(output) = super::output_node(graph) else { return vec!["No output node defined".into()] };

    if has_value_source(graph, output.id) { Vec::new() } else { vec!["Output node has no connected value source".into()] }
}

fn has_value_source(graph: &FunctionGraph, output: u32) -> bool {
    // Match the evaluator's node budget to bound work on malformed graphs
    if graph.nodes.len() > 4096 {
        return false;
    }

    let mut remaining: BTreeMap<_, _> = graph.nodes.iter().map(|node| (node.id, node)).collect();
    let mut pending = vec![output];
    while let Some(id) = pending.pop() {
        let Some(node) = remaining.remove(&id) else { continue };
        let Some(definition) = super::definition(&node.op) else { continue };

        if definition.inputs.is_empty() {
            return true;
        }

        pending.extend(node.inputs.iter().take(definition.inputs.len()).flatten().copied());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_core::Value;

    #[test]
    fn warnings_ignore_unused_nodes_and_allow_partial_outputs_with_a_value_source() {
        let mut graph = FunctionGraph::constant(Value::Float(0.5));
        let source = graph.output.unwrap();
        let multiply = graph.add("mul", &[], None);
        graph.nodes.last_mut().unwrap().inputs = vec![None];
        assert!(warnings(&graph).is_empty());
        graph.output = Some(multiply);
        graph.nodes.last_mut().unwrap().inputs = vec![None, Some(source)];
        let passthrough = graph.add("passthrough", &[multiply], None);
        graph.output = Some(graph.add("neg", &[passthrough], None));
        graph.add("add", &[], None);
        assert!(warnings(&graph).is_empty());
    }

    #[test]
    fn output_chains_without_a_value_source_warn_and_cycles_terminate() {
        let mut graph = FunctionGraph::default();
        let multiply = graph.add("mul", &[], None);
        let passthrough = graph.add("passthrough", &[multiply], None);
        graph.output = Some(graph.add("neg", &[passthrough], None));
        assert_eq!(warnings(&graph), ["Output node has no connected value source"]);
        graph.nodes[0].inputs = vec![Some(passthrough)];
        assert_eq!(warnings(&graph), ["Output node has no connected value source"]);
        let source = graph.add_constant(Value::Float(0.5));
        graph.nodes[0].inputs = vec![None, Some(source)];
        assert!(warnings(&graph).is_empty());
    }

    #[test]
    fn a_dot_output_is_missing_even_when_connected_to_a_value() {
        let mut graph = FunctionGraph::constant(Value::Float(0.5));
        let source = graph.output.unwrap();
        graph.output = Some(graph.add("passthrough", &[source], None));
        assert_eq!(warnings(&graph), ["No output node defined"]);
        assert!(super::super::compile(&graph, 0).is_err());
    }

    #[test]
    fn value_sources_include_variables_but_not_operator_settings_or_dangling_links() {
        let mut graph = FunctionGraph::default();
        let swizzle = graph.add("swizzle1", &[999], Some(Value::Int(0)));
        graph.output = Some(swizzle);
        assert_eq!(warnings(&graph), ["Output node has no connected value source"]);
        let source = graph.add("get_float1", &[], Some(Value::Text("value1".into())));
        graph.nodes[0].inputs = vec![Some(source)];
        assert!(warnings(&graph).is_empty());
    }

    #[test]
    fn empty_and_dangling_outputs_warn_without_preventing_editing() {
        let mut graph = FunctionGraph::default();
        assert_eq!(warnings(&graph), vec!["No output node defined"]);
        graph.output = Some(1);
        assert_eq!(warnings(&graph), vec!["No output node defined"]);
        graph.add_constant(Value::Float(0.5));
        assert!(warnings(&graph).is_empty());
    }
}
