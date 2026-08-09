#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub start: usize,
    pub end: usize,
    pub level: usize,
}

#[derive(Debug, PartialEq)]
pub enum SectionError {
    NotFound,
    Ambiguous(usize),
}

/// Parses a heading argument like "## Tasks" into its level (2) and trimmed
/// text ("Tasks"). A heading with no leading '#' characters gets level 0,
/// which never matches a real markdown heading (levels are always 1-6) and
/// so always results in `SectionError::NotFound`.
fn parse_heading_prefix(heading: &str) -> (usize, String) {
    let trimmed = heading.trim_start();
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    let text = trimmed[level..].trim().to_string();
    (level, text)
}

/// Finds a markdown section (an ATX heading, `#` through `######`, plus
/// everything under it) inside `body` matching `heading` (e.g. "## Tasks").
/// A match requires both the heading level and text to match exactly. The
/// section runs from the heading line up to (but not including) the next
/// heading of equal or lower level, or the end of the document.
pub fn find_section(body: &str, heading: &str) -> Result<Section, SectionError> {
    let (level, text) = parse_heading_prefix(heading);

    // Collect every ATX heading line: (byte offset of line start, level, trimmed text).
    // A line only counts as a heading if a space follows the '#' run — this
    // is what distinguishes "## Tasks" from an inline tag like "#project".
    let mut headings: Vec<(usize, usize, String)> = Vec::new();
    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        let hlevel = content.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hlevel) && content[hlevel..].starts_with(' ') {
            headings.push((offset, hlevel, content[hlevel..].trim().to_string()));
        }
        offset += line.len();
    }

    let matches: Vec<usize> = headings.iter()
        .enumerate()
        .filter(|(_, (_, hlevel, htext))| *hlevel == level && *htext == text)
        .map(|(i, _)| i)
        .collect();

    match matches.len() {
        0 => Err(SectionError::NotFound),
        1 => {
            let idx = matches[0];
            let (start, hlevel, _) = headings[idx];
            let end = headings[idx + 1..].iter()
                .find(|(_, l, _)| *l <= hlevel)
                .map(|(s, _, _)| *s)
                .unwrap_or(body.len());
            Ok(Section { start, end, level: hlevel })
        }
        n => Err(SectionError::Ambiguous(n)),
    }
}
