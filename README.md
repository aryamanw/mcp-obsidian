# Obsidian MCP

A high-performance [Model Context Protocol](https://modelcontextprotocol.io) server that bridges AI assistants with your Obsidian vault. Reads, writes, searches, and analyzes your notes through a standardized tool interface.

## Features

**Read & Browse**
- Read any note with full frontmatter parsing, tag extraction, and link resolution
- Browse vault folder structure with configurable depth
- Get metadata summaries (frontmatter, tags, outgoing links, backlinks)

**Search**
- Full-text search across all markdown notes with result snippets
- Tag-based search with OR/AND matching modes
- Frontmatter field filtering by key-value pairs

**Write**
- Create new notes with optional content and frontmatter
- Update existing notes (append or replace content, preserves frontmatter)
- Set and merge frontmatter fields on any note

**Link Management**
- Resolve `[[wikilinks]]` to actual file paths
- Find all backlinks to a given note
- Traverse the link graph up to N hops deep

**Templates & Daily Notes**
- List available templates from your vault
- Apply templates to notes (merges frontmatter, preserves body)
- Get or create daily notes (auto-detects daily notes directory)

**Graph Analytics**
- Vault graph statistics (node count, edge count, density)
- Connected components (community detection)
- Shortest path between any two notes

## Prerequisites

- An existing [Obsidian](https://obsidian.md) vault

## Installation

### Option 1: One-liner (Windows)

```powershell
irm https://raw.githubusercontent.com/aryamanw/mcp-obsidian/main/install.ps1 | iex
```

This downloads the latest release, installs it to `~/.local/bin`, and optionally adds it to your PATH.

### Option 2: Direct download

Download `obsidian-mcp.exe` from the [Releases](https://github.com/aryamanw/mcp-obsidian/releases) page and place it somewhere on your PATH.

### Option 3: Build from source

```bash
git clone <repo-url>
cd obsidian-mcp
cargo build --release
```

The binary will be at `target/release/obsidian-mcp.exe`.

## Configuration

Set the `OBSIDIAN_VAULT` environment variable to the path of your Obsidian vault:

```powershell
$env:OBSIDIAN_VAULT = "C:\path\to\your\vault"
```

The server validates this path on startup and rejects all operations that attempt directory traversal outside the vault.

## Usage

Run the server:

```bash
obsidian-mcp.exe
```

The server communicates over **stdio** using the MCP protocol. Logs are written to stderr; MCP messages flow over stdout.

### MCP Client Configuration

Add to your MCP client's configuration file:

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "path\\to\\obsidian-mcp.exe",
      "env": {
        "OBSIDIAN_VAULT": "C:\\path\\to\\your\\vault"
      }
    }
  }
}
```

**VS Code / Cline** (`cline_mcp_settings.json` or similar):

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "path\\to\\obsidian-mcp.exe",
      "env": {
        "OBSIDIAN_VAULT": "C:\\path\\to\\your\\vault"
      }
    }
  }
}
```

## Tools

### Read
| Tool | Description |
|---|---|
| `read_note` | Read a note's content, frontmatter, tags, and resolved links |
| `list_vault` | List vault structure — folders and notes |
| `get_metadata` | Get frontmatter, tags, outgoing links, and backlink count |

### Search
| Tool | Description |
|---|---|
| `search_notes` | Full-text search with preview snippets |
| `search_by_tag` | Find notes by tags (OR or AND mode) |
| `search_by_frontmatter` | Filter notes by frontmatter key-value pairs |

### Write
| Tool | Description |
|---|---|
| `create_note` | Create a new note with optional content and frontmatter |
| `update_note` | Append or replace note content (preserves frontmatter) |
| `set_frontmatter` | Set or merge frontmatter fields on a note |

### Links
| Tool | Description |
|---|---|
| `resolve_links` | Resolve all `[[wikilinks]]` to file paths |
| `backlinks` | Find all notes that link to a given note |
| `link_graph` | Traverse outgoing links up to N hops |

### Templates
| Tool | Description |
|---|---|
| `list_templates` | List available templates |
| `apply_template` | Apply a template (merges frontmatter, preserves body) |
| `get_daily_note` | Get or create today's daily note |

### Graph
| Tool | Description |
|---|---|
| `graph_stats` | Vault graph statistics |
| `graph_communities` | Connected components in the note graph |
| `graph_path` | Shortest path between two notes |

> **Note:** Graph tools rely on Obsidian's built-in `graph.json`. Open Obsidian at least once to generate it.

## Security

- **Path traversal protection**: All file paths are validated to stay within the vault directory
- **Frontmatter size limit**: Parsing capped at 8 KB
- **Template isolation**: Templates confirmed to reside within the templates directory
- **Listing depth limit**: Recursive listing capped at 20 levels

## Building from Source

```bash
cargo build --release
cargo test
```

## License

MIT
