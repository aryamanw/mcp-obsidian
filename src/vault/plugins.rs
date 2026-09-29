//! Vault operations for community-plugin file formats: Excalidraw drawings,
//! Advanced Tables (markdown tables), Kanban boards, Mermaid and Charts code
//! blocks, and (Advanced) Canvas files. This is a child module of `vault`
//! so every path still goes through `Vault::validate_path`/`validate_parent`.

use super::Vault;
use crate::parse::codeblocks::{self, CodeBlock};
use crate::parse::excalidraw::{self, ElementSpec};
use crate::parse::canvas::{self, EdgeSpec, EditSummary, NodeSpec};
use crate::parse::kanban::{self, Board, Card, Lane};
use crate::parse::tables::{self, Alignment, Table};
use crate::parse::{frontmatter, sections, wikilink};
use serde_json::Value;
use std::path::PathBuf;

pub struct DrawingInfo {
    pub path: String,
    pub compressed: bool,
    pub text_elements: Vec<(String, String)>,
    pub embedded_files: Vec<(String, String)>,
    pub element_links: Vec<(String, String)>,
    pub scene: Value,
}

pub struct NewLane {
    pub title: String,
    pub complete: bool,
    pub cards: Vec<String>,
}

#[derive(Default)]
pub struct CardUpdate<'a> {
    pub to_lane: Option<&'a str>,
    pub new_text: Option<&'a str>,
    pub checked: Option<bool>,
    pub archive: bool,
}

/// Inserts `block` (which ends in '\n') at the end of the section under
/// `heading` (creating the section at the end of the note if missing), or at
/// the end of the note when `heading` is `None`. Returns the new body and
/// the byte offset the block was inserted at.
fn insert_block(body: &str, block: &str, heading: Option<&str>) -> anyhow::Result<(String, usize)> {
    let append_to_end = |prefix: Option<&str>| {
        let mut out = body.trim_end().to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        if let Some(h) = prefix {
            out.push_str(h.trim());
            out.push_str("\n\n");
        }
        let at = out.len();
        out.push_str(block);
        (out, at)
    };

    let Some(heading) = heading else { return Ok(append_to_end(None)) };
    if !heading.trim_start().starts_with('#') {
        return Err(anyhow::anyhow!("Heading must include '#' markers (e.g. '## Diagrams')"));
    }
    match sections::find_section(body, heading) {
        Ok(section) => {
            let mut out = format!("{}\n\n", body[..section.end].trim_end());
            let at = out.len();
            out.push_str(block);
            let rest = &body[section.end..];
            if !rest.is_empty() {
                out.push('\n');
                out.push_str(rest);
            }
            Ok((out, at))
        }
        Err(sections::SectionError::NotFound) => Ok(append_to_end(Some(heading))),
        Err(sections::SectionError::Ambiguous(n)) => Err(anyhow::anyhow!(
            "Heading '{}' matches {} sections; ambiguous", heading, n
        )),
    }
}

fn lane_index(board: &Board, title: &str) -> anyhow::Result<usize> {
    board.lanes.iter().position(|l| l.title.eq_ignore_ascii_case(title.trim())).ok_or_else(|| {
        let titles: Vec<&str> = board.lanes.iter().map(|l| l.title.as_str()).collect();
        anyhow::anyhow!("Lane '{}' not found (lanes: {})", title, titles.join(", "))
    })
}

/// Finds a card by exact text, falling back to a unique case-insensitive
/// substring match. Returns (lane index, card index).
fn find_card(board: &Board, query: &str, lane: Option<usize>) -> anyhow::Result<(usize, usize)> {
    let query = query.trim();
    let candidates: Vec<(usize, usize, &Card)> = board.lanes.iter().enumerate()
        .filter(|(li, _)| lane.is_none_or(|l| l == *li))
        .flat_map(|(li, l)| l.cards.iter().enumerate().map(move |(ci, c)| (li, ci, c)))
        .collect();

    let exact: Vec<_> = candidates.iter().filter(|(_, _, c)| c.text.trim() == query).collect();
    if let [(li, ci, _)] = exact.as_slice() {
        return Ok((*li, *ci));
    }
    let query_lower = query.to_lowercase();
    let fuzzy: Vec<_> = if exact.is_empty() {
        candidates.iter().filter(|(_, _, c)| c.text.to_lowercase().contains(&query_lower)).collect()
    } else {
        exact
    };
    match fuzzy.as_slice() {
        [] => Err(anyhow::anyhow!("No card matching '{}'", query)),
        [(li, ci, _)] => Ok((*li, *ci)),
        many => {
            let list: Vec<String> = many.iter()
                .map(|(li, _, c)| format!("'{}' in '{}'", c.text, board.lanes[*li].title)).collect();
            Err(anyhow::anyhow!("'{}' matches {} cards; be more specific: {}", query, many.len(), list.join("; ")))
        }
    }
}

