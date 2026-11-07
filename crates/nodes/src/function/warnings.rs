use materialcraft_core::FunctionGraph;

pub fn warnings(graph: &FunctionGraph) -> Vec<String> {
    let mut messages = Vec::new();
    if graph.output.and_then(|output| graph.node(output)).is_none() {
        messages.push("No output node defined".into());
    }

    for node in &graph.nodes {
        let Some(definition) = super::definition(&node.op) else { continue };

        for (index, input) in definition.inputs.iter().enumerate() {
            if node.inputs.get(index).copied().flatten().is_none() {
                messages.push(format!("{} (node {}): input {input} is not connected", definition.label, node.id));
            }
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_core::Value;

    #[test]
    fn warnings_include_disconnected_nodes_and_absent_input_slots() {
        let mut graph = FunctionGraph::constant(Value::Float(0.5));
        let multiply = graph.add("mul", &[], None);
        graph.nodes.last_mut().unwrap().inputs = vec![None];
        assert_eq!(
            warnings(&graph),
            vec![format!("Multiply (node {multiply}): input a is not connected"), format!("Multiply (node {multiply}): input b is not connected")]
        );
        graph.nodes.last_mut().unwrap().inputs = vec![graph.output, graph.output];
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
