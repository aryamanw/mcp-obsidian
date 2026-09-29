use crate::parse::canvas::{EdgeSpec, NodeSpec};
use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadCanvasRequest {
    #[schemars(description = "Path to the .canvas file (extension optional)")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateCanvasRequest {
    #[schemars(description = "Path for the new canvas ('.canvas' is appended if missing)")]
    pub path: String,
    #[schemars(description = "Nodes to create (optional)")]
    pub nodes: Option<Vec<NodeSpec>>,
    #[schemars(description = "Edges between nodes (optional); reference node ids given in 'nodes'")]
    pub edges: Option<Vec<EdgeSpec>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditCanvasRequest {
    #[schemars(description = "Path to the .canvas file (extension optional)")]
    pub path: String,
    #[schemars(description = "Nodes to add, or to update when the id matches an existing node (optional)")]
    pub nodes: Option<Vec<NodeSpec>>,
    #[schemars(description = "Edges to add, or to update when the id matches an existing edge (optional)")]
    pub edges: Option<Vec<EdgeSpec>>,
    #[schemars(description = "Ids of nodes/edges to remove; removing a node also removes its edges. Applied before adds/updates (optional).")]
    pub remove_ids: Option<Vec<String>>,
}
