use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]").unwrap())
}

#[derive(Debug, Clone)]
pub struct Wikilink {
    pub target: String,
    #[allow(dead_code)]
    pub alias: Option<String>,
    #[allow(dead_code)]
    pub raw: String,
}

pub fn extract_wikilinks(content: &str) -> Vec<Wikilink> {
    let re = wikilink_regex();
    let mut links = Vec::new();

    for cap in re.captures_iter(content) {
        let target = cap[1].to_string();
        let alias = cap.get(2).map(|m| m.as_str().to_string());
        let raw = cap[0].to_string();
        links.push(Wikilink { target, alias, raw });
    }

    links
}

pub fn resolve_wikilink(target: &str, vault_path: &Path) -> Option<PathBuf> {
    let direct = vault_path.join(format!("{}.md", target));
    if direct.exists() {
        return Some(direct);
    }

    let target_lower = target.to_lowercase();
    for entry in walkdir::WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == target_lower {
                return Some(entry.path().to_path_buf());
            }
        }
    }

    None
}

pub fn relative_path(path: &Path, vault_path: &Path) -> String {
    path.strip_prefix(vault_path)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
