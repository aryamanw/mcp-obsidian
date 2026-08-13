# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release        # build the obsidian-mcp binary
cargo test                   # run all tests (unit tests in src/, integration tests in tests/integration.rs)
cargo test test_vault_read_note   # run a single test by name
cargo test --test integration     # run only the integration test binary
```

To run the server manually against a real vault (it speaks MCP over stdio, so it's not interactive):

```bash
OBSIDIAN_VAULT="/path/to/vault" cargo run
```

Integration tests read fixtures from `tests/fixtures/test-vault/` — no `OBSIDIAN_VAULT` env var needed for `cargo test`.

## Architecture

This is a single-binary [rmcp](https://docs.rs/rmcp) (Rust MCP SDK) server exposing an Obsidian vault as MCP tools over stdio. Logs go to stderr (`tracing`); MCP protocol messages go to stdout — never write to stdout directly.

**Layering**, bottom to top:

- `src/parse/` — pure, stateless parsing functions with no filesystem access: `frontmatter.rs` (YAML frontmatter block extraction, capped at 8 KB), `wikilink.rs` (`[[wikilink]]` regex extraction/resolution), `tags.rs` (`#tag` extraction from body + frontmatter `tags` field). These are the easiest place to add unit tests (see `tests/integration.rs`, which despite the name covers both parse-level unit tests and vault-level integration tests in one file).
- `src/vault.rs` — the `Vault` struct is the core domain layer. All filesystem reads/writes to the vault go through it (`read_note`, `create_note`, `update_note`, `search_notes`, `rename_note`, etc.). It composes the `parse` functions and owns all path-safety logic.
- `src/graph.rs` — `GraphAnalyzer` reads Obsidian's own `.obsidian/graph.json` (not derived from `Vault`'s wikilink parsing) to compute stats, connected components, and shortest paths. This means graph tools only work if the user has opened the vault in Obsidian at least once to generate that file.
- `src/tools/*.rs` — request/response structs (`serde` + `schemars`-style `Deserialize`/JSON schema) for each tool, grouped by category (`read`, `search`, `write`, `links`, `templates`, `graph`). These are pure data shapes; no logic lives here.
- `src/main.rs` — the `ObsidianMcp` struct wires `Vault` + `GraphAnalyzer` together and declares every MCP tool via the `#[tool]` macro from `rmcp`. Each tool method is a thin adapter: deserialize request → call into `Vault`/`GraphAnalyzer` → serialize an ad-hoc `serde_json::Value` response (there are no typed response structs — responses are built inline with `serde_json::json!`). Errors from the domain layer become `CallToolResult::error`, not Rust-level panics/`Err`.

**Adding a new tool** means: define request struct in the matching `src/tools/*.rs` file, implement the operation on `Vault` (or `GraphAnalyzer`) in `src/vault.rs`/`src/graph.rs`, then add a `#[tool]`-annotated method in `src/main.rs` that wires them together and add it to the README's tool table.

### Path safety

All vault-relative paths passed by tool callers go through `Vault::validate_path` / `validate_parent`, which canonicalize the target (or its parent, for not-yet-existing paths) and verify it stays within the canonicalized vault root — this is the only defense against directory traversal and must not be bypassed when adding new filesystem-touching tools. Similarly, `apply_template` re-validates the resolved template path against the canonicalized templates directory before reading it.

### Note path conventions

Note paths are vault-relative, `/`-separated (even on Windows — see `relative_path`), and the `.md` extension is optional almost everywhere (`resolve_note_path` tries the bare path, then falls back to `path + ".md"`). Wikilink targets and backlink matching are compared by file stem, not full path, matching Obsidian's own linking behavior.
