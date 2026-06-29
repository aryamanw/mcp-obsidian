use rmcp::schemars;
use serde::Deserialize;
use std::collections::HashMap;

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
