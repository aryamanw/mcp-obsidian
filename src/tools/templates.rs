use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyTemplateRequest {
    #[schemars(description = "Template name (without .md extension)")]
    pub template: String,
    #[schemars(description = "Path to the note to apply the template to")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDailyNoteRequest {
    #[schemars(description = "Date in YYYY-MM-DD format (optional, defaults to today)")]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTemplatesRequest {}
