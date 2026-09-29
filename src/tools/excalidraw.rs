use crate::parse::excalidraw::ElementSpec;
use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadDrawingRequest {
    #[schemars(description = "Path to the drawing ('.excalidraw.md' or '.excalidraw'; the extension is optional)")]
    pub path: String,
    #[schemars(description = "Also return the full raw Excalidraw scene JSON (optional, defaults to false)")]
    pub include_raw: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateDrawingRequest {
    #[schemars(description = "Path for the new drawing; '.excalidraw.md' is appended if missing")]
    pub path: String,
    #[schemars(description = "Elements to draw. Shapes with text get a centered label; arrows with from/to connect two elements by id.")]
    pub elements: Vec<ElementSpec>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddDrawingElementsRequest {
    #[schemars(description = "Path to an existing drawing")]
    pub path: String,
    #[schemars(description = "Elements to add. Arrows' from/to may reference existing element ids (see read_drawing) or ids of elements in this call.")]
    pub elements: Vec<ElementSpec>,
}
