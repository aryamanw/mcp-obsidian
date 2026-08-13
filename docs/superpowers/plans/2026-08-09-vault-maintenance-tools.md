# Vault Maintenance & Editing Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add seven new MCP tools to the Obsidian vault server — `list_tags`, `find_broken_links`, `find_orphan_notes`, `list_recent_notes`, `get_section`, `update_section`, `trash_note` — filling the gaps identified in [docs/superpowers/specs/2026-08-09-vault-maintenance-tools-design.md](../specs/2026-08-09-vault-maintenance-tools-design.md).

**Architecture:** Every tool follows the codebase's existing three-layer shape: a method on `Vault` (or a new pure-parsing module under `src/parse/`) that does the real work, a request struct in the matching `src/tools/*.rs` file, and a thin `#[tool]`-annotated adapter method in `src/main.rs` that deserializes the request, calls the `Vault` method, and serializes an ad-hoc `serde_json::Value` response. Section addressing (`get_section`/`update_section`) is built on one new shared primitive, `parse::sections::find_section`, added first since two later tasks depend on it.

**Tech Stack:** Rust, `rmcp` (MCP server macros), `walkdir`, `anyhow`, `chrono`. No new dependencies.

## Global Constraints

- Every new `Vault` method returns `anyhow::Result<T>`, matching all existing methods.
- Every new `#[tool]` method in `src/main.rs` follows the existing `match result { Ok(v) => CallToolResult::success(...), Err(e) => CallToolResult::error(...) }` shape. No new error-handling convention.
- `trash_note`, `get_section`, and `update_section` reuse the existing `resolve_note_path` / `validate_parent` path-validation helpers. No new path-safety mechanism is introduced.
- No shared link-index cache: each new vault-scanning method does its own independent `WalkDir` pass, matching every existing scanning method (`search_notes`, `search_by_tag`, `backlinks`, etc.).
- `trash_note` moves files to a vault-local `.trash/` folder only. No system-trash (macOS Trash / Recycle Bin) integration.
- `list_recent_notes` uses filesystem modification time only (`Metadata::modified()`). No creation-time metadata is exposed anywhere.
- New pure-parsing logic (no filesystem access) lives in `src/parse/*.rs`, following the style of `wikilink.rs` and `tags.rs`.

---

### Task 1: `sections` parsing module

**Files:**
- Create: `src/parse/sections.rs`
- Modify: `src/parse/mod.rs` (add `pub mod sections;`)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Produces: `pub struct Section { pub start: usize, pub end: usize, pub level: usize }`; `pub enum SectionError { NotFound, Ambiguous(usize) }` (derives `Debug, PartialEq`); `pub fn find_section(body: &str, heading: &str) -> Result<Section, SectionError>`. Tasks 6 and 7 call this directly as `sections::find_section`.

- [ ] **Step 1: Write the failing tests**

Append to `tests/integration.rs` (after the final existing test, `test_link_related_notes_finds_less_common_keyword`):

```rust
#[test]
fn test_find_section_basic() {
    let body = "# Title\n\n## Tasks\n\n- one\n- two\n\n## Notes\n\nSome notes.\n";
    let section = obsidian_mcp::parse::sections::find_section(body, "## Tasks").unwrap();
    let text = &body[section.start..section.end];
    assert!(text.contains("- one"));
    assert!(text.contains("- two"));
    assert!(!text.contains("Some notes"));
}

#[test]
fn test_find_section_not_found() {
    let body = "# Title\n\n## Tasks\n\nContent.\n";
    let err = obsidian_mcp::parse::sections::find_section(body, "## Nonexistent").unwrap_err();
    assert_eq!(err, obsidian_mcp::parse::sections::SectionError::NotFound);
}

#[test]
fn test_find_section_ambiguous() {
    let body = "## Notes\n\nFirst.\n\n## Other\n\nMiddle.\n\n## Notes\n\nSecond.\n";
    let err = obsidian_mcp::parse::sections::find_section(body, "## Notes").unwrap_err();
    assert_eq!(err, obsidian_mcp::parse::sections::SectionError::Ambiguous(2));
}

#[test]
fn test_find_section_nested_subheadings_included() {
    let body = "## Tasks\n\n### Subtask A\n\nDetail.\n\n## Notes\n\nOther.\n";
    let section = obsidian_mcp::parse::sections::find_section(body, "## Tasks").unwrap();
    let text = &body[section.start..section.end];
    assert!(text.contains("### Subtask A"));
    assert!(text.contains("Detail."));
    assert!(!text.contains("## Notes"));
}

#[test]
fn test_find_section_end_of_document() {
    let body = "# Title\n\n## Tasks\n\nOnly section, runs to EOF.\n";
    let section = obsidian_mcp::parse::sections::find_section(body, "## Tasks").unwrap();
    assert_eq!(section.end, body.len());
}

#[test]
fn test_find_section_ignores_hashtag_without_space() {
    // A line starting with '#project' (no space) is an inline Obsidian tag,
    // not a heading, and must not be mistaken for a level-1 heading with
    // text "project".
    let body = "#project\n\n## Tasks\n\nContent.\n";
    let err = obsidian_mcp::parse::sections::find_section(body, "# project").unwrap_err();
    assert_eq!(err, obsidian_mcp::parse::sections::SectionError::NotFound);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration test_find_section -- --test-threads=1`
