//! Parsing and generation of Obsidian Excalidraw plugin drawings. A drawing
//! is a markdown note (`*.excalidraw.md`, frontmatter `excalidraw-plugin:
//! parsed`) whose body holds a `## Text Elements` section (each text element
//! as `text ^elementId`, searchable/linkable from the rest of the vault) and
//! a `## Drawing` section containing the Excalidraw scene JSON, either as
//! plain ```json or LZ-string-compressed ```compressed-json.

use crate::parse::{codeblocks, ids};
use regex::Regex;
use rmcp::schemars;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

pub const NEW_DRAWING_HEADER: &str = "---\n\nexcalidraw-plugin: parsed\ntags: [excalidraw]\n\n---\n==⚠  Switch to EXCALIDRAW VIEW in the MORE OPTIONS menu of this document. ⚠== You can decompress Drawing data with the command palette: 'Decompress current Excalidraw file'. For more info check in plugin settings under 'Saving'\n\n\n# Excalidraw Data\n\n";

const DEFAULT_FONT_SIZE: f64 = 20.0;
const LINE_HEIGHT: f64 = 1.25;
const BINDING_GAP: f64 = 8.0;

/// A simplified, LLM-friendly description of an Excalidraw element. Shapes
/// with `text` get a centered, bound label; arrows/lines with `from`/`to`
/// are bound to (and drawn between) those elements.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ElementSpec {
    #[schemars(description = "Optional element id (random if omitted). Use it to reference this element from an arrow's from/to in the same call.")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    #[schemars(description = "One of: rectangle, ellipse, diamond, text, arrow, line")]
    pub kind: String,
    #[schemars(description = "X of the top-left corner (required for shapes and text; for arrows/lines without from/to, the start point)")]
    pub x: Option<f64>,
    #[schemars(description = "Y of the top-left corner (see x)")]
    pub y: Option<f64>,
    #[schemars(description = "Width (optional; shapes auto-size to fit their text)")]
    pub width: Option<f64>,
    #[schemars(description = "Height (optional; shapes auto-size to fit their text)")]
    pub height: Option<f64>,
    #[schemars(description = "Text for text elements, or a centered label for shapes and arrows")]
    pub text: Option<String>,
    #[schemars(description = "Font size (default 20)")]
    pub font_size: Option<f64>,
    #[schemars(description = "Stroke color, e.g. '#1e1e1e' (default)")]
    pub stroke_color: Option<String>,
    #[schemars(description = "Fill color, e.g. '#a5d8ff' (default 'transparent')")]
    pub background_color: Option<String>,
    #[schemars(description = "Fill style: 'solid' (default), 'hachure', or 'cross-hatch'")]
    pub fill_style: Option<String>,
    #[schemars(description = "Stroke style: 'solid' (default), 'dashed', or 'dotted'")]
    pub stroke_style: Option<String>,
    #[schemars(description = "Arrow/line only: id of the element it starts from (existing or created in this call)")]
    pub from: Option<String>,
    #[schemars(description = "Arrow/line only: id of the element it points to (existing or created in this call)")]
    pub to: Option<String>,
    #[schemars(description = "Arrow/line only, when from/to aren't given: points relative to (x, y), e.g. [[0,0],[200,0]]")]
    pub points: Option<Vec<[f64; 2]>>,
    #[schemars(description = "Optional link on the element, e.g. '[[Some Note]]' or a URL")]
    pub link: Option<String>,
}

/// Where the scene JSON lives inside a drawing note's body.
pub struct SceneBlock {
    pub start: usize,
    pub end: usize,
    pub compressed: bool,
    pub scene: Value,
}

fn decompress(data: &str) -> anyhow::Result<String> {
    let compact: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let wide = lz_str::decompress_from_base64(&compact)
        .ok_or_else(|| anyhow::anyhow!("Could not decompress Excalidraw drawing data"))?;
    String::from_utf16(&wide).map_err(|_| anyhow::anyhow!("Decompressed drawing data is not valid UTF-16"))
}

