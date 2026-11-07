use crate::FunctionGraphTarget;
use materialcraft_core::{FunctionGraph, Graph, Value};
use serde_json::{Value as Json, json};

pub(crate) fn warnings(graph: &Graph) -> Vec<Json> {
    let mut entries = Vec::new();
    let mut inspect = |target: FunctionGraphTarget, function: &FunctionGraph| {
        let messages = materialcraft_nodes::function::warnings(function);
        if !messages.is_empty() {
            entries.push(json!({"target": target, "messages": messages}));
        }
    };
    if let Some(function) = &graph.function {
        inspect(FunctionGraphTarget::Graph, function);
    }

    for node in &graph.nodes {
        for (parameter, function) in &node.functions {
            inspect(FunctionGraphTarget::Dynamic { node: node.id, parameter: parameter.clone() }, function);
        }
        let defaults = materialcraft_nodes::find(&node.def)
            .into_iter()
            .flat_map(|definition| &definition.params)
            .filter(|parameter| !node.params.contains_key(parameter.id))
            .map(|parameter| (parameter.id, &parameter.default));
        for (parameter, value) in node.params.iter().map(|(parameter, value)| (parameter.as_str(), value)).chain(defaults) {
            match value {
                Value::Function(function) => inspect(FunctionGraphTarget::Parameter { node: node.id, parameter: parameter.into() }, function),
                Value::PatternGraph(pattern) => {
                    for child in &pattern.nodes {
                        for (parameter, value) in &child.parameters {
                            if let Value::Function(function) = value {
                                inspect(FunctionGraphTarget::Pattern { owner: node.id, node: child.id, parameter: parameter.clone() }, function);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    entries
}
