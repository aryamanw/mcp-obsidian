//! Editing of Obsidian `.canvas` files (the JSON Canvas format), including
//! the `styleAttributes` extension used by the Advanced Canvas plugin for
//! node shapes/borders and edge path/arrow styles.

use crate::parse::ids;
use rmcp::schemars;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const NODE_GAP: f64 = 40.0;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct NodeSpec {
    #[schemars(description = "Node id. If it matches an existing node, that node is updated (only the given fields change); otherwise a new node is created (random id if omitted).")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    #[schemars(description = "Required for new nodes: 'text', 'file' (embeds a vault file/note), 'link' (embeds a URL), or 'group'")]
    pub kind: Option<String>,
    #[schemars(description = "X position (new nodes without x/y are auto-placed below existing content)")]
    pub x: Option<f64>,
    #[schemars(description = "Y position")]
    pub y: Option<f64>,
    #[schemars(description = "Width (defaults depend on node type)")]
    pub width: Option<f64>,
    #[schemars(description = "Height (defaults depend on node type)")]
    pub height: Option<f64>,
    #[schemars(description = "Markdown text (text nodes)")]
    pub text: Option<String>,
    #[schemars(description = "Vault-relative file path, e.g. 'Projects/Plan.md' (file nodes)")]
    pub file: Option<String>,
    #[schemars(description = "Heading or block subpath within the file, e.g. '#Goals' (file nodes)")]
    pub subpath: Option<String>,
    #[schemars(description = "URL (link nodes)")]
    pub url: Option<String>,
    #[schemars(description = "Label (group nodes)")]
    pub label: Option<String>,
    #[schemars(description = "Color: a preset '1'-'6' (red, orange, yellow, green, cyan, purple) or a hex color like '#ff0000'")]
    pub color: Option<String>,
    #[schemars(description = "Background image path (group nodes)")]
    pub background: Option<String>,
    #[schemars(description = "Background image style: 'cover', 'ratio', or 'repeat' (group nodes)")]
    pub background_style: Option<String>,
    #[schemars(description = "Advanced Canvas style attributes, merged into the node's existing ones. Common keys: shape ('pill', 'diamond', 'parallelogram', 'circle', 'predefined-process', 'document', 'database'), border ('dashed', 'dotted', 'invisible'), textAlign ('left', 'center', 'right'). Set a key to null to remove it.")]
    pub style_attributes: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct EdgeSpec {
    #[schemars(description = "Edge id. Matching an existing edge updates it; otherwise a new edge is created (random id if omitted).")]
    pub id: Option<String>,
    #[schemars(description = "Id of the node the edge starts at (required for new edges)")]
    pub from_node: Option<String>,
    #[schemars(description = "Id of the node the edge ends at (required for new edges)")]
    pub to_node: Option<String>,
    #[schemars(description = "Side the edge leaves from: 'top', 'right', 'bottom', or 'left'")]
    pub from_side: Option<String>,
    #[schemars(description = "Side the edge arrives at: 'top', 'right', 'bottom', or 'left'")]
    pub to_side: Option<String>,
    #[schemars(description = "Start endpoint shape: 'none' (default) or 'arrow'")]
    pub from_end: Option<String>,
    #[schemars(description = "End endpoint shape: 'arrow' (default) or 'none'")]
    pub to_end: Option<String>,
    #[schemars(description = "Edge label")]
    pub label: Option<String>,
    #[schemars(description = "Color: a preset '1'-'6' or a hex color")]
    pub color: Option<String>,
    #[schemars(description = "Advanced Canvas style attributes, merged into existing ones. Common keys: path ('dotted', 'short-dashed', 'long-dashed'), arrow ('triangle-outline', 'thin-triangle', 'halved-triangle', 'diamond', 'diamond-outline', 'circle', 'circle-outline', 'blunt'), pathfindingMethod ('direct', 'square', 'a-star'). Set a key to null to remove it.")]
    pub style_attributes: Option<Map<String, Value>>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct EditSummary {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

pub fn empty_canvas() -> Value {
    json!({ "nodes": [], "edges": [] })
}

fn check_one_of(field: &str, value: &Option<String>, allowed: &[&str]) -> anyhow::Result<()> {
    match value {
        Some(v) if !allowed.contains(&v.as_str()) => Err(anyhow::anyhow!(
            "Invalid {} '{}' (use one of: {})", field, v, allowed.join(", ")
        )),
        _ => Ok(()),
    }
}

fn check_color(color: &Option<String>) -> anyhow::Result<()> {
    let Some(c) = color else { return Ok(()) };
    let preset = c.len() == 1 && ("1"..="6").contains(&c.as_str());
    let hex = c.strip_prefix('#')
        .is_some_and(|h| (h.len() == 3 || h.len() == 6) && h.chars().all(|ch| ch.is_ascii_hexdigit()));
    if preset || hex {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Invalid color '{}' (use '1'-'6' or a hex color like '#ff0000')", c))
    }
}

/// JSON Canvas stores positions and sizes as integers.
fn int(v: f64) -> Value {
    json!(v.round() as i64)
}

/// Sets `key` on `obj` when `value` is `Some`.
fn set<T: serde::Serialize>(obj: &mut Map<String, Value>, key: &str, value: &Option<T>) {
    if let Some(v) = value {
        obj.insert(key.to_string(), json!(v));
    }
}

fn merge_style(obj: &mut Map<String, Value>, style: &Option<Map<String, Value>>) {
    let Some(style) = style else { return };
    let entry = obj.entry("styleAttributes").or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let target = entry.as_object_mut().unwrap();
    for (k, v) in style {
        if v.is_null() {
            target.remove(k);
        } else {
            target.insert(k.clone(), v.clone());
        }
    }
}

fn default_size(kind: &str, text: Option<&str>) -> (f64, f64) {
    match kind {
        "text" => {
            // Roughly 30 characters per line at the default width.
            let lines: usize = text.unwrap_or("").lines()
                .map(|l| l.chars().count().div_ceil(30).max(1))
                .sum::<usize>()
                .max(1);
            (250.0, (lines as f64 * 24.0 + 36.0).max(60.0))
        }
        "file" | "link" => (400.0, 400.0),
        _ => (500.0, 400.0),
    }
}

/// Top-left of a new row below all of `nodes`, aligned with the leftmost one.
fn next_row(nodes: &[Value]) -> (f64, f64) {
    let num = |n: &Value, k: &str| n[k].as_f64().unwrap_or(0.0);
    if nodes.is_empty() {
        return (0.0, 0.0);
    }
    let left = nodes.iter().map(|n| num(n, "x")).fold(f64::INFINITY, f64::min);
    let bottom = nodes.iter().map(|n| num(n, "y") + num(n, "height")).fold(f64::NEG_INFINITY, f64::max);
    (left, bottom + NODE_GAP * 2.0)
}

fn node_array(doc: &mut Value, key: &str) -> anyhow::Result<Vec<Value>> {
    match doc.get_mut(key).map(Value::take) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(anyhow::anyhow!("Canvas '{}' is not an array", key)),
    }
}

/// Applies removals, then node upserts, then edge upserts to a canvas
/// document. Removing a node also removes every edge attached to it.
pub fn apply_edits(
    doc: &mut Value,
    node_specs: &[NodeSpec],
    edge_specs: &[EdgeSpec],
    remove_ids: &[String],
) -> anyhow::Result<EditSummary> {
    if !doc.is_object() {
        return Err(anyhow::anyhow!("Canvas file is not a JSON object"));
    }
    let mut nodes = node_array(doc, "nodes")?;
    let mut edges = node_array(doc, "edges")?;
    let mut summary = EditSummary::default();

    let id_of = |v: &Value| v["id"].as_str().unwrap_or("").to_string();

    // --- removals ---
    let remove: HashSet<&str> = remove_ids.iter().map(String::as_str).collect();
    for v in nodes.iter().chain(edges.iter()) {
        let id = id_of(v);
        if remove.contains(id.as_str()) {
            summary.removed.push(id);
        }
    }
    let missing: Vec<&str> = remove.iter().copied()
        .filter(|id| !summary.removed.iter().any(|r| r == id)).collect();
    if !missing.is_empty() {
        return Err(anyhow::anyhow!("No node or edge with id: {}", missing.join(", ")));
    }
    nodes.retain(|n| !remove.contains(id_of(n).as_str()));
    let node_ids: HashSet<String> = nodes.iter().map(id_of).collect();
    edges.retain(|e| {
        let keep = !remove.contains(id_of(e).as_str())
            && node_ids.contains(e["fromNode"].as_str().unwrap_or(""))
            && node_ids.contains(e["toNode"].as_str().unwrap_or(""));
        if !keep && !summary.removed.contains(&id_of(e)) {
            summary.removed.push(id_of(e));
        }
        keep
    });

    // --- node upserts ---
    // New nodes without a position go in a row below everything already on
    // the canvas (including nodes added earlier in this same call).
    let mut auto_slot: Option<(f64, f64)> = None;

    for spec in node_specs {
        check_one_of("node type", &spec.kind, &["text", "file", "link", "group"])?;
        check_one_of("background_style", &spec.background_style, &["cover", "ratio", "repeat"])?;
        check_color(&spec.color)?;

        let existing = spec.id.as_ref().and_then(|id| nodes.iter().position(|n| n["id"] == id.as_str()));
        let (index, is_new) = match existing {
            Some(i) => (i, false),
            None => {
                let kind = spec.kind.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("New node '{}' needs a type", spec.id.as_deref().unwrap_or("(no id)")))?;
                let required = match kind {
                    "text" => ("text", spec.text.is_some()),
                    "file" => ("file", spec.file.is_some()),
                    "link" => ("url", spec.url.is_some()),
                    _ => ("", true),
                };
                if !required.1 {
                    return Err(anyhow::anyhow!("New {} node needs '{}'", kind, required.0));
                }
                let id = spec.id.clone().unwrap_or_else(|| ids::random_id(16, ids::HEX));
                let (dw, dh) = default_size(kind, spec.text.as_deref());
                let (w, h) = (spec.width.unwrap_or(dw), spec.height.unwrap_or(dh));
                let (x, y) = match (spec.x, spec.y) {
                    (Some(x), Some(y)) => (x, y),
                    (x, y) => {
                        let (slot_x, slot_y) = *auto_slot.get_or_insert_with(|| next_row(&nodes));
                        let pos = (x.unwrap_or(slot_x), y.unwrap_or(slot_y));
                        auto_slot = Some((pos.0 + w + NODE_GAP, slot_y));
                        pos
                    }
                };
                nodes.push(json!({ "id": id, "type": kind, "x": int(x), "y": int(y), "width": int(w), "height": int(h) }));
                (nodes.len() - 1, true)
            }
        };

        let obj = nodes[index].as_object_mut().unwrap();
        if !is_new {
            set(obj, "type", &spec.kind);
            for (key, value) in [("x", spec.x), ("y", spec.y), ("width", spec.width), ("height", spec.height)] {
                if let Some(v) = value {
                    obj.insert(key.to_string(), int(v));
                }
            }
        }
        set(obj, "text", &spec.text);
        set(obj, "file", &spec.file);
        set(obj, "subpath", &spec.subpath);
        set(obj, "url", &spec.url);
        set(obj, "label", &spec.label);
        set(obj, "color", &spec.color);
        set(obj, "background", &spec.background);
        set(obj, "backgroundStyle", &spec.background_style);
        merge_style(obj, &spec.style_attributes);

        let id = id_of(&nodes[index]);
        if is_new { summary.created.push(id) } else { summary.updated.push(id) }
    }

    // --- edge upserts ---
    let node_ids: HashSet<String> = nodes.iter().map(id_of).collect();
    let sides = ["top", "right", "bottom", "left"];
    for spec in edge_specs {
        check_one_of("from_side", &spec.from_side, &sides)?;
        check_one_of("to_side", &spec.to_side, &sides)?;
        check_one_of("from_end", &spec.from_end, &["none", "arrow"])?;
        check_one_of("to_end", &spec.to_end, &["none", "arrow"])?;
        check_color(&spec.color)?;
        for n in [&spec.from_node, &spec.to_node].into_iter().flatten() {
            if !node_ids.contains(n) {
                return Err(anyhow::anyhow!("Edge references unknown node '{}'", n));
            }
        }

        let existing = spec.id.as_ref().and_then(|id| edges.iter().position(|e| e["id"] == id.as_str()));
        let (index, is_new) = match existing {
            Some(i) => (i, false),
            None => {
                let (Some(from), Some(to)) = (&spec.from_node, &spec.to_node) else {
                    return Err(anyhow::anyhow!("New edge needs from_node and to_node"));
                };
                let id = spec.id.clone().unwrap_or_else(|| ids::random_id(16, ids::HEX));
                edges.push(json!({ "id": id, "fromNode": from, "toNode": to }));
                (edges.len() - 1, true)
            }
        };

        let obj = edges[index].as_object_mut().unwrap();
        set(obj, "fromNode", &spec.from_node);
        set(obj, "toNode", &spec.to_node);
        set(obj, "fromSide", &spec.from_side);
        set(obj, "toSide", &spec.to_side);
        set(obj, "fromEnd", &spec.from_end);
        set(obj, "toEnd", &spec.to_end);
        set(obj, "label", &spec.label);
        set(obj, "color", &spec.color);
        merge_style(obj, &spec.style_attributes);

        let id = id_of(&edges[index]);
        if is_new { summary.created.push(id) } else { summary.updated.push(id) }
    }

    doc["nodes"] = Value::Array(nodes);
    doc["edges"] = Value::Array(edges);
    Ok(summary)
}

/// Serializes a canvas the way Obsidian does (tab-indented JSON).
pub fn to_string(doc: &Value) -> String {
    use serde::Serialize;
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    doc.serialize(&mut ser).expect("serializing a serde_json::Value cannot fail");
    String::from_utf8(buf).expect("serde_json always emits UTF-8")
}