/// Locates and decodes the scene JSON block under `## Drawing`.
pub fn find_scene(body: &str) -> anyhow::Result<SceneBlock> {
    let drawing_at = body.find("## Drawing")
        .ok_or_else(|| anyhow::anyhow!("No '## Drawing' section found; not an Excalidraw drawing"))?;
    let block = codeblocks::extract_code_blocks(body).into_iter()
        .find(|b| b.start > drawing_at && (b.language == "json" || b.language == "compressed-json"))
        .ok_or_else(|| anyhow::anyhow!("No drawing data block found under '## Drawing'"))?;

    let compressed = block.language == "compressed-json";
    let raw = if compressed { decompress(&block.content)? } else { block.content.clone() };
    let scene: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("Invalid Excalidraw scene JSON: {}", e))?;
    Ok(SceneBlock { start: block.start, end: block.end, compressed, scene })
}

fn text_entry_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.*) \^([A-Za-z0-9_-]+)$").unwrap())
}

/// Byte range of the lines under a `## {title}` heading, up to the next
/// heading or `%%` marker.
fn section_range(body: &str, title: &str) -> Option<(usize, usize)> {
    let heading = format!("## {}\n", title);
    let start = body.find(&heading)? + heading.len();
    let mut end = start;
    for line in body[start..].split_inclusive('\n') {
        if line.starts_with('#') || line.starts_with("%%") {
            break;
        }
        end += line.len();
    }
    Some((start, end))
}

/// Parses `## Text Elements`: entries of (possibly multi-line) text, each
/// ending with ` ^elementId`, separated by blank lines.
pub fn parse_text_elements(body: &str) -> Vec<(String, String)> {
    let Some((start, end)) = section_range(body, "Text Elements") else { return Vec::new() };
    let mut entries = Vec::new();
    let mut buffer: Vec<&str> = Vec::new();
    for line in body[start..end].lines() {
        if let Some(cap) = text_entry_regex().captures(line) {
            buffer.push(cap.get(1).unwrap().as_str());
            entries.push((cap[2].to_string(), buffer.join("\n")));
            buffer.clear();
        } else if !line.trim().is_empty() || !buffer.is_empty() {
            buffer.push(line);
        }
    }
    entries
}

/// Parses `id: value` sections such as `## Embedded Files` (image/file
/// embeds) and `## Element Links`.
pub fn parse_id_section(body: &str, title: &str) -> Vec<(String, String)> {
    let Some((start, end)) = section_range(body, title) else { return Vec::new() };
    body[start..end].lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Appends `text ^id` entries to the end of the `## Text Elements` section,
/// creating the section before `%%`/`## Drawing` if it doesn't exist.
pub fn append_text_elements(body: &str, entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        return body.to_string();
    }
    let rendered: String = entries.iter().map(|(id, text)| format!("{} ^{}\n\n", text, id)).collect();
    match section_range(body, "Text Elements") {
        Some((_, end)) => format!("{}{}{}", &body[..end], rendered, &body[end..]),
        None => {
            let insert_at = body.find("%%\n## Drawing").or_else(|| body.find("## Drawing")).unwrap_or(body.len());
            format!("{}## Text Elements\n{}{}", &body[..insert_at], rendered, &body[insert_at..])
        }
    }
}

/// Renders the `## Drawing` section holding `scene` as uncompressed JSON
/// (the plugin re-compresses it on its next save if that setting is on).
pub fn render_scene_block(scene: &Value) -> String {
    codeblocks::render_code_block("json", &serde_json::to_string_pretty(scene).unwrap_or_default())
}

pub fn render_drawing_section(scene: &Value) -> String {
    format!("%%\n## Drawing\n{}%%", render_scene_block(scene))
}

pub fn new_scene(elements: Vec<Value>) -> Value {
    json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://github.com/zsviczian/obsidian-excalidraw-plugin",
        "elements": elements,
        "appState": { "gridSize": null, "viewBackgroundColor": "#ffffff" },
        "files": {},
    })
}