impl Vault {
    fn rel(&self, full: &std::path::Path) -> String {
        wikilink::relative_path(full, &self.config.vault_path)
    }

    /// Resolves an existing file, trying `path` as given and then with
    /// each of `suffixes` appended.
    fn resolve_with_suffixes(&self, path: &str, suffixes: &[&str]) -> anyhow::Result<PathBuf> {
        for candidate in std::iter::once(path.to_string()).chain(suffixes.iter().map(|s| format!("{}{}", path, s))) {
            let full = self.validate_path(&candidate)?;
            if full.is_file() {
                return Ok(full);
            }
        }
        Err(anyhow::anyhow!("File not found: {}", path))
    }

    /// Validates a path for a file that must not exist yet, creating its
    /// parent folders.
    fn new_file(&self, path: &str) -> anyhow::Result<PathBuf> {
        let full = self.validate_parent(path)?;
        if full.exists() {
            return Err(anyhow::anyhow!("File already exists: {}", path));
        }
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(full)
    }

    /// Reads a note and rewrites its body with `edit`, leaving the raw
    /// frontmatter block untouched.
    fn edit_body<T>(&self, note_path: &str, edit: impl FnOnce(&str) -> anyhow::Result<(String, T)>) -> anyhow::Result<(String, T)> {
        let full = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full)?;
        let (fm, body) = frontmatter::split_raw(&content);
        let (new_body, result) = edit(body)?;
        std::fs::write(&full, format!("{}{}", fm, new_body))?;
        Ok((self.rel(&full), result))
    }

    // ===== Code blocks (Mermaid, Charts) =====

    pub fn list_code_blocks(&self, note_path: &str, language: Option<&str>) -> anyhow::Result<Vec<CodeBlock>> {
        let note = self.read_note(note_path)?;
        Ok(codeblocks::extract_code_blocks(&note.body).into_iter()
            .filter(|b| language.is_none_or(|l| b.language.eq_ignore_ascii_case(l)))
            .collect())
    }

    /// Replaces the `index`-th (0-based) ```language block, or inserts a new
    /// one (under `heading`, or at the end). Returns the note path and the
    /// written block's index among blocks of that language.
    pub fn write_code_block(
        &self,
        note_path: &str,
        language: &str,
        content: &str,
        index: Option<usize>,
        heading: Option<&str>,
    ) -> anyhow::Result<(String, usize)> {
        let block = codeblocks::render_code_block(language, content);
        self.edit_body(note_path, |body| {
            let same_lang = |b: &CodeBlock| b.language.eq_ignore_ascii_case(language);
            let existing: Vec<CodeBlock> = codeblocks::extract_code_blocks(body).into_iter().filter(same_lang).collect();
            match index {
                Some(i) => {
                    let target = existing.get(i).ok_or_else(|| anyhow::anyhow!(
                        "No {} block at index {} (note has {})", language, i, existing.len()
                    ))?;
                    Ok((format!("{}{}{}", &body[..target.start], block, &body[target.end..]), i))
                }
                None => {
                    let (new_body, at) = insert_block(body, &block, heading)?;
                    let i = codeblocks::extract_code_blocks(&new_body).into_iter()
                        .filter(same_lang)
                        .position(|b| b.start == at)
                        .unwrap_or(existing.len());
                    Ok((new_body, i))
                }
            }
        })
    }

    // ===== Tables (Advanced Tables) =====

    pub fn read_tables(&self, note_path: &str) -> anyhow::Result<Vec<Table>> {
        let full = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full)?;
        Ok(tables::extract_tables(frontmatter::split_raw(&content).1))
    }

    /// Replaces the `index`-th table (keeping its alignments unless new ones
    /// are given) or inserts a new one under `heading`/at the end. Returns
    /// the note path and the table's index.
    pub fn write_table(
        &self,
        note_path: &str,
        headers: &[String],
        rows: &[Vec<String>],
        alignments: Option<&[Alignment]>,
        index: Option<usize>,
        heading: Option<&str>,
    ) -> anyhow::Result<(String, usize)> {
        if headers.is_empty() {
            return Err(anyhow::anyhow!("Table needs at least one header column"));
        }
        self.edit_body(note_path, |body| {
            let existing = tables::extract_tables(body);
            match index {
                Some(i) => {
                    let target = existing.get(i).ok_or_else(|| anyhow::anyhow!(
                        "No table at index {} (note has {})", i, existing.len()
                    ))?;
                    let aligns = alignments.map(<[Alignment]>::to_vec).unwrap_or_else(|| target.alignments.clone());
                    let rendered = tables::format_table(headers, &aligns, rows);
                    Ok((format!("{}{}{}", &body[..target.start], rendered, &body[target.end..]), i))
                }
                None => {
                    let rendered = tables::format_table(headers, alignments.unwrap_or(&[]), rows);
                    let (new_body, at) = insert_block(body, &rendered, heading)?;
                    let i = tables::extract_tables(&new_body).iter().position(|t| t.start == at).unwrap_or(existing.len());
                    Ok((new_body, i))
                }
            }
        })
    }

    /// Appends rows to the `index`-th table and reformats it. Returns the
    /// note path and the table's new row count.
    pub fn add_table_rows(&self, note_path: &str, index: usize, new_rows: &[Vec<String>]) -> anyhow::Result<(String, usize)> {
        self.edit_body(note_path, |body| {
            let existing = tables::extract_tables(body);
            let target = existing.get(index).ok_or_else(|| anyhow::anyhow!(
                "No table at index {} (note has {})", index, existing.len()
            ))?;
            let mut rows = target.rows.clone();
            rows.extend(new_rows.iter().cloned());
            let rendered = tables::format_table(&target.headers, &target.alignments, &rows);
            Ok((format!("{}{}{}", &body[..target.start], rendered, &body[target.end..]), rows.len()))
        })
    }

    // ===== Kanban =====

    fn load_board(&self, note_path: &str) -> anyhow::Result<(PathBuf, String, Board)> {
        let full = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full)?;
        if !frontmatter::parse(&content).frontmatter.contains_key("kanban-plugin") {
            return Err(anyhow::anyhow!("'{}' is not a Kanban board (no 'kanban-plugin' frontmatter)", note_path));
        }
        let (fm, body) = frontmatter::split_raw(&content);
        Ok((full, fm.to_string(), kanban::parse(body)))
    }

    fn save_board(&self, full: &std::path::Path, fm: &str, board: &Board) -> anyhow::Result<()> {
        let fm = if fm.ends_with("\n\n") { fm.to_string() } else { format!("{}\n", fm) };
        std::fs::write(full, format!("{}{}", fm, kanban::render(board)))?;
        Ok(())
    }

    pub fn read_kanban(&self, note_path: &str) -> anyhow::Result<(String, Board)> {
        let (full, _, board) = self.load_board(note_path)?;
        Ok((self.rel(&full), board))
    }

    pub fn create_kanban(&self, note_path: &str, lanes: &[NewLane]) -> anyhow::Result<(String, Board)> {
        if lanes.is_empty() {
            return Err(anyhow::anyhow!("A board needs at least one lane"));
        }
        let path = if note_path.ends_with(".md") { note_path.to_string() } else { format!("{}.md", note_path) };
        let full = self.new_file(&path)?;
        let board = Board {
            preamble: String::new(),
            lanes: lanes.iter().map(|l| Lane {
                title: l.title.trim().to_string(),
                complete: l.complete,
                cards: l.cards.iter().map(|c| Card { text: c.trim().to_string(), checked: l.complete }).collect(),
            }).collect(),
            archive: None,
            settings: None,
        };
        self.save_board(&full, kanban::NEW_BOARD_FRONTMATTER, &board)?;
        Ok((self.rel(&full), board))
    }

    pub fn add_kanban_card(&self, note_path: &str, lane: &str, text: &str, checked: Option<bool>, at_top: bool) -> anyhow::Result<(String, Board)> {
        if text.trim().is_empty() {
            return Err(anyhow::anyhow!("Card text must not be empty"));
        }
        let (full, fm, mut board) = self.load_board(note_path)?;
        let li = lane_index(&board, lane)?;
        let lane = &mut board.lanes[li];
        let card = Card { text: text.trim().to_string(), checked: checked.unwrap_or(lane.complete) };
        if at_top { lane.cards.insert(0, card) } else { lane.cards.push(card) }
        self.save_board(&full, &fm, &board)?;
        Ok((self.rel(&full), board))
    }

    /// Moves, edits, checks/unchecks, or archives a card. Moving a card into
    /// or out of a "complete" lane checks/unchecks it (as the plugin does)
    /// unless `checked` is given explicitly.
    pub fn update_kanban_card(&self, note_path: &str, card: &str, lane: Option<&str>, update: CardUpdate) -> anyhow::Result<(String, Board)> {
        if update.to_lane.is_none() && update.new_text.is_none() && update.checked.is_none() && !update.archive {
            return Err(anyhow::anyhow!("Nothing to update: give to_lane, new_text, checked, or archive"));
        }
        if update.archive && update.to_lane.is_some() {
            return Err(anyhow::anyhow!("Use either archive or to_lane, not both"));
        }
        let (full, fm, mut board) = self.load_board(note_path)?;
        let source_lane = lane.map(|l| lane_index(&board, l)).transpose()?;
        let target_lane = update.to_lane.map(|l| lane_index(&board, l)).transpose()?;
        let (li, ci) = find_card(&board, card, source_lane)?;

        let mut moved = board.lanes[li].cards.remove(ci);
        if let Some(text) = update.new_text {
            if text.trim().is_empty() {
                return Err(anyhow::anyhow!("Card text must not be empty"));
            }
            moved.text = text.trim().to_string();
        }
        if let Some(t) = target_lane.filter(|t| *t != li) {
            if update.checked.is_none() && board.lanes[t].complete != board.lanes[li].complete {
                moved.checked = board.lanes[t].complete;
            }
        }
        if let Some(checked) = update.checked {
            moved.checked = checked;
        }

        if update.archive {
            board.archive.get_or_insert_with(|| Lane { title: "Archive".to_string(), complete: false, cards: Vec::new() })
                .cards.push(moved);
        } else {
            match target_lane {
                Some(t) if t != li => board.lanes[t].cards.push(moved),
                _ => board.lanes[li].cards.insert(ci, moved),
            }
        }
        self.save_board(&full, &fm, &board)?;
        Ok((self.rel(&full), board))
    }

    // ===== Excalidraw =====

    pub fn read_drawing(&self, path: &str) -> anyhow::Result<DrawingInfo> {
        let full = self.resolve_with_suffixes(path, &[".excalidraw.md", ".md", ".excalidraw"])?;
        let content = std::fs::read_to_string(&full)?;
        if full.extension().is_some_and(|e| e == "excalidraw") {
            let scene = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Invalid Excalidraw JSON: {}", e))?;
            return Ok(DrawingInfo {
                path: self.rel(&full),
                compressed: false,
                text_elements: Vec::new(),
                embedded_files: Vec::new(),
                element_links: Vec::new(),
                scene,
            });
        }
        let body = frontmatter::split_raw(&content).1;
        let block = excalidraw::find_scene(body)?;
        Ok(DrawingInfo {
            path: self.rel(&full),
            compressed: block.compressed,
            text_elements: excalidraw::parse_text_elements(body),
            embedded_files: excalidraw::parse_id_section(body, "Embedded Files"),
            element_links: excalidraw::parse_id_section(body, "Element Links"),
            scene: block.scene,
        })
    }

    pub fn create_drawing(&self, path: &str, specs: &[ElementSpec]) -> anyhow::Result<(String, usize)> {
        let base = path.strip_suffix(".excalidraw.md")
            .or_else(|| path.strip_suffix(".md"))
            .or_else(|| path.strip_suffix(".excalidraw"))
            .unwrap_or(path);
        let mut elements = Vec::new();
        let texts = excalidraw::add_elements(&mut elements, specs)?;
        let full = self.new_file(&format!("{}.excalidraw.md", base))?;
        let count = elements.len();
        let body = excalidraw::append_text_elements("## Text Elements\n", &texts);
        let content = format!(
            "{}{}{}\n",
            excalidraw::NEW_DRAWING_HEADER,
            body,
            excalidraw::render_drawing_section(&excalidraw::new_scene(elements)),
        );
        std::fs::write(&full, content)?;
        Ok((self.rel(&full), count))
    }

    /// Adds elements to an existing drawing. Returns the path and the ids
    /// of every element created (including auto-generated labels). The scene
    /// is rewritten as uncompressed JSON.
    pub fn add_drawing_elements(&self, path: &str, specs: &[ElementSpec]) -> anyhow::Result<(String, Vec<String>)> {
        let full = self.resolve_with_suffixes(path, &[".excalidraw.md", ".md", ".excalidraw"])?;
        let content = std::fs::read_to_string(&full)?;
        let is_json_file = full.extension().is_some_and(|e| e == "excalidraw");

        let (mut scene, fm, body, block_range) = if is_json_file {
            let scene: Value = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Invalid Excalidraw JSON: {}", e))?;
            (scene, "", "", (0, 0))
        } else {
            let (fm, body) = frontmatter::split_raw(&content);
            let block = excalidraw::find_scene(body)?;
            (block.scene, fm, body, (block.start, block.end))
        };

        let mut elements = match scene.get_mut("elements").map(Value::take) {
            Some(Value::Array(a)) => a,
            _ => Vec::new(),
        };
        let before = elements.len();
        let texts = excalidraw::add_elements(&mut elements, specs)?;
        let new_ids: Vec<String> = elements[before..].iter()
            .filter_map(|e| e["id"].as_str().map(str::to_string)).collect();
        scene["elements"] = Value::Array(elements);

        let new_content = if is_json_file {
            serde_json::to_string_pretty(&scene)?
        } else {
            let replaced = format!(
                "{}{}{}",
                &body[..block_range.0],
                excalidraw::render_scene_block(&scene),
                &body[block_range.1..],
            );
            format!("{}{}", fm, excalidraw::append_text_elements(&replaced, &texts))
        };
        std::fs::write(&full, new_content)?;
        Ok((self.rel(&full), new_ids))
    }

    // ===== Canvas (JSON Canvas / Advanced Canvas) =====

    pub fn read_canvas(&self, path: &str) -> anyhow::Result<(String, Value)> {
        let full = self.resolve_with_suffixes(path, &[".canvas"])?;
        let content = std::fs::read_to_string(&full)?;
        let doc = if content.trim().is_empty() {
            canvas::empty_canvas()
        } else {
            serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("Invalid canvas JSON: {}", e))?
        };
        Ok((self.rel(&full), doc))
    }

    pub fn create_canvas(&self, path: &str, nodes: &[NodeSpec], edges: &[EdgeSpec]) -> anyhow::Result<(String, EditSummary)> {
        let path = if path.ends_with(".canvas") { path.to_string() } else { format!("{}.canvas", path) };
        let mut doc = canvas::empty_canvas();
        let summary = canvas::apply_edits(&mut doc, nodes, edges, &[])?;
        let full = self.new_file(&path)?;
        std::fs::write(&full, canvas::to_string(&doc))?;
        Ok((self.rel(&full), summary))
    }

    pub fn edit_canvas(&self, path: &str, nodes: &[NodeSpec], edges: &[EdgeSpec], remove_ids: &[String]) -> anyhow::Result<(String, EditSummary)> {
        let (rel, mut doc) = self.read_canvas(path)?;
        let summary = canvas::apply_edits(&mut doc, nodes, edges, remove_ids)?;
        let full = self.validate_path(&rel)?;
        std::fs::write(&full, canvas::to_string(&doc))?;
        Ok((rel, summary))
    }
}
