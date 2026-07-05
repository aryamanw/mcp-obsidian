use rmcp::schemars;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameNoteRequest {
    #[schemars(description = "Current path of the note")]
    pub source: String,
    #[schemars(description = "New path for the note")]
    pub dest: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MergeNotesRequest {
    #[schemars(description = "Source note path (will be deleted after merge)")]
    pub source: String,
    #[schemars(description = "Destination note path (content appended here)")]
    pub dest: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkTagRequest {
    #[schemars(description = "Full-text search query to find notes")]
    pub query: String,
    #[schemars(description = "Tags to add")]
    pub add_tags: Option<Vec<String>>,
    #[schemars(description = "Tags to remove")]
    pub remove_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LinkRelatedNotesRequest {
    #[schemars(description = "Path to the note to link from")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateFolderRequest {
    #[schemars(description = "Path for the new folder (relative to vault root)")]
    pub path: String,
}

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
