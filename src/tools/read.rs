use rmcp::schemars;
use serde::Deserialize;

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
