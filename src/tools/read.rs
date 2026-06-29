use crate::vault::Vault;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadNoteRequest {
    #[schemars(description = "Path to the note relative to vault root (e.g. 'folder/note.md')")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListVaultRequest {
    #[schemars(description = "Subpath to list from (optional, defaults to vault root)")]
    pub path: Option<String>,
    #[schemars(description = "Max depth to traverse (optional, defaults to 10)")]
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetMetadataRequest {
    #[schemars(description = "Path to the note relative to vault root")]
    pub path: String,
}

#[derive(Clone)]
pub struct ReadTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl ReadTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Read a note by path. Returns markdown content, parsed frontmatter, and resolved links.")]
    fn read_note(
        &self,
        Parameters(ReadNoteRequest { path }): Parameters<ReadNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "body": note.body,
                    "tags": note.tags,
                    "links": note.links,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "List vault structure — folders and notes.")]
    fn list_vault(
        &self,
        Parameters(ListVaultRequest { path, depth }): Parameters<ListVaultRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_vault(path.as_deref(), depth) {
            Ok(entries) => {
                let result = serde_json::json!({
                    "entries": entries,
                    "count": entries.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get metadata for a note: frontmatter fields, tags, outgoing links, and backlink count.")]
    fn get_metadata(
        &self,
        Parameters(GetMetadataRequest { path }): Parameters<GetMetadataRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let backlinks = self.vault.backlinks(&path).unwrap_or_default();
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "tags": note.tags,
                    "outgoing_links": note.links,
                    "backlink_count": backlinks.len(),
                    "backlinks": backlinks,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
