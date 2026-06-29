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
pub struct CreateNoteRequest {
    #[schemars(description = "Path for the new note (relative to vault root)")]
    pub path: String,
    #[schemars(description = "Note content (markdown)")]
    pub content: Option<String>,
    #[schemars(description = "Initial frontmatter fields")]
    pub frontmatter: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateNoteRequest {
    #[schemars(description = "Path to the note to update")]
    pub path: String,
    #[schemars(description = "Content to write")]
    pub content: String,
    #[schemars(description = "'append' to add to end, 'replace' to overwrite body (preserves frontmatter)")]
    pub mode: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFrontmatterRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Frontmatter fields to set (merges with existing)")]
    pub fields: HashMap<String, String>,
}

#[derive(Clone)]
pub struct WriteTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl WriteTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Create a new note with optional content and frontmatter.")]
    fn create_note(
        &self,
        Parameters(CreateNoteRequest { path, content, frontmatter }): Parameters<CreateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = content.unwrap_or_default();
        match self.vault.create_note(&path, &body, frontmatter.as_ref()) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "message": "Note created successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Update an existing note. Use 'append' to add content or 'replace' to overwrite (preserves frontmatter).")]
    fn update_note(
        &self,
        Parameters(UpdateNoteRequest { path, content, mode }): Parameters<UpdateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.update_note(&path, &content, &mode) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "message": "Note updated successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Set frontmatter fields on a note. Merges with existing frontmatter.")]
    fn set_frontmatter(
        &self,
        Parameters(SetFrontmatterRequest { path, fields }): Parameters<SetFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.set_frontmatter(&path, &fields) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "message": "Frontmatter updated successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
