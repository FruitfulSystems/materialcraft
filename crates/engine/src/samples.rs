use crate::{EngineError, Result};
use materialcraft_core::{Graph, Value};

pub const TEMPLATES: &[(&str, &str)] = &[("empty", "Empty"), ("pbr", "Material outputs"), ("atomics", "Atomic sample")];

pub fn template(id: &str, name: &str, size_log2: u32) -> Result<Graph> {
    let mut graph = match id {
        "empty" | "pbr" => Graph::new(name),
        "atomics" => crate::atomic_sample()?.graphs.into_iter().next().ok_or_else(|| EngineError::Other("atomic sample has no graph".into()))?,
        _ => return Err(EngineError::Other(format!("unknown template `{id}`"))),
    };

    graph.name = name.to_string();
    graph.size = [size_log2, size_log2];
    if id == "pbr" {
        // Uniform colors into every output; the normal is derived from the height, so a height
        // wired in shows up in the 3D view right away (as a normal map, and as relief).
        let mut height_source = None;
        for (index, (identifier, color)) in [
            ("basecolor", [0.5, 0.5, 0.5, 1.0]),
            ("normal", [0.5, 0.5, 1.0, 1.0]),
            ("roughness", [0.5, 0.5, 0.5, 1.0]),
            ("metallic", [0.0, 0.0, 0.0, 1.0]),
            ("height", [0.0, 0.0, 0.0, 1.0]),
            ("ambientocclusion", [1.0; 4]),
        ]
        .into_iter()
        .enumerate()
        {
            let y = index as f32 * 180.0;
            let output = graph.add_node("output", [440.0, y]);
            let node = graph.node_mut(output)?;
            node.params.insert("identifier".into(), Value::Text(identifier.into()));
            node.params.insert("usage".into(), Value::Enum(index as u32));
            if identifier == "normal" {
                // Filled in below, from the height
                continue;
            }
            let source = graph.add_node("uniform", [0.0, y]);
            let node = graph.node_mut(source)?;
            node.params.insert("color_mode".into(), Value::Bool(index == 0));
            node.params.insert("color".into(), Value::Color(color));
            graph.connect(source, "output", output, "input")?;
            if identifier == "height" {
                height_source = Some(source);
            }
        }
        let normal = graph.add_node("normal", [220.0, 180.0]);
        graph.node_mut(normal)?.params.insert("intensity".into(), Value::Float(2.0));
        if let Some(height) = height_source {
            graph.connect(height, "output", normal, "input")?;
        }
        let normal_output = graph.nodes.iter().find(|n| n.def == "output" && n.params.get("identifier") == Some(&Value::Text("normal".into()))).map(|n| n.id);
        if let Some(output) = normal_output {
            graph.connect(normal, "output", output, "input")?;
        }
    }
    Ok(graph)
}
