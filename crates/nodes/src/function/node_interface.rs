use materialcraft_core::{FunctionNode, Graph, GraphPort};

pub fn input_identifiers(node: &FunctionNode) -> impl Iterator<Item = &str> {
    node.instance
        .iter()
        .flat_map(|instance| instance.inputs.iter().map(|port| port.identifier.as_str()))
        .chain(super::definition(&node.op).filter(|_| node.instance.is_none()).into_iter().flat_map(|definition| definition.inputs.iter().copied()))
}

pub fn node_label(node: &FunctionNode) -> &str {
    node.instance
        .as_ref()
        .map(|instance| instance.graph.as_str())
        .unwrap_or_else(|| super::definition(&node.op).map_or("Unknown function", |definition| definition.label))
}

pub fn graph_ports(graph: &Graph) -> Result<(Vec<GraphPort>, Vec<GraphPort>), String> {
    let function = graph.function.as_ref().ok_or("the source must be a Function Graph")?;
    let output = super::output_node(function).ok_or("the source graph needs an output before it can be instanced")?;
    let types = super::node_types(function)?;
    if graph.exposed.len() > 4096 {
        return Err("Function instance exceeds the 4096-input limit".into());
    }
    let inputs: Vec<_> = graph
        .exposed
        .iter()
        .map(|parameter| GraphPort { identifier: parameter.id.clone(), type_code: super::value_type::type_code(&parameter.default) })
        .collect();
    if inputs.iter().any(|port| port.type_code == 0) {
        return Err("Function inputs must be scalar or vector values".into());
    }
    Ok((inputs, vec![GraphPort { identifier: "output".into(), type_code: types.get(&output.id).copied().unwrap_or(256) }]))
}