fn f(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Condenses a scene into what's useful to read: live (non-deleted)
/// elements with their geometry, text, labels, and arrow endpoints.
pub fn summarize(scene: &Value) -> Vec<Value> {
    let elements: Vec<&Value> = scene.get("elements").and_then(Value::as_array)
        .map(|a| a.iter().filter(|e| e.get("isDeleted") != Some(&Value::Bool(true))).collect())
        .unwrap_or_default();
    let label_of = |id: &str| {
        elements.iter()
            .find(|e| e.get("containerId").and_then(Value::as_str) == Some(id))
            .and_then(|e| e.get("text").cloned())
    };

    elements.iter().filter(|e| e.get("containerId").is_none_or(Value::is_null)).map(|e| {
        let id = e.get("id").and_then(Value::as_str).unwrap_or("");
        let mut out = json!({
            "id": id,
            "type": e.get("type"),
            "x": f(e, "x").round(),
            "y": f(e, "y").round(),
            "width": f(e, "width").round(),
            "height": f(e, "height").round(),
        });
        let obj = out.as_object_mut().unwrap();
        if let Some(text) = e.get("text") {
            obj.insert("text".into(), text.clone());
        } else if let Some(label) = label_of(id) {
            obj.insert("label".into(), label);
        }
        for (key, name) in [("startBinding", "from"), ("endBinding", "to")] {
            if let Some(target) = e.get(key).and_then(|b| b.get("elementId")) {
                obj.insert(name.into(), target.clone());
            }
        }
        for key in ["link", "fileId"] {
            if let Some(v) = e.get(key).filter(|v| !v.is_null()) {
                obj.insert(key.into(), v.clone());
            }
        }
        out
    }).collect()
}

fn base_element(id: &str, kind: &str, x: f64, y: f64, w: f64, h: f64, spec: &ElementSpec) -> Value {
    let roundness = match kind {
        "rectangle" | "diamond" => json!({ "type": 3 }),
        "ellipse" | "text" => Value::Null,
        _ => json!({ "type": 2 }),
    };
    json!({
        "id": id,
        "type": kind,
        "x": x, "y": y, "width": w, "height": h,
        "angle": 0,
        "strokeColor": spec.stroke_color.as_deref().unwrap_or("#1e1e1e"),
        "backgroundColor": if kind == "text" { "transparent" } else { spec.background_color.as_deref().unwrap_or("transparent") },
        "fillStyle": spec.fill_style.as_deref().unwrap_or("solid"),
        "strokeWidth": 2,
        "strokeStyle": spec.stroke_style.as_deref().unwrap_or("solid"),
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": null,
        "roundness": roundness,
        "seed": ids::random_u32(),
        "version": 1,
        "versionNonce": ids::random_u32(),
        "isDeleted": false,
        "boundElements": [],
        "updated": chrono::Utc::now().timestamp_millis(),
        "link": spec.link,
        "locked": false,
    })
}

/// Approximate rendered size of `text` in Excalidraw's hand-drawn font.
fn measure(text: &str, font_size: f64) -> (f64, f64) {
    let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let lines = text.lines().count().max(1) as f64;
    (longest * font_size * 0.6, lines * font_size * LINE_HEIGHT)
}

fn text_element(id: &str, text: &str, x: f64, y: f64, font_size: f64, container: Option<&str>, spec: &ElementSpec) -> Value {
    let (w, h) = measure(text, font_size);
    let mut el = base_element(id, "text", x, y, w, h, spec);
    let obj = el.as_object_mut().unwrap();
    obj.insert("link".into(), Value::Null);
    for (k, v) in [
        ("text", json!(text)),
        ("originalText", json!(text)),
        ("fontSize", json!(font_size)),
        ("fontFamily", json!(1)),
        ("textAlign", json!(if container.is_some() { "center" } else { "left" })),
        ("verticalAlign", json!(if container.is_some() { "middle" } else { "top" })),
        ("containerId", json!(container)),
        ("autoResize", json!(true)),
        ("lineHeight", json!(LINE_HEIGHT)),
    ] {
        obj.insert(k.into(), v);
    }
    el
}

/// A centered label bound to a shape or arrow (Excalidraw's `containerId`).
fn label_for(container: &Value, text: &str, font_size: f64, spec: &ElementSpec) -> Value {
    let (w, h) = measure(text, font_size);
    let (cx, cy) = if container["type"] == "arrow" || container["type"] == "line" {
        let pts = container["points"].as_array().cloned().unwrap_or_default();
        let last = pts.last().cloned().unwrap_or(json!([0, 0]));
        (f(container, "x") + last[0].as_f64().unwrap_or(0.0) / 2.0, f(container, "y") + last[1].as_f64().unwrap_or(0.0) / 2.0)
    } else {
        (f(container, "x") + f(container, "width") / 2.0, f(container, "y") + f(container, "height") / 2.0)
    };
    let id = ids::random_id(8, ids::ALNUM);
    text_element(&id, text, cx - w / 2.0, cy - h / 2.0, font_size, container["id"].as_str(), spec)
}

/// Point where the ray from the center of `el` towards (tx, ty) exits its
/// bounding box, pushed out by `BINDING_GAP`.
fn edge_point(el: &Value, tx: f64, ty: f64) -> (f64, f64) {
    let (hw, hh) = (f(el, "width") / 2.0, f(el, "height") / 2.0);
    let (cx, cy) = (f(el, "x") + hw, f(el, "y") + hh);
    let (dx, dy) = (tx - cx, ty - cy);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return (cx, cy);
    }
    let tx_ = if dx != 0.0 { hw / dx.abs() } else { f64::INFINITY };
    let ty_ = if dy != 0.0 { hh / dy.abs() } else { f64::INFINITY };
    let t = tx_.min(ty_);
    (cx + dx * t + dx / len * BINDING_GAP, cy + dy * t + dy / len * BINDING_GAP)
}

