//! Function graphs: small typed expression graphs evaluated per pixel (pixel processor), once
//! per cook (value processor) or to compute a parameter dynamically. The data lives here; the
//! evaluator is `materialcraft_nodes::function`.

use serde::{Deserialize, Serialize};

use crate::Value;

/// A node in a function graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FnNode {
    pub id: u32,
    /// Operation id, e.g. `const_float`, `add`, `sample_gray`, `get_float2` (see the evaluator's
    /// catalog for the full list).
    pub op: String,
    /// Upstream node per input slot, in the op's slot order.
    #[serde(default)]
    pub inputs: Vec<Option<u32>>,
    /// The constant (`const_*`), the variable name (`get_*`) or the input index (`sample_*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default)]
    pub pos: [f32; 2],
}

/// A function graph: nodes plus the id of the node whose value is the result.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FnGraph {
    pub nodes: Vec<FnNode>,
    #[serde(default)]
    pub output: Option<u32>,
}

impl FnGraph {
    pub fn node(&self, id: u32) -> Option<&FnNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(1)
    }

    /// Append a node and return its id.
    pub fn add(&mut self, op: &str, inputs: &[u32], value: Option<Value>) -> u32 {
        let id = self.next_id();
        let x = self.nodes.len() as f32 * 140.0;
        self.nodes.push(FnNode { id, op: op.into(), inputs: inputs.iter().map(|i| Some(*i)).collect(), value, pos: [x, 0.0] });
        id
    }

    /// A graph returning one constant.
    pub fn constant(v: Value) -> FnGraph {
        let op = match &v {
            Value::Float(_) => "const_float",
            Value::Float2(_) => "const_float2",
            Value::Float3(_) => "const_float3",
            Value::Float4(_) | Value::Color(_) => "const_float4",
            Value::Int(_) | Value::Enum(_) => "const_int",
            Value::Bool(_) => "const_bool",
            _ => "const_float",
        };
        let mut g = FnGraph::default();
        let id = g.add(op, &[], Some(v));
        g.output = Some(id);
        g
    }
}
