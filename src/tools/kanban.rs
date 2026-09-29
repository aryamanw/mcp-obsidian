use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadKanbanRequest {
    #[schemars(description = "Path to the Kanban board note")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct KanbanLaneSpec {
    #[schemars(description = "Lane title")]
    pub title: String,
    #[schemars(description = "Mark this as a 'complete' lane: its cards are checked, and cards moved into it get checked (optional, defaults to false)")]
    pub complete: Option<bool>,
    #[schemars(description = "Initial card texts (optional)")]
    pub cards: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateKanbanRequest {
    #[schemars(description = "Path for the new board note ('.md' is appended if missing)")]
    pub path: String,
    #[schemars(description = "Lanes, left to right")]
    pub lanes: Vec<KanbanLaneSpec>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddKanbanCardRequest {
    #[schemars(description = "Path to the Kanban board note")]
    pub path: String,
    #[schemars(description = "Title of the lane to add the card to (case-insensitive)")]
    pub lane: String,
    #[schemars(description = "Card text (markdown; may include [[links]], #tags, and @{YYYY-MM-DD} dates)")]
    pub text: String,
    #[schemars(description = "Whether the card is checked (optional; defaults to the lane's complete setting)")]
    pub checked: Option<bool>,
    #[schemars(description = "'top' or 'bottom' of the lane (optional, defaults to 'bottom')")]
    pub position: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateKanbanCardRequest {
    #[schemars(description = "Path to the Kanban board note")]
    pub path: String,
    #[schemars(description = "Card to update: its exact text, or a unique case-insensitive substring of it")]
    pub card: String,
    #[schemars(description = "Only look for the card in this lane (optional)")]
    pub lane: Option<String>,
    #[schemars(description = "Move the card to the bottom of this lane (optional)")]
    pub to_lane: Option<String>,
    #[schemars(description = "Replace the card's text (optional)")]
    pub new_text: Option<String>,
    #[schemars(description = "Check or uncheck the card (optional)")]
    pub checked: Option<bool>,
    #[schemars(description = "Move the card to the board's archive (optional)")]
    pub archive: Option<bool>,
}
