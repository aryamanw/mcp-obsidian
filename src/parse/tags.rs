use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

fn tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"#([\w/-]+)").unwrap())
}

fn fenced_code_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)```.*?```").unwrap())
}

fn inline_code_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"`[^`\n]+`").unwrap())
}

fn wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[[^\]]*\]\]").unwrap())
}

/// Extracts inline `#tag` occurrences from note body text, matching
/// Obsidian's own tag rules rather than a bare `#word` scan:
///
/// - Code (fenced ` ``` ` blocks and inline `` `spans` ``) is not scanned —
///   a shell comment or CSS hex color inside code isn't a tag.
/// - `[[wikilink#Heading]]` references are not scanned — the `#Heading`
///   part addresses a section, it isn't a tag.
/// - A `#` glued directly to a preceding word character (e.g. a URL
///   anchor like `page.html#section`) is not a tag; Obsidian requires a
///   tag to start at whitespace, punctuation, or the start of a line.
/// - A tag made up entirely of digits (e.g. `#2024`) is not valid;
///   Obsidian requires at least one non-numeric character.
pub fn extract_tags(content: &str) -> HashSet<String> {
    let sanitized = fenced_code_regex().replace_all(content, " ");
    let sanitized = inline_code_regex().replace_all(&sanitized, " ");
    let sanitized = wikilink_regex().replace_all(&sanitized, " ");

    let mut tags = HashSet::new();
    let re = tag_regex();
    for cap in re.captures_iter(&sanitized) {
        let whole = cap.get(0).unwrap();
        let preceded_by_word = sanitized[..whole.start()]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if preceded_by_word {
            continue;
        }

        let tag = cap[1].to_string();
        if tag.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        tags.insert(tag);
    }
    tags
}

fn protected_spans(body: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    spans.extend(fenced_code_regex().find_iter(body).map(|m| (m.start(), m.end())));
    spans.extend(inline_code_regex().find_iter(body).map(|m| (m.start(), m.end())));
    spans.extend(wikilink_regex().find_iter(body).map(|m| (m.start(), m.end())));
    spans
}

fn in_protected_span(spans: &[(usize, usize)], pos: usize) -> bool {
    spans.iter().any(|&(start, end)| pos >= start && pos < end)
}

/// Removes inline `#tag` occurrences matching (case-insensitively) any of
/// `remove_tags` from `body`, using the same tag-boundary rules as
/// `extract_tags` — so it won't touch text inside code spans/blocks or
/// `[[wikilink#Heading]]` references. Returns the (possibly unchanged)
/// body and whether any removal happened.
///
/// `bulk_tag` previously only ever edited the frontmatter `tags` field,
/// so removing a tag that a note only carried as an inline `#tag` in its
/// body was a silent no-op — the tag stayed fully visible in
/// `note.tags`. This restores symmetry: a removed tag is actually gone,
/// regardless of which form it was written in.
pub fn remove_inline_tags(body: &str, remove_tags: &[String]) -> (String, bool) {
    let targets: HashSet<String> = remove_tags.iter()
        .map(|t| t.trim().trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if targets.is_empty() {
        return (body.to_string(), false);
    }

    let spans = protected_spans(body);
    let re = tag_regex();
    let mut removals: Vec<(usize, usize)> = Vec::new();

    for cap in re.captures_iter(body) {
        let whole = cap.get(0).unwrap();
        if in_protected_span(&spans, whole.start()) {
            continue;
        }
        let preceded_by_word = body[..whole.start()]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if preceded_by_word {
            continue;
        }
        if targets.contains(&cap[1].to_lowercase()) {
            // Also consume one trailing space so removal doesn't leave a
            // double space behind.
            let mut end = whole.end();
            if body[end..].starts_with(' ') {
                end += 1;
            }
            removals.push((whole.start(), end));
        }
    }

    if removals.is_empty() {
        return (body.to_string(), false);
    }

    let mut result = String::with_capacity(body.len());
    let mut last = 0;
    for (start, end) in removals {
        result.push_str(&body[last..start]);
        last = end;
    }
    result.push_str(&body[last..]);
    (result, true)
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
