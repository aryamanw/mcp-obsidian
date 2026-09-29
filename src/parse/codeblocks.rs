/// A fenced code block (```lang ... ``` or ~~~lang ... ~~~). `start` is the
/// byte offset of the opening fence line and `end` the byte offset just past
/// the closing fence line (including its newline, if any), so
/// `body[start..end]` is the whole block and can be spliced out/replaced.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    pub language: String,
    pub content: String,
    pub start: usize,
    pub end: usize,
}

/// Parses an opening/closing fence line into (fence char, fence length,
/// info string). Up to 3 spaces of indentation are allowed, per CommonMark.
fn parse_fence(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let fence_char = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = rest.chars().take_while(|c| *c == fence_char).count();
    if len < 3 {
        return None;
    }
    Some((fence_char, len, rest[len..].trim()))
}

/// Extracts every fenced code block in `body`, in document order. An
/// unclosed fence runs to the end of the document (CommonMark behavior).
pub fn extract_code_blocks(body: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    // (fence char, fence len, language, block start, content start)
    let mut open: Option<(char, usize, String, usize, usize)> = None;
    let mut offset = 0usize;

    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        match &open {
            None => {
                if let Some((ch, len, info)) = parse_fence(content) {
                    // Backtick fences can't have backticks in their info string.
                    if !(ch == '`' && info.contains('`')) {
                        let language = info.split_whitespace().next().unwrap_or("").to_string();
                        open = Some((ch, len, language, offset, offset + line.len()));
                    }
                }
            }
            Some((ch, len, _, _, _)) => {
                if let Some((c, l, info)) = parse_fence(content) {
                    if c == *ch && l >= *len && info.is_empty() {
                        let (_, _, language, start, content_start) = open.take().unwrap();
                        blocks.push(CodeBlock {
                            language,
                            content: body[content_start..offset].to_string(),
                            start,
                            end: offset + line.len(),
                        });
                    }
                }
            }
        }
        offset += line.len();
    }

    if let Some((_, _, language, start, content_start)) = open {
        blocks.push(CodeBlock {
            language,
            content: body[content_start.min(body.len())..].to_string(),
            start,
            end: body.len(),
        });
    }

    blocks
}

/// Renders a fenced block with a fence long enough that `content` can't
/// close it early (e.g. content that itself contains ``` lines).
pub fn render_code_block(language: &str, content: &str) -> String {
    let longest_run = content.lines()
        .filter_map(|l| parse_fence(l).filter(|(c, _, _)| *c == '`').map(|(_, n, _)| n))
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{}{}\n{}\n{}\n", fence, language, content.trim_end_matches('\n'), fence)
}

/// Diagram types Mermaid recognizes as the first keyword of a diagram.
pub const MERMAID_DIAGRAM_TYPES: &[&str] = &[
    "graph", "flowchart", "sequenceDiagram", "classDiagram", "classDiagram-v2",
    "stateDiagram", "stateDiagram-v2", "erDiagram", "journey", "gantt", "pie",
    "quadrantChart", "requirementDiagram", "gitGraph", "C4Context", "C4Container",
    "C4Component", "C4Dynamic", "C4Deployment", "mindmap", "timeline", "zenuml",
    "sankey-beta", "xychart-beta", "block-beta", "packet-beta", "kanban",
    "architecture-beta", "radar-beta", "treemap-beta",
];

/// Returns the diagram type keyword of a Mermaid source (e.g. "flowchart"
/// for "flowchart LR\n  A --> B"), skipping blank lines, `%%` comments/
/// directives, and a leading `---` YAML config block. `None` means the
/// first real line doesn't start with a known diagram type.
pub fn mermaid_diagram_type(source: &str) -> Option<&'static str> {
    let mut lines = source.lines().map(str::trim).peekable();
    if lines.peek() == Some(&"---") {
        lines.next();
        for l in lines.by_ref() {
            if l == "---" {
                break;
            }
        }
    }
    let first = lines.find(|l| !l.is_empty() && !l.starts_with("%%"))?;
    let keyword = first.split(|c: char| c.is_whitespace() || c == ':').next()?;
    MERMAID_DIAGRAM_TYPES.iter().copied().find(|t| *t == keyword)
}

/// Chart types supported by the Obsidian Charts plugin.
pub const CHART_TYPES: &[&str] = &["bar", "line", "pie", "doughnut", "radar", "polarArea"];

pub struct ChartSeries<'a> {
    pub title: Option<&'a str>,
    pub data: &'a [f64],
}

/// Builds the YAML body of an Obsidian Charts ```chart block. Values are
/// written as JSON, which is valid YAML flow syntax, so labels/titles with
/// colons, quotes, etc. don't need hand-rolled YAML escaping.
pub fn build_chart_yaml(
    chart_type: &str,
    labels: &[String],
    series: &[ChartSeries],
    options: &serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<String> {
    if !CHART_TYPES.contains(&chart_type) {
        return Err(anyhow::anyhow!(
            "Unsupported chart type '{}' (use one of: {})", chart_type, CHART_TYPES.join(", ")
        ));
    }
    if series.is_empty() {
        return Err(anyhow::anyhow!("Chart needs at least one series"));
    }
    for s in series {
        if s.data.len() != labels.len() {
            return Err(anyhow::anyhow!(
                "Series '{}' has {} data points but there are {} labels",
                s.title.unwrap_or(""), s.data.len(), labels.len()
            ));
        }
    }

    let mut out = format!("type: {}\nlabels: {}\nseries:\n", chart_type, json(labels));
    for s in series {
        match s.title {
            Some(t) => out.push_str(&format!("  - title: {}\n    data: {}\n", json(t), json(s.data))),
            None => out.push_str(&format!("  - data: {}\n", json(s.data))),
        }
    }
    for (k, v) in options {
        if matches!(k.as_str(), "type" | "labels" | "series") {
            continue;
        }
        out.push_str(&format!("{}: {}\n", k, v));
    }
    Ok(out)
}

fn json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
