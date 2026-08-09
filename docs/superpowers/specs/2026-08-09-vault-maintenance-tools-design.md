# Vault Maintenance & Editing Tools — Design

## Motivation

The current tool set (read, search, write, links, templates, graph) covers the core CRUD and
navigation operations on an Obsidian vault, but a scan against common vault-management needs
turned up six clear gaps:

- No way to see all tags used across a vault (only per-note tag reads and tag-filtered search).
- No way to find dangling `[[wikilinks]]` that don't resolve to any note.
- No way to find notes nothing else links to ("orphans").
- No way to list notes by recency (what was worked on most recently).
- No way to read or edit a single section of a note by heading — only whole-note
  append/replace via `update_note`.
- No way to remove a note. `create_note`, `rename_note`, and `merge_notes` exist, but there's
  no delete/trash tool.

These are all small, independently useful additions that follow the same shape as every
existing tool (a `Vault` method, a request struct in `src/tools/*.rs`, and a `#[tool]`-annotated
method in `src/main.rs`), so they're specified together as one batch rather than as separate
sub-projects.

## Non-goals

- No shared link-index cache. Every existing `Vault` method that scans the vault (`search_notes`,
  `search_by_tag`, `backlinks`, etc.) does its own independent `WalkDir` pass over all notes.
  Introducing a shared index (built once, reused across `find_broken_links`,
  `find_orphan_notes`, `backlinks`, etc.) would be a reasonable efficiency improvement for large
  vaults, but it's a structural refactor of existing code, not something this batch of additive
  tools needs. Deferred as a separate future improvement.
- No system-trash integration (e.g. macOS Trash, Windows Recycle Bin). `trash_note` moves files
  to a vault-local `.trash/` folder only.
- No creation-time metadata anywhere (e.g. in `list_recent_notes`). Filesystem creation time
  (`birthtime`) isn't reliably available across platforms/filesystems, so only modification time
  is used.

## New tools

| Tool | Category | Request | Response |
| --- | --- | --- | --- |
| `list_tags` | Search | *(none)* | All tags used in the vault, each with a usage count, sorted by count descending then name |
| `find_broken_links` | Links | *(none)* | Every `[[wikilink]]` across the vault that doesn't resolve to a note, paired with the note it appears in |
| `find_orphan_notes` | Links | *(none)* | Every note with zero backlinks (nothing links to it) |
| `list_recent_notes` | Read | `{ limit?: usize }` (default 20) | Notes sorted by filesystem modification time, newest first |
| `get_section` | Read | `{ path: string, heading: string }` | The text under the given heading, up to the next heading of equal or higher level |
| `update_section` | Write | `{ path: string, heading: string, content: string, mode: "append" \| "replace" }` | Edits just that section; creates it (as a new heading appended to the end of the note) if the heading doesn't exist |
| `trash_note` | Write | `{ path: string }` | Moves the note into `.trash/` at the vault root instead of deleting it |

## Detailed design

### `list_tags`

`Vault::list_tags(&self) -> anyhow::Result<Vec<(String, usize)>>`

One `WalkDir` pass over all `.md` files, reusing `tags::all_tags` (already used by
`search_by_tag`) on each note's content + frontmatter. Tag occurrences are accumulated into a
`HashMap<String, usize>` and returned as a `Vec` sorted by count descending, then alphabetically
for ties (same tie-break convention as `extract_significant_words` in `link_related_notes`).

### `find_broken_links`

`Vault::find_broken_links(&self) -> anyhow::Result<Vec<BrokenLink>>` where
`BrokenLink { source: String, target: String }`.

Walks all notes, extracts wikilinks via the existing `wikilink::extract_wikilinks`, and resolves
each target via the existing `wikilink::resolve_wikilink`. Any target that resolves to `None` is
recorded as broken, along with the path of the note containing it. This reuses exactly the logic
already proven correct in the `resolve_links` tool — the difference is running it vault-wide
instead of for one note's links.

This is deliberately independent of Obsidian's `.obsidian/graph.json` (unlike the `graph_*`
tools), so it works even in a vault that's never been opened in Obsidian.

### `find_orphan_notes`

`Vault::find_orphan_notes(&self) -> anyhow::Result<Vec<String>>`

Walks all notes once, extracting wikilinks from each and normalizing each target to a file stem
(matching how `backlinks()` already compares links — by stem, not full path). This builds a
`HashSet<String>` of every stem that is linked to from somewhere in the vault. A second pass over
all note stems returns every note whose stem is *not* in that set.

**Orphan definition:** a note with **no incoming links**, regardless of whether it has outgoing
links. A hub/index note that only links out to other notes is not an orphan by this definition —
only notes that nothing points to are flagged.

### `list_recent_notes`

`Vault::list_recent_notes(&self, limit: usize) -> anyhow::Result<Vec<(String, SystemTime)>>`

Walks all notes, reads `entry.metadata()?.modified()?` for each, sorts descending by
timestamp, and truncates to `limit`. The `#[tool]` layer serializes the timestamp as an RFC 3339
string (via `chrono`, already a dependency) rather than exposing `SystemTime` directly.

### `get_section` / `update_section`

New module `src/parse/sections.rs`, parallel in style to `wikilink.rs` and `tags.rs`:

