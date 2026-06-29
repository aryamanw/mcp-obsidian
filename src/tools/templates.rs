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

#[derive(Clone)]
pub struct TemplateTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl TemplateTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "List available templates in the vault's templates folder.")]
    fn list_templates(
        &self,
        Parameters(()): Parameters<()>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_templates() {
            Ok(templates) => {
                let result = serde_json::json!({
                    "templates": templates,
                    "count": templates.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Apply a template to a note. Merges template frontmatter with existing.")]
    fn apply_template(
        &self,
        Parameters(ApplyTemplateRequest { template, path }): Parameters<ApplyTemplateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.apply_template(&template, &path) {
            Ok(()) => {
                let result = serde_json::json!({
                    "message": format!("Template '{}' applied to '{}'", template, path),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get or create today's daily note. Returns existing note or creates a new one.")]
    fn get_daily_note(
        &self,
        Parameters(GetDailyNoteRequest { date }): Parameters<GetDailyNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.get_daily_note(date.as_deref()) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "body": note.body,
                    "tags": note.tags,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
