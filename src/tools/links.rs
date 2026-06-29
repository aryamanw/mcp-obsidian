use crate::vault::Vault;
use crate::parse::wikilink;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResolveLinksRequest {
    #[schemars(description = "Path to the note to resolve links from")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BacklinksRequest {
    #[schemars(description = "Path to the note to find backlinks for")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LinkGraphRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Depth of link neighbors to traverse (optional, defaults to 1)")]
    pub depth: Option<usize>,
}

#[derive(Clone)]
pub struct LinkTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl LinkTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Resolve all [[wikilinks]] in a note to their actual file paths.")]
    fn resolve_links(
        &self,
        Parameters(ResolveLinksRequest { path }): Parameters<ResolveLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let resolved: Vec<serde_json::Value> = note.links.iter().map(|link| {
                    let resolved_path = wikilink::resolve_wikilink(
                        link,
                        &self.vault.config.vault_path,
                    );
                    serde_json::json!({
                        "target": link,
                        "resolved": resolved_path.as_ref().map(|p| {
                            wikilink::relative_path(p, &self.vault.config.vault_path)
                        }),
                        "exists": resolved_path.is_some(),
                    })
                }).collect();
                let result = serde_json::json!({
                    "links": resolved,
                    "count": resolved.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find all notes that link to a given note (backlinks).")]
    fn backlinks(
        &self,
        Parameters(BacklinksRequest { path }): Parameters<BacklinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.backlinks(&path) {
            Ok(links) => {
                let result = serde_json::json!({
                    "backlinks": links,
                    "count": links.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get the link graph for a note — its outgoing links and their links, up to N hops.")]
    fn link_graph(
        &self,
        Parameters(LinkGraphRequest { path, depth }): Parameters<LinkGraphRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max_depth = depth.unwrap_or(1);
        let mut visited = std::collections::HashSet::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        fn explore(
            note_path: &str,
            depth: usize,
            max_depth: usize,
            vault: &Vault,
            visited: &mut std::collections::HashSet<String>,
            nodes: &mut Vec<serde_json::Value>,
            edges: &mut Vec<serde_json::Value>,
        ) {
            if depth > max_depth || visited.contains(note_path) { return; }
            visited.insert(note_path.to_string());

            if let Ok(note) = vault.read_note(note_path) {
                nodes.push(serde_json::json!({
                    "path": note.path,
                    "tags": note.tags,
                }));
                for link in &note.links {
                    edges.push(serde_json::json!({
                        "source": note_path,
                        "target": link,
                    }));
                    explore(link, depth + 1, max_depth, vault, visited, nodes, edges);
                }
            }
        }

        explore(&path, 0, max_depth, &self.vault, &mut visited, &mut nodes, &mut edges);

        let result = serde_json::json!({
            "nodes": nodes,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
    }
}
