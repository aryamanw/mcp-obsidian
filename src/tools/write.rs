use rmcp::schemars;
use serde::Deserialize;
use std::collections::HashMap;

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
