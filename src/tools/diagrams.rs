use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCodeBlocksRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Only return blocks of this language, e.g. 'mermaid' or 'chart' (optional)")]
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteMermaidRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Mermaid source without the ``` fence, e.g. 'flowchart LR\\n  A --> B'")]
    pub diagram: String,
    #[schemars(description = "Index of an existing mermaid block to replace (from list_code_blocks with language 'mermaid'). Omit to insert a new one.")]
    pub index: Option<usize>,
    #[schemars(description = "When inserting, heading to put the diagram under (e.g. '## Architecture'); created if missing. Omit to append at the end.")]
    pub heading: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChartSeriesSpec {
    #[schemars(description = "Series name shown in the legend (optional)")]
    pub title: Option<String>,
    #[schemars(description = "One value per label")]
    pub data: Vec<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteChartRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[serde(rename = "type")]
    #[schemars(description = "Chart type: 'bar', 'line', 'pie', 'doughnut', 'radar', or 'polarArea'")]
    pub chart_type: String,
    #[schemars(description = "X-axis / category labels")]
    pub labels: Vec<String>,
    #[schemars(description = "Data series; each must have one value per label")]
    pub series: Vec<ChartSeriesSpec>,
    #[schemars(description = "Chart width, e.g. '80%' (optional)")]
    pub width: Option<String>,
    #[schemars(description = "Start the y-axis at zero (optional)")]
    pub begin_at_zero: Option<bool>,
    #[schemars(description = "Stack series (bar/line charts, optional)")]
    pub stacked: Option<bool>,
    #[schemars(description = "Fill the area under lines (line charts, optional)")]
    pub fill: Option<bool>,
    #[schemars(description = "Line curve tension, 0 (straight) to 1 (line charts, optional)")]
    pub tension: Option<f64>,
    #[schemars(description = "Any other Obsidian Charts options, written as-is (e.g. {\"indexAxis\": \"y\", \"legendPosition\": \"bottom\"})")]
    pub extra_options: Option<Map<String, Value>>,
    #[schemars(description = "Index of an existing chart block to replace (from list_code_blocks with language 'chart'). Omit to insert a new one.")]
    pub index: Option<usize>,
    #[schemars(description = "When inserting, heading to put the chart under; created if missing. Omit to append at the end.")]
    pub heading: Option<String>,
}
