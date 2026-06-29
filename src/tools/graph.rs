use crate::graph::GraphAnalyzer;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphPathRequest {
    #[schemars(description = "Source note path")]
    pub from: String,
    #[schemars(description = "Target note path")]
    pub to: String,
}

#[derive(Clone)]
pub struct GraphTools {
    pub analyzer: Arc<GraphAnalyzer>,
}

#[tool_router]
impl GraphTools {
    pub fn new(analyzer: Arc<GraphAnalyzer>) -> Self {
        Self { analyzer }
    }

    #[tool(description = "Get vault graph statistics: node count, edge count, density.")]
    fn graph_stats(
        &self,
        Parameters(()): Parameters<()>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.stats() {
            Ok(stats) => {
                let result = serde_json::json!({
                    "node_count": stats.node_count,
                    "edge_count": stats.edge_count,
                    "density": stats.density,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get communities (connected components) from Obsidian's note graph.")]
    fn graph_communities(
        &self,
        Parameters(()): Parameters<()>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.communities() {
            Ok(communities) => {
                let result: Vec<serde_json::Value> = communities.iter().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "node_count": c.nodes.len(),
                        "nodes": c.nodes,
                    })
                }).collect();
                let output = serde_json::json!({
                    "communities": result,
                    "count": result.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(output.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find the shortest path between two notes in the vault graph.")]
    fn graph_path(
        &self,
        Parameters(GraphPathRequest { from, to }): Parameters<GraphPathRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.shortest_path(&from, &to) {
            Ok(path) => {
                let result = serde_json::json!({
                    "path": path,
                    "hops": path.len().saturating_sub(1),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