Expected: FAIL to compile — `obsidian_mcp::parse::sections` doesn't exist yet.

- [ ] **Step 3: Create the `sections` module**

Create `src/parse/sections.rs`:

```rust
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
```

Modify `src/parse/mod.rs` — it currently reads:

```rust
pub mod frontmatter;
pub mod wikilink;
pub mod tags;
```

Change to:

```rust
pub mod frontmatter;
pub mod wikilink;
pub mod tags;
pub mod sections;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test integration test_find_section -- --test-threads=1`
Expected: PASS (6 tests: `test_find_section_basic`, `test_find_section_not_found`, `test_find_section_ambiguous`, `test_find_section_nested_subheadings_included`, `test_find_section_end_of_document`, `test_find_section_ignores_hashtag_without_space`)

- [ ] **Step 5: Commit**

```bash
git add src/parse/sections.rs src/parse/mod.rs tests/integration.rs
git commit -m "feat: add section-boundary parsing for get_section/update_section

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 2: `list_tags` tool

**Files:**
- Modify: `src/vault.rs` (add `Vault::list_tags`, right before `pub fn backlinks`)
- Modify: `src/tools/search.rs` (add `ListTagsRequest`)
- Modify: `src/main.rs` (add `list_tags` tool method, right before the `// ===== Write Tools =====` comment)
- Modify: `README.md` (Search table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Consumes: `tags::all_tags(content: &str, frontmatter: &HashMap<String, String>) -> HashSet<String>` (existing, from `src/parse/tags.rs`).
- Produces: `Vault::list_tags(&self) -> anyhow::Result<Vec<(String, usize)>>`, sorted by count descending then tag name ascending. Tags are aggregated by their exact stored form (case-sensitive) — not lowercased the way `search_by_tag` normalizes for matching.

- [ ] **Step 1: Write the failing test**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_list_tags() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let tags = vault.list_tags().unwrap();

    let project = tags.iter().find(|(t, _)| t == "project")
        .expect("expected 'project' tag to be present");
    assert!(project.1 >= 1);

    let test_tag = tags.iter().find(|(t, _)| t == "test")
        .expect("expected 'test' tag to be present");
    assert!(test_tag.1 >= 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration test_vault_list_tags`
Expected: FAIL to compile — `Vault::list_tags` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::list_tags`**

In `src/vault.rs`, find this exact line (the start of the `backlinks` method):

```rust
    pub fn backlinks(&self, note_path: &str) -> anyhow::Result<Vec<String>> {
```

Insert immediately before it:

```rust
    pub fn list_tags(&self) -> anyhow::Result<Vec<(String, usize)>> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                for tag in tags::all_tags(&content, &parsed.frontmatter) {
                    *counts.entry(tag).or_insert(0) += 1;
                }
            }
        }

        // Sorted by count descending, then alphabetically for ties — same
        // tie-break convention as extract_significant_words below.
        let mut result: Vec<(String, usize)> = counts.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(result)
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/search.rs`:

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTagsRequest {}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line:

```rust
    // ===== Write Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "List all tags used across the vault, each with a usage count (number of notes containing it).")]
    fn list_tags(
        &self,
        Parameters(_req): Parameters<tools::search::ListTagsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_tags() {
            Ok(tags) => {
                let results: Vec<serde_json::Value> = tags.iter().map(|(tag, count)| {
                    serde_json::json!({ "tag": tag, "count": count })
                }).collect();
                let result = serde_json::json!({
                    "tags": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --test integration test_vault_list_tags`
Expected: PASS

- [ ] **Step 7: Update README**

In `README.md`, find:

```
| `search_by_frontmatter` | Filter notes by frontmatter key-value pairs |
```

Insert immediately after it:

```
| `list_tags` | List all tags used in the vault, each with a usage count |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/search.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add list_tags tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 3: `find_broken_links` tool

**Files:**
- Modify: `src/vault.rs` (add `BrokenLink` struct near `NoteInfo`; add `Vault::find_broken_links`, right before `pub fn rename_note`)
- Modify: `src/tools/links.rs` (add `FindBrokenLinksRequest`)
- Modify: `src/main.rs` (add `find_broken_links` tool method, right before the `// ===== Template Tools =====` comment)
- Modify: `README.md` (Links table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Consumes: `wikilink::extract_wikilinks(content: &str) -> Vec<Wikilink>` and `wikilink::resolve_wikilink(target: &str, vault_path: &Path) -> Option<PathBuf>` (existing, from `src/parse/wikilink.rs`).
- Produces: `pub struct BrokenLink { pub source: String, pub target: String }` (derives `Debug, Clone`); `Vault::find_broken_links(&self) -> anyhow::Result<Vec<BrokenLink>>`.

- [ ] **Step 1: Write the failing test**

Append to `tests/integration.rs`. This relies on `tests/fixtures/test-vault/note3.md`, which already contains `Links to [[note1]] and [[nonexistent]].` — no new fixture needed:

```rust
#[test]
fn test_vault_find_broken_links() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let broken = vault.find_broken_links().unwrap();

    assert!(broken.iter().any(|b| b.source.contains("note3") && b.target == "nonexistent"));
    assert!(!broken.iter().any(|b| b.target == "note1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration test_vault_find_broken_links`
Expected: FAIL to compile — `Vault::find_broken_links` doesn't exist yet.

- [ ] **Step 3: Implement `BrokenLink` and `Vault::find_broken_links`**

In `src/vault.rs`, find this exact block:

```rust
pub struct Vault {
    pub config: Config,
}
```

Insert immediately before it:

```rust
#[derive(Debug, Clone)]
pub struct BrokenLink {
    pub source: String,
    pub target: String,
}

```

Then find this exact line (the start of the `rename_note` method):

```rust
    pub fn rename_note(&self, source: &str, dest: &str) -> anyhow::Result<NoteInfo> {
```

Insert immediately before it:

```rust
    pub fn find_broken_links(&self) -> anyhow::Result<Vec<BrokenLink>> {
        let mut broken = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let source = wikilink::relative_path(entry.path(), &self.config.vault_path);
                for link in wikilink::extract_wikilinks(&content) {
                    if wikilink::resolve_wikilink(&link.target, &self.config.vault_path).is_none() {
                        broken.push(BrokenLink {
                            source: source.clone(),
                            target: link.target,
                        });
                    }
                }
            }
        }

        Ok(broken)
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/links.rs`:

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindBrokenLinksRequest {}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line:

```rust
    // ===== Template Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "Find all [[wikilinks]] across the vault that don't resolve to an existing note.")]
    fn find_broken_links(
        &self,
        Parameters(_req): Parameters<tools::links::FindBrokenLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.find_broken_links() {
            Ok(broken) => {
                let results: Vec<serde_json::Value> = broken.iter().map(|b| {
                    serde_json::json!({ "source": b.source, "target": b.target })
                }).collect();
                let result = serde_json::json!({
                    "broken_links": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --test integration test_vault_find_broken_links`
Expected: PASS

- [ ] **Step 7: Update README**

In `README.md`, find:

```
| `link_related_notes` | Find notes related by content similarity and add a `## Related` section |
```

Insert immediately after it:

```
| `find_broken_links` | Find `[[wikilinks]]` that don't resolve to an existing note |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/links.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add find_broken_links tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 4: `find_orphan_notes` tool

**Files:**
- Modify: `src/vault.rs` (add `HashSet` import; add `Vault::find_orphan_notes`, right before `pub fn rename_note`)
- Modify: `src/tools/links.rs` (add `FindOrphanNotesRequest`)
- Modify: `src/main.rs` (add `find_orphan_notes` tool method, right before the `// ===== Template Tools =====` comment)
- Modify: `README.md` (Links table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Consumes: `wikilink::extract_wikilinks` (existing).
- Produces: `Vault::find_orphan_notes(&self) -> anyhow::Result<Vec<String>>`. "Orphan" means zero incoming links (nothing links to it), regardless of whether it links out to others.

- [ ] **Step 1: Write the failing test**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_find_orphan_notes() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-orphan.md"));

    vault.create_note("_test-orphan.md", "Nothing links here.", None).unwrap();

    let orphans = vault.find_orphan_notes().unwrap();
    assert!(orphans.iter().any(|o| o.contains("_test-orphan")));
    assert!(!orphans.iter().any(|o| o == "note1.md"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-orphan.md"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration test_vault_find_orphan_notes`
Expected: FAIL to compile — `Vault::find_orphan_notes` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::find_orphan_notes`**

In `src/vault.rs`, find this exact import line:

```rust
use std::collections::HashMap;
```

Change to:

```rust
use std::collections::{HashMap, HashSet};
```

Then find this exact line (the start of the `rename_note` method — `find_broken_links` from Task 3 is now immediately above it):

```rust
    pub fn rename_note(&self, source: &str, dest: &str) -> anyhow::Result<NoteInfo> {
```

Insert immediately before it:

```rust
    pub fn find_orphan_notes(&self) -> anyhow::Result<Vec<String>> {
        let mut linked_stems: HashSet<String> = HashSet::new();
        let mut all_notes: Vec<String> = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            let rel = wikilink::relative_path(entry.path(), &self.config.vault_path);
            all_notes.push(rel);

            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                for link in wikilink::extract_wikilinks(&content) {
                    let stem = Path::new(&link.target)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&link.target)
                        .to_string();
                    linked_stems.insert(stem);
                }
            }
        }

        // "Orphan" here means no incoming links, regardless of whether the
        // note itself links out — matching backlinks()'s own file-stem
        // comparison, not full-path comparison.
        let orphans = all_notes.into_iter()
            .filter(|rel| {
                let stem = Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or(rel);
                !linked_stems.contains(stem)
            })
            .collect();

        Ok(orphans)
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/links.rs` (after `FindBrokenLinksRequest` from Task 3):

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindOrphanNotesRequest {}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line:

```rust
    // ===== Template Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "Find notes with no backlinks \u{2014} nothing else in the vault links to them.")]
    fn find_orphan_notes(
        &self,
        Parameters(_req): Parameters<tools::links::FindOrphanNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.find_orphan_notes() {
            Ok(orphans) => {
                let result = serde_json::json!({
                    "orphans": orphans,
                    "count": orphans.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --test integration test_vault_find_orphan_notes`
Expected: PASS

- [ ] **Step 7: Update README**

In `README.md`, find (now immediately after the `find_broken_links` row added in Task 3):

```
| `find_broken_links` | Find `[[wikilinks]]` that don't resolve to an existing note |
```

Insert immediately after it:

```
| `find_orphan_notes` | Find notes with no backlinks (nothing links to them) |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/links.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add find_orphan_notes tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 5: `list_recent_notes` tool

**Files:**
- Modify: `src/vault.rs` (add `Vault::list_recent_notes`, right before `pub fn create_folder`)
- Modify: `src/tools/read.rs` (add `ListRecentNotesRequest`)
- Modify: `src/main.rs` (add `list_recent_notes` tool method, right before the `// ===== Search Tools =====` comment)
- Modify: `README.md` (Read table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Produces: `Vault::list_recent_notes(&self, limit: usize) -> anyhow::Result<Vec<(String, std::time::SystemTime)>>`, sorted newest-first, truncated to `limit`.

- [ ] **Step 1: Write the failing test**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_list_recent_notes() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-recent.md"));

    vault.create_note("_test-recent.md", "Recently created", None).unwrap();

    let recent = vault.list_recent_notes(100).unwrap();
    assert!(recent.iter().any(|(p, _)| p == "_test-recent.md"));
    for pair in recent.windows(2) {
        assert!(pair[0].1 >= pair[1].1, "results not sorted by modified time descending");
    }

    let limited = vault.list_recent_notes(1).unwrap();
    assert_eq!(limited.len(), 1);

    let _ = std::fs::remove_file(test_vault_path().join("_test-recent.md"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration test_vault_list_recent_notes`
Expected: FAIL to compile — `Vault::list_recent_notes` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::list_recent_notes`**

In `src/vault.rs`, find this exact line (the start of the `create_folder` method):

```rust
    pub fn create_folder(&self, folder_path: &str) -> anyhow::Result<()> {
```

Insert immediately before it:

```rust
    pub fn list_recent_notes(&self, limit: usize) -> anyhow::Result<Vec<(String, std::time::SystemTime)>> {
        let mut entries: Vec<(String, std::time::SystemTime)> = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let rel = wikilink::relative_path(entry.path(), &self.config.vault_path);
                    entries.push((rel, modified));
                }
            }
        }

        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(limit);
        Ok(entries)
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/read.rs`:

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRecentNotesRequest {
    #[schemars(description = "Max notes to return (optional, defaults to 20)")]
    pub limit: Option<usize>,
}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line:

```rust
    // ===== Search Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "List notes sorted by last-modified time, newest first.")]
    fn list_recent_notes(
        &self,
        Parameters(req): Parameters<tools::read::ListRecentNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = req.limit.unwrap_or(20);
        match self.vault.list_recent_notes(limit) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|(path, modified)| {
                    let modified: chrono::DateTime<chrono::Utc> = (*modified).into();
                    serde_json::json!({
                        "path": path,
                        "modified": modified.to_rfc3339(),
                    })
                }).collect();
                let result = serde_json::json!({
                    "notes": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --test integration test_vault_list_recent_notes`
Expected: PASS

- [ ] **Step 7: Update README**

In `README.md`, find:

```
| `get_metadata` | Get frontmatter, tags, outgoing links, and backlink count |
```

Insert immediately after it:

```
| `list_recent_notes` | List notes sorted by last-modified time, newest first |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/read.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add list_recent_notes tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 6: `get_section` tool

**Files:**
- Modify: `src/vault.rs` (add `sections` to the `parse` import; add `Vault::get_section`, right before `pub fn list_vault`)
- Modify: `src/tools/read.rs` (add `GetSectionRequest`)
- Modify: `src/main.rs` (add `get_section` tool method, right before the `// ===== Search Tools =====` comment)
- Modify: `README.md` (Read table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Consumes: `sections::find_section` and `sections::SectionError` from Task 1.
- Produces: `Vault::get_section(&self, note_path: &str, heading: &str) -> anyhow::Result<String>`. The `heading` argument must include its `#` markers (e.g. `"## Tasks"`).

- [ ] **Step 1: Write the failing tests**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_get_section() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-get-section.md"));

    vault.create_note(
        "_test-get-section.md",
        "# Title\n\n## Tasks\n\n- one\n- two\n\n## Notes\n\nSome notes.\n",
        None,
    ).unwrap();

    let section = vault.get_section("_test-get-section.md", "## Tasks").unwrap();
    assert!(section.contains("- one"));
    assert!(section.contains("- two"));
    assert!(!section.contains("Some notes"));

    let missing = vault.get_section("_test-get-section.md", "## Nonexistent");
    assert!(missing.is_err());

    let _ = std::fs::remove_file(test_vault_path().join("_test-get-section.md"));
}

#[test]
fn test_vault_get_section_ambiguous() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-get-section-ambig.md"));

    vault.create_note(
        "_test-get-section-ambig.md",
        "## Notes\n\nFirst.\n\n## Other\n\nMiddle.\n\n## Notes\n\nSecond.\n",
        None,
    ).unwrap();

    let err = vault.get_section("_test-get-section-ambig.md", "## Notes").unwrap_err();
    assert!(err.to_string().contains("ambiguous"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-get-section-ambig.md"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration test_vault_get_section`
Expected: FAIL to compile — `Vault::get_section` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::get_section`**

In `src/vault.rs`, find this exact line:

```rust
use crate::parse::{frontmatter, wikilink, tags};
```

Change to:

```rust
use crate::parse::{frontmatter, wikilink, tags, sections};
```

Then find this exact line (the start of the `list_vault` method):

```rust
    pub fn list_vault(&self, subpath: Option<&str>, depth: Option<usize>) -> anyhow::Result<Vec<String>> {
```

Insert immediately before it:

```rust
    pub fn get_section(&self, note_path: &str, heading: &str) -> anyhow::Result<String> {
        let full_path = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full_path)?;
        let parsed = frontmatter::parse(&content);

        match sections::find_section(&parsed.body, heading) {
            Ok(section) => Ok(parsed.body[section.start..section.end].trim_end().to_string()),
            Err(sections::SectionError::NotFound) => {
                Err(anyhow::anyhow!("Section '{}' not found", heading))
            }
            Err(sections::SectionError::Ambiguous(n)) => Err(anyhow::anyhow!(
                "Heading '{}' matches {} sections; ambiguous", heading, n
            )),
        }
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/read.rs` (after `ListRecentNotesRequest` from Task 5):

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSectionRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Heading to read, including its '#' markers (e.g. '## Tasks')")]
    pub heading: String,
}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line (now immediately after the `list_recent_notes` method added in Task 5):

```rust
    // ===== Search Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "Get the text under a heading in a note (e.g. '## Tasks'), up to the next heading of equal or higher level.")]
    fn get_section(
        &self,
        Parameters(req): Parameters<tools::read::GetSectionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.get_section(&req.path, &req.heading) {
            Ok(section) => {
                let result = serde_json::json!({
                    "path": req.path,
                    "heading": req.heading,
                    "content": section,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test integration test_vault_get_section`
Expected: PASS (`test_vault_get_section`, `test_vault_get_section_ambiguous`)

- [ ] **Step 7: Update README**

In `README.md`, find (now immediately after the `list_recent_notes` row added in Task 5):

```
| `list_recent_notes` | List notes sorted by last-modified time, newest first |
```

Insert immediately after it:

```
| `get_section` | Read a single section of a note by heading (e.g. '## Tasks') |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/read.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add get_section tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 7: `update_section` tool

**Files:**
- Modify: `src/vault.rs` (add `Vault::update_section`, right before `pub fn search_notes`)
- Modify: `src/tools/write.rs` (add `UpdateSectionRequest`)
- Modify: `src/main.rs` (add `update_section` tool method, right before the `// ===== Link Tools =====` comment)
- Modify: `README.md` (Write table)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Consumes: `sections::find_section` / `sections::SectionError` (Task 1); `serialize_frontmatter` (existing private free function at the bottom of `src/vault.rs`, already used by `update_note`/`set_frontmatter`/`bulk_tag`/`link_related_notes`).
- Produces: `Vault::update_section(&self, note_path: &str, heading: &str, content: &str, mode: &str) -> anyhow::Result<NoteInfo>`. `mode` must be `"append"` or `"replace"`; anything else is an error. A heading that doesn't exist is created at the end of the note.

- [ ] **Step 1: Write the failing tests**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_update_section_replace() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section.md"));

    vault.create_note(
        "_test-update-section.md",
        "# Title\n\n## Tasks\n\n- old\n\n## Notes\n\nUnrelated.\n",
        None,
    ).unwrap();

    let updated = vault.update_section("_test-update-section.md", "## Tasks", "- new", "replace").unwrap();
    assert!(updated.body.contains("- new"));
    assert!(!updated.body.contains("- old"));
    assert!(updated.body.contains("Unrelated."));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section.md"));
}

#[test]
fn test_vault_update_section_append() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-append.md"));

    vault.create_note(
        "_test-update-section-append.md",
        "## Tasks\n\n- one\n\n## Notes\n\nUnrelated.\n",
        None,
    ).unwrap();

    let updated = vault.update_section("_test-update-section-append.md", "## Tasks", "- two", "append").unwrap();
    assert!(updated.body.contains("- one"));
    assert!(updated.body.contains("- two"));
    assert!(updated.body.contains("Unrelated."));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-append.md"));
}

#[test]
fn test_vault_update_section_creates_missing_heading() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-missing.md"));

    vault.create_note("_test-update-section-missing.md", "# Title\n\nBody text.\n", None).unwrap();

    let updated = vault.update_section("_test-update-section-missing.md", "## Tasks", "- new task", "append").unwrap();
    assert!(updated.body.contains("## Tasks"));
    assert!(updated.body.contains("- new task"));
    assert!(updated.body.contains("Body text."));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-missing.md"));
}

#[test]
fn test_vault_update_section_preserves_nested_subheadings() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-nested.md"));

    vault.create_note(
        "_test-update-section-nested.md",
        "## Tasks\n\n### Subtask\n\nDetail.\n\n## Notes\n\nOther.\n",
        None,
    ).unwrap();

    let updated = vault.update_section("_test-update-section-nested.md", "## Tasks", "- appended", "append").unwrap();
    assert!(updated.body.contains("### Subtask"));
    assert!(updated.body.contains("Detail."));
    assert!(updated.body.contains("- appended"));
    assert!(updated.body.contains("## Notes"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-nested.md"));
}

#[test]
fn test_vault_update_section_invalid_mode() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-mode.md"));

    vault.create_note("_test-update-section-mode.md", "## Tasks\n\n- one\n", None).unwrap();

    let err = vault.update_section("_test-update-section-mode.md", "## Tasks", "x", "bogus").unwrap_err();
    assert!(err.to_string().contains("Invalid mode"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-mode.md"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration test_vault_update_section`
Expected: FAIL to compile — `Vault::update_section` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::update_section`**

In `src/vault.rs`, find this exact line (the start of the `search_notes` method):

```rust
    pub fn search_notes(&self, query: &str, limit: usize) -> anyhow::Result<Vec<NoteInfo>> {
```

Insert immediately before it:

```rust
    pub fn update_section(&self, note_path: &str, heading: &str, content: &str, mode: &str) -> anyhow::Result<NoteInfo> {
        if mode != "append" && mode != "replace" {
            return Err(anyhow::anyhow!("Invalid mode: {} (use 'append' or 'replace')", mode));
        }

        let full_path = self.resolve_note_path(note_path)?;
        let existing = std::fs::read_to_string(&full_path)?;
        let parsed = frontmatter::parse(&existing);

        let new_body = match sections::find_section(&parsed.body, heading) {
            Ok(section) => {
                let heading_line_end = parsed.body[section.start..]
                    .find('\n')
                    .map(|p| section.start + p + 1)
                    .unwrap_or(parsed.body.len());
                let heading_line = &parsed.body[section.start..heading_line_end];

                if mode == "replace" {
                    format!(
                        "{}{}{}\n{}",
                        &parsed.body[..section.start],
                        heading_line,
                        content.trim_end(),
                        &parsed.body[section.end..]
                    )
                } else {
                    let mut section_body = parsed.body[..section.end].to_string();
                    if !section_body.ends_with('\n') {
                        section_body.push('\n');
                    }
                    section_body.push_str(content.trim_end());
                    section_body.push('\n');
                    format!("{}{}", section_body, &parsed.body[section.end..])
                }
            }
            Err(sections::SectionError::NotFound) => {
                let mut body = parsed.body.trim_end().to_string();
                body.push_str("\n\n");
                body.push_str(heading.trim());
                body.push('\n');
                body.push_str(content.trim_end());
                body.push('\n');
                body
            }
            Err(sections::SectionError::Ambiguous(n)) => {
                return Err(anyhow::anyhow!(
                    "Heading '{}' matches {} sections; ambiguous", heading, n
                ));
            }
        };

        let fm_str = serialize_frontmatter(&parsed.frontmatter);
        let final_content = if parsed.frontmatter.is_empty() {
            new_body
        } else {
            format!("---\n{}---\n{}", fm_str, new_body)
        };

        std::fs::write(&full_path, &final_content)?;
        self.read_note(note_path)
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/write.rs`:

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSectionRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Heading to update, including its '#' markers (e.g. '## Tasks'). Created at the end of the note if not found.")]
    pub heading: String,
    #[schemars(description = "Content to write into the section")]
    pub content: String,
    #[schemars(description = "'append' to add to the end of the section, 'replace' to overwrite it")]
    pub mode: String,
}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line:

```rust
    // ===== Link Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "Update just one section of a note, addressed by heading (e.g. '## Tasks'). Creates the section at the end of the note if the heading doesn't exist yet. Use 'append' to add to the section or 'replace' to overwrite it.")]
    fn update_section(
        &self,
        Parameters(req): Parameters<tools::write::UpdateSectionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.update_section(&req.path, &req.heading, &req.content, &req.mode) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "heading": req.heading,
                    "message": "Section updated successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test integration test_vault_update_section`
Expected: PASS (`test_vault_update_section_replace`, `test_vault_update_section_append`, `test_vault_update_section_creates_missing_heading`, `test_vault_update_section_preserves_nested_subheadings`, `test_vault_update_section_invalid_mode`)

- [ ] **Step 7: Update README**

In `README.md`, find:

```
| `bulk_tag` | Add or remove tags across notes matching a full-text search query |
```

Insert immediately after it:

```
| `update_section` | Append to or replace a single section of a note by heading |
```

- [ ] **Step 8: Commit**

```bash
git add src/vault.rs src/tools/write.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add update_section tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 8: `trash_note` tool

**Files:**
- Modify: `src/vault.rs` (add `Vault::trash_note`, right before `pub fn bulk_tag`)
- Modify: `src/tools/write.rs` (add `TrashNoteRequest`)
- Modify: `src/main.rs` (add `trash_note` tool method, right before the `// ===== Link Tools =====` comment)
- Modify: `README.md` (Write table + Security section)
- Test: `tests/integration.rs` (append)

**Interfaces:**
- Produces: `Vault::trash_note(&self, note_path: &str) -> anyhow::Result<()>`. Moves the resolved note into `.trash/` at the vault root, creating that folder if needed. On a filename collision, appends a numeric suffix (`note (1).md`, `note (2).md`, ...) rather than overwriting.

- [ ] **Step 1: Write the failing tests**

Append to `tests/integration.rs`:

```rust
#[test]
fn test_vault_trash_note() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let trash_dir = test_vault_path().join(".trash");
    let _ = std::fs::remove_file(test_vault_path().join("_test-trash.md"));
    let _ = std::fs::remove_file(trash_dir.join("_test-trash.md"));

    vault.create_note("_test-trash.md", "To be trashed", None).unwrap();
    vault.trash_note("_test-trash.md").unwrap();

    assert!(!test_vault_path().join("_test-trash.md").exists());
    assert!(trash_dir.join("_test-trash.md").exists());

    let _ = std::fs::remove_file(trash_dir.join("_test-trash.md"));
}

#[test]
fn test_vault_trash_note_collision() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let trash_dir = test_vault_path().join(".trash");
    let _ = std::fs::remove_file(test_vault_path().join("_test-trash-collide.md"));
    let _ = std::fs::remove_file(trash_dir.join("_test-trash-collide.md"));
    let _ = std::fs::remove_file(trash_dir.join("_test-trash-collide (1).md"));

    vault.create_note("_test-trash-collide.md", "First", None).unwrap();
    vault.trash_note("_test-trash-collide.md").unwrap();

    vault.create_note("_test-trash-collide.md", "Second", None).unwrap();
    vault.trash_note("_test-trash-collide.md").unwrap();

    assert!(trash_dir.join("_test-trash-collide.md").exists());
    assert!(trash_dir.join("_test-trash-collide (1).md").exists());

    let _ = std::fs::remove_file(trash_dir.join("_test-trash-collide.md"));
    let _ = std::fs::remove_file(trash_dir.join("_test-trash-collide (1).md"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration test_vault_trash_note`
Expected: FAIL to compile — `Vault::trash_note` doesn't exist yet.

- [ ] **Step 3: Implement `Vault::trash_note`**

In `src/vault.rs`, find this exact line (the start of the `bulk_tag` method):

```rust
    pub fn bulk_tag(&self, query: &str, add_tags: &[String], remove_tags: &[String]) -> anyhow::Result<usize> {
```

Insert immediately before it:

```rust
    pub fn trash_note(&self, note_path: &str) -> anyhow::Result<()> {
        let full_path = self.resolve_note_path(note_path)?;

        let trash_dir = self.config.vault_path.join(".trash");
        std::fs::create_dir_all(&trash_dir)?;

        let file_name = full_path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?
            .to_string_lossy()
            .to_string();
        let stem = full_path.file_stem().and_then(|s| s.to_str()).unwrap_or("note").to_string();
        let ext = full_path.extension().and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();

        let mut dest = trash_dir.join(&file_name);
        let mut counter = 1;
        while dest.exists() {
            dest = trash_dir.join(format!("{} ({}){}", stem, counter, ext));
            counter += 1;
        }

        std::fs::rename(&full_path, &dest)?;
        Ok(())
    }

```

- [ ] **Step 4: Add the request struct**

Append to `src/tools/write.rs` (after `UpdateSectionRequest` from Task 7):

```rust

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TrashNoteRequest {
    #[schemars(description = "Path to the note to move to .trash")]
    pub path: String,
}
```

- [ ] **Step 5: Add the tool method**

In `src/main.rs`, find this exact line (now immediately after the `update_section` method added in Task 7):

```rust
    // ===== Link Tools =====
```

Insert immediately before it:

```rust
    #[tool(description = "Move a note to the vault's .trash folder instead of deleting it. Collisions in .trash get a numeric suffix.")]
    fn trash_note(
        &self,
        Parameters(req): Parameters<tools::write::TrashNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.trash_note(&req.path) {
            Ok(()) => {
                let result = serde_json::json!({
                    "path": req.path,
                    "message": "Note moved to .trash",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test integration test_vault_trash_note`
Expected: PASS (`test_vault_trash_note`, `test_vault_trash_note_collision`)

- [ ] **Step 7: Update README**

In `README.md`, find (now immediately after the `update_section` row added in Task 7):

```
| `update_section` | Append to or replace a single section of a note by heading |
```

Insert immediately after it:

```
| `trash_note` | Move a note to the vault's `.trash` folder instead of deleting it |
```

Then find:

```
- **Listing depth limit**: Recursive listing capped at 20 levels
```

Insert immediately after it:

```
- **Trash, not delete**: `trash_note` moves notes into a vault-local `.trash/` folder rather than deleting them; that folder is still a normal, visible part of the vault to other tools (e.g. `list_vault`, `search_notes`)
```

- [ ] **Step 8: Run the full test suite**

Run: `cargo test`
Expected: PASS — all tests, old and new, green.

- [ ] **Step 9: Commit**

```bash
git add src/vault.rs src/tools/write.rs src/main.rs README.md tests/integration.rs
git commit -m "feat: add trash_note tool

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```
