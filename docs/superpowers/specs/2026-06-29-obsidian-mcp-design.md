# Obsidian MCP Server — Design Spec

**Date**: 2026-06-29
**Status**: Approved
**Language**: Rust
**Architecture**: Pure file system access (no runtime dependency on Obsidian)

## Purpose

Build an MCP (Model Context Protocol) server that gives AI assistants full management capabilities over an Obsidian vault — reading, writing, searching, link management, templates, and graph analysis.

## Constraints

- Vault path configured via environment variable in MCP config
- Pure file system access: works whether Obsidian is open or not
- Graph data read from Obsidian's native `.obsidian/graph.json`
- Must handle vaults of any size without performance degradation

## Project Structure

```
obsidian-mcp/
├── Cargo.toml
├── src/
│   ├── main.rs          # Entry point, MCP server setup
│   ├── config.rs        # Vault path config from env var
│   ├── vault.rs         # Core vault operations (read, write, search)
│   ├── parse/
│   │   ├── mod.rs
│   │   ├── frontmatter.rs  # YAML frontmatter parsing
│   │   ├── wikilink.rs     # [[wikilink]] resolution
│   │   └── tags.rs         # #tag extraction
│   ├── graph.rs         # Obsidian graph.json reader + analysis
│   └── tools/
│       ├── mod.rs
│       ├── read.rs      # read_note, list_vault, get_metadata
│       ├── search.rs    # search_notes, search_by_tag
│       ├── write.rs     # create_note, update_note, set_frontmatter
│       ├── links.rs     # resolve_links, backlinks, link_structure
│       ├── templates.rs # template list, apply template, daily note
│       └── graph.rs     # graph query, communities, path
├── docs/
│   └── superpowers/specs/
└── README.md
```

## Dependencies

- `rmcp` — Rust MCP SDK (stdio transport)
- `serde`, `serde_json` — Serialization
- `glob` — File pattern matching
- `yaml-rust2` — YAML frontmatter parsing
- `walkdir` — Recursive directory traversal
- `regex` — Wikilink and tag pattern matching
- `chrono` — Date handling for daily notes

## MCP Tools

### Read & Browse

| Tool | Parameters | Returns |
|------|-----------|---------|
| `read_note` | `path: string` | Markdown content + parsed frontmatter + resolved links |
| `list_vault` | `path?: string, depth?: number` | Directory tree of notes and folders |
| `get_metadata` | `path: string` | Frontmatter fields, tags, outgoing links, backlinks count |

### Search

| Tool | Parameters | Returns |
|------|-----------|---------|
| `search_notes` | `query: string, limit?: number` | Matching notes with snippets |
| `search_by_tag` | `tags: string[], match?: "any"\|"all"` | Notes containing specified tags |
| `search_by_frontmatter` | `filters: Record<string, string>` | Notes matching frontmatter criteria |

### Write & Update

| Tool | Parameters | Returns |
|------|-----------|---------|
| `create_note` | `path: string, content?: string, frontmatter?: object` | Confirmation + metadata |
| `update_note` | `path: string, content: string, mode: "append"\|"replace"` | Updated note metadata |
| `set_frontmatter` | `path: string, fields: Record<string, any>` | Updated frontmatter |

### Link Management

| Tool | Parameters | Returns |
|------|-----------|---------|
| `resolve_links` | `path: string` | List of wikilinks with resolved file paths |
| `backlinks` | `path: string` | Notes that link to the given note |
| `link_graph` | `path: string, depth?: number` | Note + its link neighbors (N hops) |

### Templates & Daily Notes

| Tool | Parameters | Returns |
|------|-----------|---------|
| `list_templates` | — | Available template names and paths |
| `apply_template` | `template: string, path: string` | Confirmation |
| `get_daily_note` | `date?: string (YYYY-MM-DD)` | Daily note content (creates if missing) |

### Graph Analysis

| Tool | Parameters | Returns |
|------|-----------|---------|
| `graph_stats` | — | Node count, edge count, density |
| `graph_communities` | — | Community clusters from Obsidian's graph data |
| `graph_path` | `from: string, to: string` | Shortest path between two notes |

## Configuration

Set via environment variable in MCP client config:

```json
{
  "mcp": {
    "servers": {
      "obsidian": {
        "command": "obsidian-mcp",
        "env": {
          "OBSIDIAN_VAULT": "C:/Users/Aryaman/Documents/MyVault"
        }
      }
    }
  }
}
```

## Data Flow

1. **Config** → Read `OBSIDIAN_VAULT` env var at startup, validate path exists
2. **Read** → `walkdir` traversal + `std::fs::read_to_string` for file content
3. **Parse** → Split at `---` for frontmatter, regex for `[[wikilinks]]` and `#tags`
4. **Write** → Atomic writes (temp file + rename) to prevent corruption
5. **Search** → In-memory term index built on first query, cached across calls
6. **Graph** → Read `.obsidian/graph.json`, parse into adjacency list
7. **Templates** → Read from `{vault}/templates/` folder (configurable)

## Error Handling

| Error | Response |
|-------|----------|
| Vault not found | "Vault not found at {path}" |
| Note not found | "Note not found: {path}" |
| Invalid frontmatter | Return raw content + warning |
| Permission denied | "Cannot write: permission denied for {path}" |
| graph.json missing | "Graph data unavailable — open Obsidian once to generate" |

## Testing

- **Unit tests**: Parsers (frontmatter, wikilinks, tags) with fixture markdown files
- **Integration tests**: Create temp vault, write/read round-trip, verify correctness
- **MCP protocol tests**: Verify tool call responses match expected JSON schemas
- **Graph tests**: Mock `graph.json` format, verify community detection and path finding

## Build

```bash
cargo build --release
# Produces: target/release/obsidian-mcp.exe
```
