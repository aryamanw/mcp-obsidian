use std::collections::HashMap;
use yaml_rust2::{YamlLoader, Yaml};

#[derive(Debug, Clone)]
pub struct NoteContent {
    pub frontmatter: HashMap<String, String>,
    pub body: String,
    #[allow(dead_code)]
    pub raw: String,
}

pub fn parse(content: &str) -> NoteContent {
    let raw = content.to_string();

    if content.starts_with("---") {
        let without_opening = &content[3..];
        if let Some(end_idx) = without_opening.find("---") {
            let yaml_str = &without_opening[..end_idx];
            let body = without_opening[end_idx + 3..].trim_start_matches('\n').to_string();

            let frontmatter = parse_yaml(yaml_str);
            return NoteContent { frontmatter, body, raw };
        }
    }

    NoteContent {
        frontmatter: HashMap::new(),
        body: content.to_string(),
        raw,
    }
}

fn parse_yaml(yaml_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(docs) = YamlLoader::load_from_str(yaml_str) {
        if let Some(doc) = docs.first() {
            if let Some(hash) = doc.as_hash() {
                for (key, value) in hash {
                    if let (Some(k), Some(v)) = (key.as_str(), yaml_to_string(value)) {
                        map.insert(k.to_string(), v);
                    }
                }
            }
        }
    }
    map
}

fn yaml_to_string(yaml: &Yaml) -> Option<String> {
    match yaml {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(r) => Some(r.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Null => Some("".to_string()),
        Yaml::Array(arr) => {
            let items: Vec<String> = arr.iter().filter_map(yaml_to_string).collect();
            Some(items.join(", "))
        }
        _ => None,
    }
}
