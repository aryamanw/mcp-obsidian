use crate::parse::codeblocks;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// A GitHub-flavored markdown table. `start`/`end` are byte offsets of the
/// header line start and just past the last row (including its newline),
/// so `body[start..end]` can be spliced. Rows are padded/truncated to the
/// header's column count.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub start: usize,
    pub end: usize,
    pub headers: Vec<String>,
    pub alignments: Vec<Alignment>,
    pub rows: Vec<Vec<String>>,
    /// Text of the nearest heading above the table (e.g. "## Budget"), if any.
    pub heading: Option<String>,
}

/// Splits a table line into trimmed cells, honoring `\|` escapes (which is
/// how Obsidian tables write aliased wikilinks, `[[note\|alias]]`). Escapes
/// are kept as-is so a read → write round trip is lossless.
fn split_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = if inner.ends_with('|') && !inner.ends_with("\\|") {
        &inner[..inner.len() - 1]
    } else {
        inner
    };

    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                current.push_str("\\|");
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    cells.push(current.trim().to_string());
    cells
}

fn parse_delimiter_row(line: &str) -> Option<Vec<Alignment>> {
    if !line.contains('-') {
        return None;
    }
    split_cells(line).iter().map(|cell| {
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let dashes = cell.trim_matches(':');
        if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
            return None;
        }
        Some(match (left, right) {
            (true, true) => Alignment::Center,
            (true, false) => Alignment::Left,
            (false, true) => Alignment::Right,
            (false, false) => Alignment::None,
        })
    }).collect()
}

fn heading_text(line: &str) -> Option<&str> {
    let level = line.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then(|| line.trim())
}

/// Finds every markdown table in `body`, skipping anything inside fenced
/// code blocks.
pub fn extract_tables(body: &str) -> Vec<Table> {
    let code_ranges: Vec<(usize, usize)> = codeblocks::extract_code_blocks(body)
        .iter().map(|b| (b.start, b.end)).collect();
    let in_code = |offset: usize| code_ranges.iter().any(|(s, e)| offset >= *s && offset < *e);

    // (byte offset, line without newline, full line length)
    let mut lines: Vec<(usize, &str, usize)> = Vec::new();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        lines.push((offset, line.trim_end_matches(['\n', '\r']), line.len()));
        offset += line.len();
    }

    let mut tables = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let (start, line, _) = lines[i];
        if in_code(start) {
            i += 1;
            continue;
        }
        if let Some(h) = heading_text(line) {
            current_heading = Some(h.to_string());
        }

        let header_ok = line.contains('|') && i + 1 < lines.len() && !in_code(lines[i + 1].0);
        if header_ok {
            let headers = split_cells(line);
            if let Some(alignments) = parse_delimiter_row(lines[i + 1].1) {
                if alignments.len() == headers.len() {
                    let mut rows = Vec::new();
                    let mut j = i + 2;
                    while j < lines.len() && lines[j].1.contains('|') && !lines[j].1.trim().is_empty() && !in_code(lines[j].0) {
                        let mut cells = split_cells(lines[j].1);
                        cells.resize(headers.len(), String::new());
                        rows.push(cells);
                        j += 1;
                    }
                    let (last_off, _, last_len) = lines[j - 1];
                    tables.push(Table {
                        start,
                        end: last_off + last_len,
                        headers,
                        alignments,
                        rows,
                        heading: current_heading.clone(),
                    });
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    tables
}

/// Makes arbitrary text safe to put in a single table cell: unescaped pipes
/// would split the cell and newlines would end the row.
pub fn escape_cell(cell: &str) -> String {
    let mut out = String::new();
    let mut chars = cell.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                out.push_str("\\|");
                chars.next();
            }
            '|' => out.push_str("\\|"),
            '\r' => {}
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
    out
}

/// Renders a table the way the Advanced Tables plugin formats it: every
/// column padded to its widest cell (minimum 3), with alignment reflected
/// in both the delimiter row and the cell padding. Cells are escaped with
/// `escape_cell`; rows are padded/truncated to the header count.
pub fn format_table(headers: &[String], alignments: &[Alignment], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let align = |c: usize| alignments.get(c).copied().unwrap_or(Alignment::None);
    let headers: Vec<String> = headers.iter().map(|h| escape_cell(h)).collect();
    let rows: Vec<Vec<String>> = rows.iter().map(|r| {
        (0..cols).map(|c| r.get(c).map(|s| escape_cell(s)).unwrap_or_default()).collect()
    }).collect();

    let width = |c: usize| {
        std::iter::once(&headers[c]).chain(rows.iter().map(|r| &r[c]))
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0)
            .max(3)
    };
    let widths: Vec<usize> = (0..cols).map(width).collect();

    let pad = |text: &str, c: usize| {
        let gap = widths[c] - text.chars().count();
        match align(c) {
            Alignment::Right => format!("{}{}", " ".repeat(gap), text),
            Alignment::Center => format!("{}{}{}", " ".repeat(gap / 2), text, " ".repeat(gap - gap / 2)),
            _ => format!("{}{}", text, " ".repeat(gap)),
        }
    };
    let render_row = |cells: &[String]| {
        let inner: Vec<String> = cells.iter().enumerate().map(|(c, s)| pad(s, c)).collect();
        format!("| {} |\n", inner.join(" | "))
    };

    let mut out = render_row(&headers);
    let delims: Vec<String> = (0..cols).map(|c| {
        let w = widths[c];
        match align(c) {
            Alignment::None => "-".repeat(w),
            Alignment::Left => format!(":{}", "-".repeat(w - 1)),
            Alignment::Right => format!("{}:", "-".repeat(w - 1)),
            Alignment::Center => format!(":{}:", "-".repeat(w - 2)),
        }
    }).collect();
    out.push_str(&format!("| {} |\n", delims.join(" | ")));
    for row in &rows {
        out.push_str(&render_row(row));
    }
    out
}
