use regex::Regex;
use rmcp::schemars;
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct NoteContent {
    pub frontmatter: HashMap<String, FrontmatterValue>,
    pub body: String,
}

pub fn parse(content: &str) -> NoteContent {
    if content.starts_with("---") {
        let without_opening = &content[3..];
        if let Some(end_idx) = without_opening.find("---") {
            let yaml_str = &without_opening[..end_idx];
            if yaml_str.len() > MAX_FRONTMATTER_BYTES
                || yaml_anchor_or_alias_regex().is_match(yaml_str)
            {
                return NoteContent {
                    frontmatter: HashMap::new(),
                    body: content.to_string(),
                };
            }
            let body = without_opening[end_idx + 3..].trim_start_matches('\n').to_string();

            let frontmatter = parse_yaml(yaml_str);
            return NoteContent { frontmatter, body };
        }
    }

    NoteContent {
        frontmatter: HashMap::new(),
        body: content.to_string(),
    }
}

fn parse_yaml(yaml_str: &str) -> HashMap<String, FrontmatterValue> {
    let mut map = HashMap::new();
    if let Ok(docs) = YamlLoader::load_from_str(yaml_str) {
        if let Some(doc) = docs.first() {
            if let Some(hash) = doc.as_hash() {
                for (key, value) in hash {
                    if let (Some(k), Some(v)) = (key.as_str(), yaml_to_frontmatter_value(value)) {
                        map.insert(k.to_string(), v);
                    }
                }
            }
        }
    }
    map
}

fn yaml_to_frontmatter_value(yaml: &Yaml) -> Option<FrontmatterValue> {
    match yaml {
        Yaml::Array(arr) => {
            let items: Vec<String> = arr.iter().filter_map(yaml_scalar_to_string).collect();
            Some(FrontmatterValue::List(items))
        }
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
