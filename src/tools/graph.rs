use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphPathRequest {
    #[schemars(description = "Source note path")]
    pub from: String,
    #[schemars(description = "Target note path")]
    pub to: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphStatsRequest {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphCommunitiesRequest {}
