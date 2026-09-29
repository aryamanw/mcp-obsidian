//! Parsing and serialization of Obsidian Kanban plugin boards. A board is a
//! markdown note with `kanban-plugin: board` frontmatter whose body is:
//!
//! ```text
//! ## Lane title
//!
//! **Complete**          <- optional; marks the lane as a "done" lane
//! - [ ] Card text
//!     continuation line   <- multi-line cards are indented
//!
//! ***                   <- separates the archive from the lanes
//!
//! ## Archive
//!
//! - [x] Archived card
//!
//! %% kanban:settings
//! ```{"kanban-plugin":"board"}```
//! %%
//! ```

use serde::Serialize;

pub const SETTINGS_MARKER: &str = "%% kanban:settings";
const DEFAULT_SETTINGS: &str = "%% kanban:settings\n```\n{\"kanban-plugin\":\"board\"}\n```\n%%\n";
pub const NEW_BOARD_FRONTMATTER: &str = "---\n\nkanban-plugin: board\n\n---\n\n";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    /// Card text; multi-line cards have their continuation lines joined
    /// with '\n' and their list indentation removed.
    pub text: String,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lane {
    pub title: String,
    /// Whether the lane is a "complete" lane (cards moved here are checked).
    pub complete: bool,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// Any text before the first lane heading, preserved verbatim.
    pub preamble: String,
    pub lanes: Vec<Lane>,
    pub archive: Option<Lane>,
    /// The raw `%% kanban:settings ... %%` block, preserved verbatim.
    pub settings: Option<String>,
}

fn parse_card_line(line: &str) -> Option<Card> {
    let rest = line.strip_prefix("- [")?;
    let mut chars = rest.chars();
    let mark = chars.next()?;
    let rest = chars.as_str().strip_prefix(']')?;
    let text = rest.strip_prefix(' ').unwrap_or(rest);
    Some(Card { text: text.to_string(), checked: mark != ' ' })
}

/// Strips one level of list-continuation indentation (a tab, or up to 4 spaces).
fn dedent(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix('\t') {
        return rest;
    }
    let spaces = line.len() - line.trim_start_matches(' ').len();
    &line[spaces.min(4)..]
}

/// Parses a board body (the note content after its frontmatter).
pub fn parse(body: &str) -> Board {
    let (content, settings) = match body.find(SETTINGS_MARKER) {
        Some(idx) => (&body[..idx], Some(body[idx..].to_string())),
        None => (body, None),
    };

    let mut board = Board { preamble: String::new(), lanes: Vec::new(), archive: None, settings };
    // Every lane after the `***` separator belongs to the archive.
    let mut after_separator = false;
    let mut current: Option<(Lane, bool)> = None;

    let flush = |current: Option<(Lane, bool)>, board: &mut Board| match current {
        Some((lane, true)) => board.archive = Some(lane),
        Some((lane, false)) => board.lanes.push(lane),
        None => {}
    };

    for line in content.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            flush(current.take(), &mut board);
            let lane = Lane { title: title.trim().to_string(), complete: false, cards: Vec::new() };
            current = Some((lane, after_separator));
            continue;
        }
        if line.trim() == "***" {
            flush(current.take(), &mut board);
            after_separator = true;
            continue;
        }

        match current.as_mut() {
            None if !after_separator => {
                board.preamble.push_str(line);
                board.preamble.push('\n');
            }
            None => {}
            Some((lane, _)) => {
                if line.trim() == "**Complete**" {
                    lane.complete = true;
                } else if let Some(card) = parse_card_line(line) {
                    lane.cards.push(card);
                } else if !line.trim().is_empty() {
                    if let Some(card) = lane.cards.last_mut() {
                        card.text.push('\n');
                        card.text.push_str(dedent(line));
                    }
                }
            }
        }
    }

    flush(current.take(), &mut board);
    board.preamble = board.preamble.trim().to_string();
    board
}

fn render_lane(lane: &Lane, out: &mut String) {
    out.push_str(&format!("## {}\n\n", lane.title));
    if lane.complete {
        out.push_str("**Complete**\n");
    }
    for card in &lane.cards {
        let text = card.text.trim().replace('\n', "\n\t");
        out.push_str(&format!("- [{}] {}\n", if card.checked { 'x' } else { ' ' }, text));
    }
    out.push_str("\n\n");
}

/// Serializes a board back to a note body (without frontmatter), in the
/// same layout the Kanban plugin itself writes.
pub fn render(board: &Board) -> String {
    let mut out = String::new();
    if !board.preamble.is_empty() {
        out.push_str(&board.preamble);
        out.push_str("\n\n");
    }
    for lane in &board.lanes {
        render_lane(lane, &mut out);
    }
    if let Some(archive) = &board.archive {
        out.push_str("***\n\n");
        render_lane(archive, &mut out);
    }
    out.push_str(board.settings.as_deref().unwrap_or(DEFAULT_SETTINGS));
    out
}
