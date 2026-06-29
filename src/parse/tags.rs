use regex::Regex;
use std::collections::HashSet;

pub fn extract_tags(content: &str) -> HashSet<String> {
    let mut tags = HashSet::new();
    let re = Regex::new(r"#([\w/-]+)").unwrap();
    for cap in re.captures_iter(content) {
        let tag = cap[1].to_string();
        tags.insert(tag);
    }
    tags
}

pub fn extract_tags_from_frontmatter(frontmatter: &std::collections::HashMap<String, String>) -> HashSet<String> {
    let mut tags = HashSet::new();
    for key in ["tags", "tag"] {
        if let Some(tag_str) = frontmatter.get(key) {
            for tag in tag_str.split(',') {
                let tag = tag.trim().trim_start_matches('#').to_string();
                if !tag.is_empty() {
                    tags.insert(tag);
                }
            }
        }
    }
    tags
}

pub fn all_tags(content: &str, frontmatter: &std::collections::HashMap<String, String>) -> HashSet<String> {
    let mut tags = extract_tags(content);
    tags.extend(extract_tags_from_frontmatter(frontmatter));
    tags
}
