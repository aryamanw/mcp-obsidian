use crate::vault::Vault;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchNotesRequest {
    #[schemars(description = "Search query (full-text)")]
    pub query: String,
    #[schemars(description = "Max results to return (optional, defaults to 20)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchByTagRequest {
    #[schemars(description = "Tags to search for (e.g. ['project', 'todo'])")]
    pub tags: Vec<String>,
    #[schemars(description = "Match mode: 'any' (OR) or 'all' (AND). Defaults to 'any'.")]
    pub match_mode: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchByFrontmatterRequest {
    #[schemars(description = "Frontmatter filters as key-value pairs (e.g. {\"status\": \"done\"})")]
    pub filters: HashMap<String, String>,
}

#[derive(Clone)]
pub struct SearchTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl SearchTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Full-text search across the vault. Returns matching notes with snippets.")]
    fn search_notes(
        &self,
        Parameters(SearchNotesRequest { query, limit }): Parameters<SearchNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max = limit.unwrap_or(20);
        match self.vault.search_notes(&query, max) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "tags": n.tags,
                        "body_preview": n.body.chars().take(200).collect::<String>(),
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find notes with specific tags.")]
    fn search_by_tag(
        &self,
        Parameters(SearchByTagRequest { tags, match_mode }): Parameters<SearchByTagRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mode = match_mode.unwrap_or_else(|| "any".to_string());
        match self.vault.search_by_tag(&tags, &mode) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "tags": n.tags,
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Filter notes by frontmatter fields.")]
    fn search_by_frontmatter(
        &self,
        Parameters(SearchByFrontmatterRequest { filters }): Parameters<SearchByFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.search_by_frontmatter(&filters) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "frontmatter": n.frontmatter,
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
