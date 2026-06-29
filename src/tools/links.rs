use rmcp::schemars;
use serde::Deserialize;

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