```rust
pub struct Section {
    pub start: usize,   // byte offset of the heading line
    pub end: usize,     // byte offset where the section ends (next heading of equal/higher level, or end of body)
    pub level: usize,   // heading level (1 for '#', 2 for '##', etc.)
}

pub enum SectionError {
    NotFound,
    Ambiguous(usize), // number of headings matching the given text
}

pub fn find_section(body: &str, heading: &str) -> Result<Section, SectionError>;
```

The `heading` argument is the full heading line as it would appear in the note, `#` markers
included (e.g. `"## Tasks"`, not `"Tasks"`). `find_section` matches a heading line only when both
its level (number of leading `#`s) and its trimmed text match the argument exactly — two notes
with `"# Tasks"` and `"## Tasks"` are different sections, not a match for the same query. Once
matched, the section runs until the next line that is a heading of level `<=` the matched
heading's level, or the end of the document if none. This means a section's own nested
subheadings (e.g. `###` under a matched `##`) stay part of the section, matching how Obsidian
treats sections when folding or copying them.

**Ambiguity:** if more than one heading in the note has matching text, `find_section` returns
`SectionError::Ambiguous(n)`. Both `get_section` and `update_section` surface this as a
`CallToolResult::error` naming the count, rather than silently operating on the first match —
consistent with the codebase's existing preference for explicit errors (e.g. `update_note`'s
rejection of unknown `mode` values) over silently-wrong behavior.

`Vault::get_section(&self, path: &str, heading: &str) -> anyhow::Result<String>` reads the note,
runs `find_section` on the body, and returns the slice (heading line inclusive) as a string.

`Vault::update_section(&self, path: &str, heading: &str, content: &str, mode: &str) -> anyhow::Result<NoteInfo>`:

- If the heading is found: in `"replace"` mode, the section's byte range is replaced with the new
  heading line + `content`; in `"append"` mode, `content` is inserted before the section's end
  boundary (i.e. added to the end of the existing section body, before the next heading).
- If the heading is **not** found: a new heading line (at the level implied by the `heading`
  argument's own `#` prefix, e.g. passing `"## Tasks"` creates a level-2 heading) followed by
  `content` is appended to the end of the note body. This makes `update_section` usable as an
  idempotent "ensure this section exists with this content" call.
- Frontmatter is preserved exactly as `update_note` already does today (re-serialized unchanged,
  body updated).
- An invalid `mode` (anything other than `"append"`/`"replace"`) is rejected the same way
  `update_note` already rejects invalid modes.

### `trash_note`

`Vault::trash_note(&self, path: &str) -> anyhow::Result<()>`

Resolves and validates the path via the existing `resolve_note_path`. Ensures `.trash/` exists at
the vault root (lazily created via `create_dir_all`, the same pattern `get_daily_note` already
uses for its daily-notes directory). Moves the file into `.trash/` via `std::fs::rename`.

**Collision handling:** if a file with the same name already exists in `.trash/`, a numeric
suffix is appended (`note.md` → `note (1).md`, `note (2).md`, ...) rather than overwriting the
existing trashed file — trash is meant to be a recoverable safety net, not another way to
silently lose data.

`.trash/` is just a regular vault folder from the tool set's perspective: existing tools like
`list_vault` or `search_notes` will see trashed notes unless the caller filters them out. This is
consistent with how Obsidian's own local trash works (files stay inside the vault, in a special
folder) and keeps the implementation simple — no new "hidden from other tools" concept is
introduced.

## Error handling

All new `Vault` methods return `anyhow::Result<T>`, matching every existing method. All new
`#[tool]` methods in `src/main.rs` follow the existing
`match result { Ok(v) => CallToolResult::success(...), Err(e) => CallToolResult::error(...) }`
shape — no new error-handling convention is introduced.

`trash_note`, `get_section`, and `update_section` all reuse existing path-validation
(`resolve_note_path` / `validate_parent`) rather than introducing new checks, since they operate
on note paths in the same way existing write/read tools already do.

## Testing

Integration tests are added to `tests/integration.rs`, following its existing style (fixture
vault at `tests/fixtures/test-vault/`, one `#[test]` per behavior):

- `list_tags`: returns known tags from the fixture vault with correct counts.
- `find_broken_links`: a fixture note with a dangling `[[wikilink]]` to a nonexistent note is
  detected; existing resolvable links in the fixture vault are not falsely flagged.
- `find_orphan_notes`: a fixture note with no backlinks is returned; notes that are linked to are
  not.
- `list_recent_notes`: returns notes ordered by modification time; respects `limit`.
- `get_section`: retrieves a known section by heading; returns `NotFound` for a missing heading;
  returns `Ambiguous` for a fixture note with two headings at the same level and matching text.
- `update_section`: `"replace"` mode overwrites an existing section; `"append"` mode adds to the
  end of an existing section without disturbing sibling sections; targeting a missing heading
  creates a new section at the end of the note; nested subheadings under a matched heading are
  preserved as part of the section during both read and write.
- `trash_note`: moves a note into `.trash/`; a second `trash_note` call for a same-named file
  does not overwrite the first.

Unit tests for `find_section`'s boundary logic (nested headings, end-of-document boundary,
ambiguous match, no match) are added alongside the new `src/parse/sections.rs` module itself.

## Documentation

Each new tool gets a row in the appropriate table in `README.md` (Read / Search / Write / Links),
matching the existing documentation style. The `.trash/` behavior is noted under the existing
"Security" section, since it changes what happens to file paths users might expect to have
disappeared.
