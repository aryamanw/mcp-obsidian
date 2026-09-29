use crate::parse::tables::Alignment;
use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadTablesRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteTableRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Column headers")]
    pub headers: Vec<String>,
    #[schemars(description = "Rows of cell values; short rows are padded, long rows truncated to the header count")]
    pub rows: Vec<Vec<String>>,
    #[schemars(description = "Per-column alignment: 'none', 'left', 'center', or 'right' (optional; when replacing, defaults to the existing table's)")]
    pub alignments: Option<Vec<Alignment>>,
    #[schemars(description = "Index of an existing table to replace (from read_tables). Omit to insert a new table.")]
    pub table_index: Option<usize>,
    #[schemars(description = "When inserting, heading to put the table under (e.g. '## Budget'); created if missing. Omit to append at the end of the note.")]
    pub heading: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddTableRowsRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Index of the table (from read_tables; 0 is the first table in the note)")]
    pub table_index: usize,
    #[schemars(description = "Rows to append")]
    pub rows: Vec<Vec<String>>,
}