fn center(el: &Value) -> (f64, f64) {
    (f(el, "x") + f(el, "width") / 2.0, f(el, "y") + f(el, "height") / 2.0)
}

fn push_bound(el: &mut Value, kind: &str, id: &str) {
    if !el["boundElements"].is_array() {
        el["boundElements"] = json!([]);
    }
    el["boundElements"].as_array_mut().unwrap().push(json!({ "type": kind, "id": id }));
}

/// Builds Excalidraw elements from `specs` and appends them to `elements`
/// (a scene's existing element list), wiring up bound labels and arrow
/// bindings in both directions. Returns `(id, text)` for every new text
/// element, for the note's `## Text Elements` section.
pub fn add_elements(elements: &mut Vec<Value>, specs: &[ElementSpec]) -> anyhow::Result<Vec<(String, String)>> {
    let mut taken: std::collections::HashSet<String> = elements.iter()
        .filter_map(|e| e["id"].as_str().map(str::to_string)).collect();
    let mut spec_ids = Vec::new();
    for spec in specs {
        let id = spec.id.clone().unwrap_or_else(|| ids::random_id(8, ids::ALNUM));
        if !taken.insert(id.clone()) {
            return Err(anyhow::anyhow!("Element id '{}' is already used in this drawing", id));
        }
        spec_ids.push(id);
    }

    let index_of = |elements: &[Value], id: &str| elements.iter().position(|e| e["id"] == id);
    let mut texts = Vec::new();

    // Shapes and text first, so arrows in the same call can bind to them.
    for (spec, id) in specs.iter().zip(&spec_ids) {
        let font_size = spec.font_size.unwrap_or(DEFAULT_FONT_SIZE);
        match spec.kind.as_str() {
            "rectangle" | "ellipse" | "diamond" => {
                let (x, y) = spec.x.zip(spec.y)
                    .ok_or_else(|| anyhow::anyhow!("{} '{}' needs x and y", spec.kind, id))?;
                let (tw, th) = spec.text.as_deref().map(|t| measure(t, font_size)).unwrap_or((0.0, 0.0));
                let scale = if spec.kind == "rectangle" { 1.0 } else { 1.5 };
                let w = spec.width.unwrap_or(((tw + 40.0) * scale).max(120.0));
                let h = spec.height.unwrap_or(((th + 40.0) * scale).max(60.0));
                let mut shape = base_element(id, &spec.kind, x, y, w, h, spec);
                if let Some(text) = &spec.text {
                    let label = label_for(&shape, text, font_size, spec);
                    let label_id = label["id"].as_str().unwrap().to_string();
                    push_bound(&mut shape, "text", &label_id);
                    texts.push((label_id, text.clone()));
                    elements.push(shape);
                    elements.push(label);
                } else {
                    elements.push(shape);
                }
            }
            "text" => {
                let (x, y) = spec.x.zip(spec.y)
                    .ok_or_else(|| anyhow::anyhow!("text '{}' needs x and y", id))?;
                let text = spec.text.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("text element '{}' needs text", id))?;
                elements.push(text_element(id, text, x, y, font_size, None, spec));
                texts.push((id.clone(), text.to_string()));
            }
            "arrow" | "line" => {}
            other => return Err(anyhow::anyhow!(
                "Unsupported element type '{}' (use rectangle, ellipse, diamond, text, arrow, or line)", other
            )),
        }
    }

    for (spec, id) in specs.iter().zip(&spec_ids) {
        if spec.kind != "arrow" && spec.kind != "line" {
            continue;
        }
        let mut el = match (&spec.from, &spec.to) {
            (Some(from), Some(to)) => {
                let fi = index_of(elements, from).ok_or_else(|| anyhow::anyhow!("'from' element '{}' not found", from))?;
                let ti = index_of(elements, to).ok_or_else(|| anyhow::anyhow!("'to' element '{}' not found", to))?;
                let (fcx, fcy) = center(&elements[fi]);
                let (tcx, tcy) = center(&elements[ti]);
                let (sx, sy) = edge_point(&elements[fi], tcx, tcy);
                let (ex, ey) = edge_point(&elements[ti], fcx, fcy);
                let mut el = base_element(id, &spec.kind, sx, sy, (ex - sx).abs(), (ey - sy).abs(), spec);
                el["points"] = json!([[0.0, 0.0], [ex - sx, ey - sy]]);
                el["startBinding"] = json!({ "elementId": from, "focus": 0, "gap": BINDING_GAP });
                el["endBinding"] = json!({ "elementId": to, "focus": 0, "gap": BINDING_GAP });
                push_bound(&mut elements[fi], "arrow", id);
                push_bound(&mut elements[ti], "arrow", id);
                el
            }
            (None, None) => {
                let (x, y) = spec.x.zip(spec.y)
                    .ok_or_else(|| anyhow::anyhow!("{} '{}' needs either from/to or x/y", spec.kind, id))?;
                let points = spec.points.clone()
                    .unwrap_or_else(|| vec![[0.0, 0.0], [spec.width.unwrap_or(100.0), spec.height.unwrap_or(0.0)]]);
                if points.len() < 2 {
                    return Err(anyhow::anyhow!("{} '{}' needs at least 2 points", spec.kind, id));
                }
                let xs = points.iter().map(|p| p[0]);
                let ys = points.iter().map(|p| p[1]);
                let w = xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min);
                let h = ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min);
                let mut el = base_element(id, &spec.kind, x, y, w, h, spec);
                el["points"] = json!(points);
                el["startBinding"] = Value::Null;
                el["endBinding"] = Value::Null;
                el
            }
            _ => return Err(anyhow::anyhow!("{} '{}' needs both from and to (or neither)", spec.kind, id)),
        };
        el["lastCommittedPoint"] = Value::Null;
        el["startArrowhead"] = Value::Null;
        el["endArrowhead"] = if spec.kind == "arrow" { json!("arrow") } else { Value::Null };

        if let Some(text) = &spec.text {
            let label = label_for(&el, text, spec.font_size.unwrap_or(DEFAULT_FONT_SIZE), spec);
            let label_id = label["id"].as_str().unwrap().to_string();
            push_bound(&mut el, "text", &label_id);
            texts.push((label_id, text.clone()));
            elements.push(el);
            elements.push(label);
        } else {
            elements.push(el);
        }
    }

    Ok(texts)
}
