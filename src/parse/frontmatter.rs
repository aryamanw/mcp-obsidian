use regex::Regex;
use rmcp::schemars;
use std::sync::OnceLock;
use yaml_rust2::{YamlLoader, Yaml};

const MAX_FRONTMATTER_BYTES: usize = 8192;

fn yaml_anchor_or_alias_regex() -> &'static Regex {
    // YAML anchors (`&name`) and aliases (`*name`) let a small document
    // reference-multiply into an exponentially larger in-memory tree
    // ("billion laughs" / entity expansion, CWE-776). A handful of small
    // fan-out levels easily produce 10s of millions of elements from well
    // under the 8KB frontmatter cap below, hanging the single-process
    // server on the next parse. Frontmatter never has a legitimate need for
    // either construct, so reject outright rather than trying to bound the
    // expansion after the fact.
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)(^|\s)[&*][A-Za-z0-9_-]+").unwrap())
}

/// A frontmatter property's value. Obsidian's Properties panel treats
/// `tags`, `aliases`, `cssclasses` (and any user-defined list property) as
/// YAML sequences; everything else is a plain scalar. Modeling both shapes
/// — rather than forcing every value to a string, which forced list values
/// to be joined into a single comma string on write and collapsed real
/// YAML sequences into that same joined string on read — lets list-typed
/// properties round-trip as actual sequences instead of a scalar string
/// Obsidian's UI reads as one invalid tag literal full of commas.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum FrontmatterValue {
    String(String),
    List(Vec<String>),
}

impl PartialEq<str> for FrontmatterValue {
    fn eq(&self, other: &str) -> bool {
        matches!(self, FrontmatterValue::String(s) if s == other)
    }
}

/// A note's properties, in the order they appear in the file. Order
/// matters: Obsidian shows properties in file order, and rewriting a note
/// shouldn't shuffle them. New keys are appended; updated keys keep their
/// position.
pub type Frontmatter = indexmap::IndexMap<String, FrontmatterValue>;

#[derive(Debug, Clone)]
pub struct NoteContent {
    pub frontmatter: Frontmatter,
    pub body: String,
    /// Why the note's frontmatter block couldn't be fully parsed, if it
    /// couldn't: invalid YAML, over the size cap, anchors/aliases, or values
    /// (nested maps/lists) that `FrontmatterValue` can't represent. When set,
    /// `frontmatter` is missing some or all of the note's real properties,
    /// so writing it back would destroy them — write paths must refuse.
    pub problem: Option<String>,
}

pub fn parse(content: &str) -> NoteContent {
    if content.starts_with("---") {
        let without_opening = &content[3..];
        if let Some(end_idx) = without_opening.find("---") {
            let yaml_str = &without_opening[..end_idx];
            let rejected = if yaml_str.len() > MAX_FRONTMATTER_BYTES {
                Some(format!("frontmatter exceeds {} bytes", MAX_FRONTMATTER_BYTES))
            } else if yaml_anchor_or_alias_regex().is_match(yaml_str) {
                Some("frontmatter uses YAML anchors/aliases".to_string())
            } else {
                None
            };
            if rejected.is_some() {
                return NoteContent {
                    frontmatter: Frontmatter::new(),
                    body: content.to_string(),
                    problem: rejected,
                };
            }
            let body = without_opening[end_idx + 3..].trim_start_matches('\n').to_string();

            let (frontmatter, problem) = parse_yaml(yaml_str);
            return NoteContent { frontmatter, body, problem };
        }
    }

    NoteContent {
        frontmatter: Frontmatter::new(),
        body: content.to_string(),
        problem: None,
    }
}

fn parse_yaml(yaml_str: &str) -> (Frontmatter, Option<String>) {
    let mut map = Frontmatter::new();
    let docs = match YamlLoader::load_from_str(yaml_str) {
        Ok(docs) => docs,
        Err(e) => return (map, Some(format!("invalid YAML: {}", e))),
    };
    let hash = match docs.first() {
        None | Some(Yaml::Null) => return (map, None),
        Some(Yaml::Hash(hash)) => hash,
        Some(_) => return (map, Some("frontmatter is not a key-value mapping".to_string())),
    };

    let mut problem = None;
    for (key, value) in hash {
        match (yaml_scalar_to_string(key), yaml_to_frontmatter_value(value)) {
            (Some(k), Some(v)) => {
                map.insert(k, v);
            }
            (k, _) => {
                problem.get_or_insert_with(|| format!(
                    "property '{}' has a nested value that can't be preserved",
                    k.unwrap_or_else(|| format!("{:?}", key)),
                ));
            }
        }
    }
    (map, problem)
}

fn yaml_to_frontmatter_value(yaml: &Yaml) -> Option<FrontmatterValue> {
    match yaml {
        // A list with any non-scalar item (e.g. `[[X]]`, which YAML reads as
        // a list containing a list) can't be represented; dropping those
        // items would silently lose data on the next write.
        Yaml::Array(arr) => arr.iter().map(yaml_scalar_to_string).collect::<Option<Vec<_>>>()
            .map(FrontmatterValue::List),
        other => yaml_scalar_to_string(other).map(FrontmatterValue::String),
    }
}

fn yaml_scalar_to_string(yaml: &Yaml) -> Option<String> {
    match yaml {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(r) => Some(r.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Null => Some("".to_string()),
        _ => None,
    }
}

fn reads_back_plain(s: &str) -> bool {
    if s.contains(['\n', '\r', '\t']) {
        return false;
    }
    let Ok(docs) = YamlLoader::load_from_str(&format!("v: {}", s)) else { return false };
    match docs.first().map(|doc| &doc["v"]) {
        // `key:` with nothing after it is how an empty property is written.
        Some(Yaml::Null) => s.is_empty(),
        Some(value) => yaml_scalar_to_string(value).as_deref() == Some(s),
        None => false,
    }
}

/// Renders `s` as a YAML scalar that reads back as exactly `s`. It's written
/// plain when YAML already parses it that way (so `status: active`, `year:
/// 2024`, and `done: true` stay unquoted and keep their types in Obsidian);
/// anything YAML would misread — `[[wikilinks]]` (a nested list), `a: b`,
/// `#tag` (a comment), `null`, leading/trailing spaces, newlines — is
/// double-quoted and escaped instead, the way Obsidian writes links.
pub fn yaml_scalar(s: &str) -> String {
    if reads_back_plain(s) {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Splits `content` into its raw frontmatter block (opening `---` through
/// the closing `---` line, inclusive) and the rest, without re-serializing
/// anything. Plugin-managed files (Kanban boards, Excalidraw drawings) are
/// rewritten through this so their frontmatter survives byte-for-byte —
/// `serialize_frontmatter` only round-trips flat string/list values.
pub fn split_raw(content: &str) -> (&str, &str) {
    if let Some(after_open) = content.strip_prefix("---") {
        if after_open.starts_with('\n') || after_open.starts_with("\r\n") {
            let mut offset = 3;
            for line in after_open.split_inclusive('\n') {
                offset += line.len();
                if line.trim_end() == "---" {
                    return content.split_at(offset);
                }
            }
        }
    }
    ("", content)
}
