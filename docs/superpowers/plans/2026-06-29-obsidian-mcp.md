# Obsidian MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust MCP server that gives AI assistants full management capabilities over an Obsidian vault — reading, writing, searching, link management, templates, and graph analysis.

**Architecture:** Pure file system access using the `rmcp` Rust MCP SDK. Reads/writes markdown files directly, parses YAML frontmatter and wikilinks, accesses Obsidian's native `.obsidian/graph.json` for graph analysis. Stdio transport for MCP communication.

**Tech Stack:** Rust, `rmcp` 1.x (MCP SDK), `serde`/`serde_json`, `yaml-rust2`, `walkdir`, `glob`, `regex`, `chrono`, `tokio`

---

## File Map

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Project metadata, dependencies |
| `src/main.rs` | Entry point, MCP server setup with stdio transport |
| `src/config.rs` | Read `OBSIDIAN_VAULT` env var, validate path |
| `src/vault.rs` | Core vault operations: read, write, list, search |
| `src/parse/frontmatter.rs` | Extract YAML frontmatter from markdown |
| `src/parse/wikilink.rs` | Extract and resolve `[[wikilinks]]` |
| `src/parse/tags.rs` | Extract `#tags` from body and frontmatter |
| `src/graph.rs` | Read `.obsidian/graph.json`, adjacency list, path finding |
| `src/tools/read.rs` | MCP tools: `read_note`, `list_vault`, `get_metadata` |
| `src/tools/search.rs` | MCP tools: `search_notes`, `search_by_tag`, `search_by_frontmatter` |
| `src/tools/write.rs` | MCP tools: `create_note`, `update_note`, `set_frontmatter` |
| `src/tools/links.rs` | MCP tools: `resolve_links`, `backlinks`, `link_graph` |
| `src/tools/templates.rs` | MCP tools: `list_templates`, `apply_template`, `get_daily_note` |
| `src/tools/graph.rs` | MCP tools: `graph_stats`, `graph_communities`, `graph_path` |
| `src/tools/mod.rs` | Tool module declarations |
| `src/parse/mod.rs` | Parser module declarations |

---

### Task 1: Project scaffolding + Cargo.toml

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Initialize Cargo project**

```bash
cargo init --name obsidian-mcp
```

- [ ] **Step 2: Write Cargo.toml**

```toml
[package]
name = "obsidian-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
rmcp = { version = "1", features = ["server", "transport-io", "macros"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
yaml-rust2 = "0.9"
walkdir = "2"
glob = "0.3"
regex = "1"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
```

- [ ] **Step 3: Write minimal main.rs that compiles**

```rust
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Obsidian MCP server");
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo build 2>&1
```

Expected: Compiles with warnings about unused imports (fine for now).

- [ ] **Step 5: Commit**

```bash
git init
echo "/target" > .gitignore
git add .
git commit -m "chore: scaffold obsidian-mcp project"
```

---

### Task 2: Config module — read vault path

**Files:**
- Create: `src/config.rs`

- [ ] **Step 1: Write config.rs**

```rust
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub vault_path: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let vault_path = std::env::var("OBSIDIAN_VAULT")
            .map_err(|_| anyhow::anyhow!(
                "OBSIDIAN_VAULT environment variable not set. \
                 Set it to your Obsidian vault path in MCP config."
            ))?;

        let vault_path = PathBuf::from(&vault_path);
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Vault not found at {}. Check OBSIDIAN_VAULT path.",
                vault_path.display()
            ));
        }
        if !vault_path.is_dir() {
            return Err(anyhow::anyhow!(
                "OBSIDIAN_VAULT ({}) is not a directory.",
                vault_path.display()
            ));
        }

        Ok(Self { vault_path })
    }
}
```

- [ ] **Step 2: Add `use` to main.rs**

Add at top of `src/main.rs`:

```rust
mod config;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src/config.rs
git commit -m "feat: add config module to read OBSIDIAN_VAULT env var"
```

---

### Task 3: Frontmatter parser

**Files:**
- Create: `src/parse/mod.rs`
- Create: `src/parse/frontmatter.rs`

- [ ] **Step 1: Create parse module**

`src/parse/mod.rs`:

```rust
pub mod frontmatter;
pub mod wikilink;
pub mod tags;
```

- [ ] **Step 2: Write frontmatter.rs**

```rust
use std::collections::HashMap;
use yaml_rust2::{YamlLoader, Yaml};

#[derive(Debug, Clone)]
pub struct NoteContent {
    pub frontmatter: HashMap<String, String>,
    pub body: String,
    pub raw: String,
}

pub fn parse(content: &str) -> NoteContent {
    let raw = content.to_string();

    if content.starts_with("---") {
        let without_opening = &content[3..];
        if let Some(end_idx) = without_opening.find("---") {
            let yaml_str = &without_opening[..end_idx];
            let body = without_opening[end_idx + 3..].trim_start_matches('\n').to_string();

            let frontmatter = parse_yaml(yaml_str);
            return NoteContent { frontmatter, body, raw };
        }
    }

    NoteContent {
        frontmatter: HashMap::new(),
        body: content.to_string(),
        raw,
    }
}

fn parse_yaml(yaml_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(docs) = YamlLoader::load_from_str(yaml_str) {
        if let Some(doc) = docs.first() {
            if let Some(hash) = doc.as_hash() {
                for (key, value) in hash {
                    if let (Some(k), Some(v)) = (key.as_str(), yaml_to_string(value)) {
                        map.insert(k.to_string(), v);
                    }
                }
            }
        }
    }
    map
}

fn yaml_to_string(yaml: &Yaml) -> Option<String> {
    match yaml {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(r) => Some(r.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Null => Some("".to_string()),
        Yaml::Array(arr) => {
            let items: Vec<String> = arr.iter().filter_map(yaml_to_string).collect();
            Some(items.join(", "))
        }
        _ => None,
    }
}

pub fn stringify_value(yaml: &Yaml) -> Option<String> {
    yaml_to_string(yaml)
}
```

- [ ] **Step 3: Update main.rs module declarations**

```rust
mod config;
mod parse;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo build 2>&1
```

Expected: Warning about unused `wikilink` and `tags` modules (they don't exist yet — that's fine).

- [ ] **Step 5: Commit**

```bash
git add src/parse/
git commit -m "feat: add frontmatter parser"
```

---

### Task 4: Wikilink parser

**Files:**
- Create: `src/parse/wikilink.rs`

- [ ] **Step 1: Write wikilink.rs**

```rust
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Wikilink {
    pub target: String,
    pub alias: Option<String>,
    pub raw: String,
}

pub fn extract_wikilinks(content: &str) -> Vec<Wikilink> {
    let re = Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]").unwrap();
    let mut links = Vec::new();

    for cap in re.captures_iter(content) {
        let target = cap[1].to_string();
        let alias = cap.get(2).map(|m| m.as_str().to_string());
        let raw = cap[0].to_string();
        links.push(Wikilink { target, alias, raw });
    }

    links
}

pub fn resolve_wikilink(target: &str, vault_path: &Path) -> Option<PathBuf> {
    // Try exact match first
    let direct = vault_path.join(format!("{}.md", target));
    if direct.exists() {
        return Some(direct);
    }

    // Try case-insensitive search
    let target_lower = target.to_lowercase();
    for entry in walkdir::WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == target_lower {
                return Some(entry.path().to_path_buf());
            }
        }
    }

    None
}

pub fn relative_path(path: &Path, vault_path: &Path) -> String {
    path.strip_prefix(vault_path)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/parse/wikilink.rs
git commit -m "feat: add wikilink parser with resolution"
```

---

### Task 5: Tag parser

**Files:**
- Create: `src/parse/tags.rs`

- [ ] **Step 1: Write tags.rs**

```rust
use regex::Regex;
use std::collections::HashSet;

pub fn extract_tags(content: &str) -> HashSet<String> {
    let mut tags = HashSet::new();

    // Extract from body: #tag (not preceded by word char or [[ )
    let re = Regex::new(r"(?<![#\w\[])(#[\w/-]+)").unwrap();
    for cap in re.captures_iter(content) {
        let tag = cap[1].trim_start_matches('#').to_string();
        tags.insert(tag);
    }

    tags
}

pub fn extract_tags_from_frontmatter(frontmatter: &std::collections::HashMap<String, String>) -> HashSet<String> {
    let mut tags = HashSet::new();

    if let Some(tag_str) = frontmatter.get("tags") {
        // Handle comma-separated string
        for tag in tag_str.split(',') {
            let tag = tag.trim().trim_start_matches('#').to_string();
            if !tag.is_empty() {
                tags.insert(tag);
            }
        }
    }

    if let Some(tag_str) = frontmatter.get("tag") {
        for tag in tag_str.split(',') {
            let tag = tag.trim().trim_start_matches('#').to_string();
            if !tag.is_empty() {
                tags.insert(tag);
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
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/parse/tags.rs
git commit -m "feat: add tag parser"
```

---

### Task 6: Vault core — read, write, list, search

**Files:**
- Create: `src/vault.rs`

- [ ] **Step 1: Write vault.rs**

```rust
use crate::config::Config;
use crate::parse::{frontmatter, wikilink, tags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct NoteInfo {
    pub path: String,
    pub frontmatter: HashMap<String, String>,
    pub body: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub backlinks: Vec<String>,
}

pub struct Vault {
    pub config: Config,
}

impl Vault {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn read_note(&self, note_path: &str) -> anyhow::Result<NoteInfo> {
        let full_path = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full_path)?;
        let parsed = frontmatter::parse(&content);
        let note_tags: Vec<String> = tags::all_tags(&content, &parsed.frontmatter)
            .into_iter().collect();
        let note_links: Vec<String> = wikilink::extract_wikilinks(&content)
            .iter().map(|l| l.target.clone()).collect();

        Ok(NoteInfo {
            path: wikilink::relative_path(&full_path, &self.config.vault_path),
            frontmatter: parsed.frontmatter,
            body: parsed.body,
            tags: note_tags,
            links: note_links,
            backlinks: Vec::new(),
        })
    }

    pub fn list_vault(&self, subpath: Option<&str>, depth: Option<usize>) -> anyhow::Result<Vec<String>> {
        let start = match subpath {
            Some(p) => self.config.vault_path.join(p),
            None => self.config.vault_path.clone(),
        };
        let max_depth = depth.unwrap_or(10);

        let mut entries = Vec::new();
        for entry in WalkDir::new(&start)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let rel = entry.path().strip_prefix(&self.config.vault_path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");

            if rel.is_empty() { continue; }

            if entry.file_type().is_dir() {
                entries.push(format!("{}/", rel));
            } else if entry.path().extension().is_some_and(|ext| ext == "md") {
                entries.push(rel.to_string());
            }
        }

        entries.sort();
        Ok(entries)
    }

    pub fn create_note(&self, note_path: &str, content: &str, frontmatter_fields: Option<&HashMap<String, String>>) -> anyhow::Result<NoteInfo> {
        let full_path = self.config.vault_path.join(note_path);

        if full_path.exists() {
            return Err(anyhow::anyhow!("Note already exists: {}", note_path));
        }

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file_content = build_note_content(content, frontmatter_fields);
        std::fs::write(&full_path, &file_content)?;

        self.read_note(note_path)
    }

    pub fn update_note(&self, note_path: &str, content: &str, mode: &str) -> anyhow::Result<NoteInfo> {
        let full_path = self.resolve_note_path(note_path)?;

        match mode {
            "append" => {
                let mut existing = std::fs::read_to_string(&full_path)?;
                existing.push('\n');
                existing.push_str(content);
                std::fs::write(&full_path, &existing)?;
            }
            "replace" => {
                // Preserve frontmatter if it exists
                let existing = std::fs::read_to_string(&full_path)?;
                let parsed = frontmatter::parse(&existing);
                if !parsed.frontmatter.is_empty() {
                    let fm_str = serialize_frontmatter(&parsed.frontmatter);
                    std::fs::write(&full_path, format!("---\n{}---\n{}", fm_str, content))?;
                } else {
                    std::fs::write(&full_path, content)?;
                }
            }
            _ => return Err(anyhow::anyhow!("Invalid mode: {} (use 'append' or 'replace')", mode)),
        }

        self.read_note(note_path)
    }

    pub fn set_frontmatter(&self, note_path: &str, fields: &HashMap<String, String>) -> anyhow::Result<NoteInfo> {
        let full_path = self.resolve_note_path(note_path)?;
        let content = std::fs::read_to_string(&full_path)?;
        let mut parsed = frontmatter::parse(&content);

        for (k, v) in fields {
            parsed.frontmatter.insert(k.clone(), v.clone());
        }

        let fm_str = serialize_frontmatter(&parsed.frontmatter);
        let new_content = if parsed.frontmatter.is_empty() {
            content
        } else {
            format!("---\n{}---\n{}", fm_str, parsed.body)
        };

        std::fs::write(&full_path, &new_content)?;
        self.read_note(note_path)
    }

    pub fn search_notes(&self, query: &str, limit: usize) -> anyhow::Result<Vec<NoteInfo>> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if content.to_lowercase().contains(&query_lower) {
                    let note = self.read_note(
                        &wikilink::relative_path(entry.path(), &self.config.vault_path)
                    )?;
                    results.push(note);
                    if results.len() >= limit { break; }
                }
            }
        }

        Ok(results)
    }

    pub fn search_by_tag(&self, search_tags: &[String], match_mode: &str) -> anyhow::Result<Vec<NoteInfo>> {
        let mut results = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                let note_tags = tags::all_tags(&content, &parsed.frontmatter);

                let matches = match match_mode {
                    "all" => search_tags.iter().all(|t| note_tags.contains(t.as_str())),
                    _ => search_tags.iter().any(|t| note_tags.contains(t.as_str())),
                };

                if matches {
                    let note = self.read_note(
                        &wikilink::relative_path(entry.path(), &self.config.vault_path)
                    )?;
                    results.push(note);
                }
            }
        }

        Ok(results)
    }

    pub fn search_by_frontmatter(&self, filters: &HashMap<String, String>) -> anyhow::Result<Vec<NoteInfo>> {
        let mut results = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                let matches = filters.iter().all(|(k, v)| {
                    parsed.frontmatter.get(k).map_or(false, |val| val == v)
                });

                if matches {
                    let note = self.read_note(
                        &wikilink::relative_path(entry.path(), &self.config.vault_path)
                    )?;
                    results.push(note);
                }
            }
        }

        Ok(results)
    }

    pub fn backlinks(&self, note_path: &str) -> anyhow::Result<Vec<String>> {
        let target_stem = Path::new(note_path).file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(note_path);
        let mut backlinking_notes = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let links = wikilink::extract_wikilinks(&content);
                if links.iter().any(|l| l.target == target_stem) {
                    let rel = wikilink::relative_path(entry.path(), &self.config.vault_path);
                    backlinking_notes.push(rel);
                }
            }
        }

        Ok(backlinking_notes)
    }

    pub fn get_templates_dir(&self) -> PathBuf {
        // Check common Obsidian template locations
        let templates_dir = self.config.vault_path.join("templates");
        if templates_dir.exists() { return templates_dir; }

        let templates_dir = self.config.vault_path.join("Templates");
        if templates_dir.exists() { return templates_dir; }

        // Check .obsidian for configured templates folder
        let obsidian_config = self.config.vault_path.join(".obsidian").join("app.json");
        if let Ok(config_str) = std::fs::read_to_string(&obsidian_config) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_str) {
                if let Some(folder) = config.get("attachmentFolderPath").and_then(|v| v.as_str()) {
                    let dir = self.config.vault_path.join(folder);
                    if dir.exists() { return dir; }
                }
            }
        }

        templates_dir
    }

    pub fn list_templates(&self) -> anyhow::Result<Vec<String>> {
        let dir = self.get_templates_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut templates = Vec::new();
        for entry in WalkDir::new(&dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Some(name) = entry.path().file_stem().and_then(|s| s.to_str()) {
                templates.push(name.to_string());
            }
        }

        templates.sort();
        Ok(templates)
    }

    pub fn apply_template(&self, template_name: &str, note_path: &str) -> anyhow::Result<()> {
        let templates_dir = self.get_templates_dir();
        let template_path = templates_dir.join(format!("{}.md", template_name));

        if !template_path.exists() {
            return Err(anyhow::anyhow!("Template not found: {}", template_name));
        }

        let template_content = std::fs::read_to_string(&template_path)?;
        let note_full_path = self.resolve_note_path(note_path)?;

        let existing = if note_full_path.exists() {
            std::fs::read_to_string(&note_full_path)?
        } else {
            String::new()
        };

        let parsed_existing = frontmatter::parse(&existing);
        let parsed_template = frontmatter::parse(&template_content);

        // Merge frontmatter: template defaults, existing overrides
        let mut merged_fm = parsed_template.frontmatter;
        for (k, v) in &parsed_existing.frontmatter {
            merged_fm.insert(k.clone(), v.clone());
        }

        // Use template body if note body is empty, otherwise keep existing
        let body = if parsed_existing.body.trim().is_empty() {
            parsed_template.body
        } else {
            parsed_existing.body
        };

        let fm_str = serialize_frontmatter(&merged_fm);
        let final_content = if merged_fm.is_empty() {
            body
        } else {
            format!("---\n{}---\n{}", fm_str, body)
        };

        if let Some(parent) = note_full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&note_full_path, &final_content)?;
        Ok(())
    }

    pub fn get_daily_note(&self, date: Option<&str>) -> anyhow::Result<NoteInfo> {
        let date_str = match date {
            Some(d) => d.to_string(),
            None => chrono::Local::now().format("%Y-%m-%d").to_string(),
        };

        // Check if daily note exists
        let daily_dir = self.config.vault_path.join("daily");
        let daily_dir2 = self.config.vault_path.join("Daily Notes");

        let candidates = vec![
            daily_dir.join(format!("{}.md", date_str)),
            daily_dir2.join(format!("{}.md", date_str)),
            self.config.vault_path.join(format!("{}.md", date_str)),
        ];

        for path in &candidates {
            if path.exists() {
                let rel = wikilink::relative_path(path, &self.config.vault_path);
                return self.read_note(&rel);
            }
        }

        // Create new daily note
        let target_dir = if daily_dir.exists() { &daily_dir } else if daily_dir2.exists() { &daily_dir2 } else { &daily_dir };
        std::fs::create_dir_all(target_dir)?;

        let note_path = format!("{}.md", date_str);
        let full_path = target_dir.join(&note_path);

        let content = format!("# {}\n\n", date_str);
        std::fs::write(&full_path, &content)?;

        let rel = wikilink::relative_path(&full_path, &self.config.vault_path);
        self.read_note(&rel)
    }

    fn resolve_note_path(&self, note_path: &str) -> anyhow::Result<PathBuf> {
        let full_path = self.config.vault_path.join(note_path);
        if full_path.exists() {
            return Ok(full_path);
        }

        // Try with .md extension
        let with_ext = self.config.vault_path.join(format!("{}.md", note_path));
        if with_ext.exists() {
            return Ok(with_ext);
        }

        Err(anyhow::anyhow!("Note not found: {}", note_path))
    }
}

fn build_note_content(body: &str, frontmatter_fields: Option<&HashMap<String, String>>) -> String {
    match frontmatter_fields {
        Some(fields) if !fields.is_empty() => {
            let fm_str = serialize_frontmatter(fields);
            format!("---\n{}---\n{}", fm_str, body)
        }
        _ => body.to_string(),
    }
}

fn serialize_frontmatter(fields: &HashMap<String, String>) -> String {
    let mut out = String::new();
    for (k, v) in fields {
        out.push_str(&format!("{}: {}\n", k, v));
    }
    out
}
```

- [ ] **Step 2: Update main.rs module declarations**

```rust
mod config;
mod parse;
mod vault;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src/vault.rs
git commit -m "feat: add vault core - read, write, list, search, templates, daily notes"
```

---

### Task 7: Graph module

**Files:**
- Create: `src/graph.rs`

- [ ] **Step 1: Write graph.rs**

```rust
use crate::config::Config;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct ObsidianGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f64,
}

#[derive(Debug, Clone)]
pub struct Community {
    pub id: usize,
    pub nodes: Vec<String>,
}

pub struct GraphAnalyzer {
    config: Config,
}

impl GraphAnalyzer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    fn graph_path(&self) -> PathBuf {
        self.config.vault_path.join(".obsidian").join("graph.json")
    }

    pub fn load_graph(&self) -> anyhow::Result<ObsidianGraph> {
        let path = self.graph_path();
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Graph data not available at {}. Open Obsidian once to generate it.",
                path.display()
            ));
        }
        let content = std::fs::read_to_string(&path)?;
        let graph: ObsidianGraph = serde_json::from_str(&content)?;
        Ok(graph)
    }

    pub fn stats(&self) -> anyhow::Result<GraphStats> {
        let graph = self.load_graph()?;
        let n = graph.nodes.len() as f64;
        let e = graph.edges.len() as f64;
        let max_edges = if n > 1.0 { n * (n - 1.0) / 2.0 } else { 1.0 };
        let density = e / max_edges;

        Ok(GraphStats {
            node_count: graph.nodes.len(),
            edge_count: graph.edges.len(),
            density,
        })
    }

    pub fn communities(&self) -> anyhow::Result<Vec<Community>> {
        let graph = self.load_graph()?;
        let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();

        for node in &graph.nodes {
            adjacency.entry(node.id.clone()).or_default();
        }
        for edge in &graph.edges {
            adjacency.entry(edge.source.clone()).or_default().insert(edge.target.clone());
            adjacency.entry(edge.target.clone()).or_default().insert(edge.source.clone());
        }

        // Simple connected components as community detection
        let mut visited = HashSet::new();
        let mut communities = Vec::new();
        let mut community_id = 0;

        for node in &graph.nodes {
            if visited.contains(&node.id) { continue; }

            let mut component = Vec::new();
            let mut stack = vec![node.id.clone()];

            while let Some(current) = stack.pop() {
                if visited.contains(&current) { continue; }
                visited.insert(current.clone());
                component.push(current.clone());

                if let Some(neighbors) = adjacency.get(&current) {
                    for neighbor in neighbors {
                        if !visited.contains(neighbor) {
                            stack.push(neighbor.clone());
                        }
                    }
                }
            }

            communities.push(Community {
                id: community_id,
                nodes: component,
            });
            community_id += 1;
        }

        Ok(communities)
    }

    pub fn shortest_path(&self, from: &str, to: &str) -> anyhow::Result<Vec<String>> {
        let graph = self.load_graph()?;
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for node in &graph.nodes {
            adjacency.entry(node.id.clone()).or_default();
        }
        for edge in &graph.edges {
            adjacency.entry(edge.source.clone()).or_default().push(edge.target.clone());
            adjacency.entry(edge.target.clone()).or_default().push(edge.source.clone());
        }

        // BFS
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(from.to_string());
        visited.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                let mut path = Vec::new();
                let mut node = to.to_string();
                path.push(node.clone());
                while let Some(p) = parent.get(&node) {
                    path.push(p.clone());
                    node = p.clone();
                }
                path.reverse();
                return Ok(path);
            }

            if let Some(neighbors) = adjacency.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        parent.insert(neighbor.clone(), current.clone());
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        Err(anyhow::anyhow!("No path found between {} and {}", from, to))
    }
}
```

- [ ] **Step 2: Update main.rs**

```rust
mod config;
mod parse;
mod vault;
mod graph;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src/graph.rs
git commit -m "feat: add graph analysis module"
```

---

### Task 8: MCP tools — read tools

**Files:**
- Create: `src/tools/mod.rs`
- Create: `src/tools/read.rs`

- [ ] **Step 1: Create tools module**

`src/tools/mod.rs`:

```rust
pub mod read;
pub mod search;
pub mod write;
pub mod links;
pub mod templates;
pub mod graph;
```

- [ ] **Step 2: Write read.rs**

```rust
use crate::vault::Vault;
use crate::parse::wikilink;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadNoteRequest {
    #[schemars(description = "Path to the note relative to vault root (e.g. 'folder/note.md')")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListVaultRequest {
    #[schemars(description = "Subpath to list from (optional, defaults to vault root)")]
    pub path: Option<String>,
    #[schemars(description = "Max depth to traverse (optional, defaults to 10)")]
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetMetadataRequest {
    #[schemars(description = "Path to the note relative to vault root")]
    pub path: String,
}

#[derive(Clone)]
pub struct ReadTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl ReadTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Read a note by path. Returns markdown content, parsed frontmatter, and resolved links.")]
    fn read_note(
        &self,
        Parameters(ReadNoteRequest { path }): Parameters<ReadNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "body": note.body,
                    "tags": note.tags,
                    "links": note.links,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "List vault structure — folders and notes.")]
    fn list_vault(
        &self,
        Parameters(ListVaultRequest { path, depth }): Parameters<ListVaultRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_vault(path.as_deref(), depth) {
            Ok(entries) => {
                let result = serde_json::json!({
                    "entries": entries,
                    "count": entries.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get metadata for a note: frontmatter fields, tags, outgoing links, and backlink count.")]
    fn get_metadata(
        &self,
        Parameters(GetMetadataRequest { path }): Parameters<GetMetadataRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let backlinks = self.vault.backlinks(&path).unwrap_or_default();
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "tags": note.tags,
                    "outgoing_links": note.links,
                    "backlink_count": backlinks.len(),
                    "backlinks": backlinks,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src/tools/
git commit -m "feat: add MCP read tools - read_note, list_vault, get_metadata"
```

---

### Task 9: MCP tools — search tools

**Files:**
- Create: `src/tools/search.rs`

- [ ] **Step 1: Write search.rs**

```rust
use crate::vault::Vault;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchNotesRequest {
    #[schemars(description = "Search query (full-text)")]
    pub query: String,
    #[schemars(description = "Max results to return (optional, defaults to 20)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchByTagRequest {
    #[schemars(description = "Tags to search for (e.g. ['project', 'todo'])")]
    pub tags: Vec<String>,
    #[schemars(description = "Match mode: 'any' (OR) or 'all' (AND). Defaults to 'any'.")]
    pub match_mode: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchByFrontmatterRequest {
    #[schemars(description = "Frontmatter filters as key-value pairs (e.g. {\"status\": \"done\"})")]
    pub filters: HashMap<String, String>,
}

#[derive(Clone)]
pub struct SearchTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl SearchTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Full-text search across the vault. Returns matching notes with snippets.")]
    fn search_notes(
        &self,
        Parameters(SearchNotesRequest { query, limit }): Parameters<SearchNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max = limit.unwrap_or(20);
        match self.vault.search_notes(&query, max) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "tags": n.tags,
                        "body_preview": n.body.chars().take(200).collect::<String>(),
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find notes with specific tags.")]
    fn search_by_tag(
        &self,
        Parameters(SearchByTagRequest { tags, match_mode }): Parameters<SearchByTagRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mode = match_mode.unwrap_or_else(|| "any".to_string());
        match self.vault.search_by_tag(&tags, &mode) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "tags": n.tags,
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Filter notes by frontmatter fields.")]
    fn search_by_frontmatter(
        &self,
        Parameters(SearchByFrontmatterRequest { filters }): Parameters<SearchByFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.search_by_frontmatter(&filters) {
            Ok(notes) => {
                let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                    serde_json::json!({
                        "path": n.path,
                        "frontmatter": n.frontmatter,
                    })
                }).collect();
                let result = serde_json::json!({
                    "results": results,
                    "count": results.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/search.rs
git commit -m "feat: add MCP search tools - search_notes, search_by_tag, search_by_frontmatter"
```

---

### Task 10: MCP tools — write tools

**Files:**
- Create: `src/tools/write.rs`

- [ ] **Step 1: Write write.rs**

```rust
use crate::vault::Vault;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateNoteRequest {
    #[schemars(description = "Path for the new note (relative to vault root)")]
    pub path: String,
    #[schemars(description = "Note content (markdown)")]
    pub content: Option<String>,
    #[schemars(description = "Initial frontmatter fields")]
    pub frontmatter: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateNoteRequest {
    #[schemars(description = "Path to the note to update")]
    pub path: String,
    #[schemars(description = "Content to write")]
    pub content: String,
    #[schemars(description = "'append' to add to end, 'replace' to overwrite body (preserves frontmatter)")]
    pub mode: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFrontmatterRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Frontmatter fields to set (merges with existing)")]
    pub fields: HashMap<String, String>,
}

#[derive(Clone)]
pub struct WriteTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl WriteTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Create a new note with optional content and frontmatter.")]
    fn create_note(
        &self,
        Parameters(CreateNoteRequest { path, content, frontmatter }): Parameters<CreateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = content.unwrap_or_default();
        match self.vault.create_note(&path, &body, frontmatter.as_ref()) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "message": "Note created successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Update an existing note. Use 'append' to add content or 'replace' to overwrite (preserves frontmatter).")]
    fn update_note(
        &self,
        Parameters(UpdateNoteRequest { path, content, mode }): Parameters<UpdateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.update_note(&path, &content, &mode) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "message": "Note updated successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Set frontmatter fields on a note. Merges with existing frontmatter.")]
    fn set_frontmatter(
        &self,
        Parameters(SetFrontmatterRequest { path, fields }): Parameters<SetFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.set_frontmatter(&path, &fields) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "message": "Frontmatter updated successfully",
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/write.rs
git commit -m "feat: add MCP write tools - create_note, update_note, set_frontmatter"
```

---

### Task 11: MCP tools — link tools

**Files:**
- Create: `src/tools/links.rs`

- [ ] **Step 1: Write links.rs**

```rust
use crate::vault::Vault;
use crate::parse::wikilink;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResolveLinksRequest {
    #[schemars(description = "Path to the note to resolve links from")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BacklinksRequest {
    #[schemars(description = "Path to the note to find backlinks for")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LinkGraphRequest {
    #[schemars(description = "Path to the note")]
    pub path: String,
    #[schemars(description = "Depth of link neighbors to traverse (optional, defaults to 1)")]
    pub depth: Option<usize>,
}

#[derive(Clone)]
pub struct LinkTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl LinkTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "Resolve all [[wikilinks]] in a note to their actual file paths.")]
    fn resolve_links(
        &self,
        Parameters(ResolveLinksRequest { path }): Parameters<ResolveLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&path) {
            Ok(note) => {
                let resolved: Vec<serde_json::Value> = note.links.iter().map(|link| {
                    let resolved_path = wikilink::resolve_wikilink(
                        link,
                        &self.vault.config.vault_path,
                    );
                    serde_json::json!({
                        "target": link,
                        "resolved": resolved_path.map(|p| {
                            wikilink::relative_path(&p, &self.vault.config.vault_path)
                        }),
                        "exists": resolved_path.is_some(),
                    })
                }).collect();
                let result = serde_json::json!({
                    "links": resolved,
                    "count": resolved.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find all notes that link to a given note (backlinks).")]
    fn backlinks(
        &self,
        Parameters(BacklinksRequest { path }): Parameters<BacklinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.backlinks(&path) {
            Ok(links) => {
                let result = serde_json::json!({
                    "backlinks": links,
                    "count": links.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get the link graph for a note — its outgoing links and their links, up to N hops.")]
    fn link_graph(
        &self,
        Parameters(LinkGraphRequest { path, depth }): Parameters<LinkGraphRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max_depth = depth.unwrap_or(1);
        let mut visited = std::collections::HashSet::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        fn explore(
            note_path: &str,
            depth: usize,
            max_depth: usize,
            vault: &Vault,
            visited: &mut std::collections::HashSet<String>,
            nodes: &mut Vec<serde_json::Value>,
            edges: &mut Vec<serde_json::Value>,
        ) {
            if depth > max_depth || visited.contains(note_path) { return; }
            visited.insert(note_path.to_string());

            if let Ok(note) = vault.read_note(note_path) {
                nodes.push(serde_json::json!({
                    "path": note.path,
                    "tags": note.tags,
                }));
                for link in &note.links {
                    edges.push(serde_json::json!({
                        "source": note_path,
                        "target": link,
                    }));
                    explore(link, depth + 1, max_depth, vault, visited, nodes, edges);
                }
            }
        }

        explore(&path, 0, max_depth, &self.vault, &mut visited, &mut nodes, &mut edges);

        let result = serde_json::json!({
            "nodes": nodes,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/links.rs
git commit -m "feat: add MCP link tools - resolve_links, backlinks, link_graph"
```

---

### Task 12: MCP tools — template tools

**Files:**
- Create: `src/tools/templates.rs`

- [ ] **Step 1: Write templates.rs**

```rust
use crate::vault::Vault;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyTemplateRequest {
    #[schemars(description = "Template name (without .md extension)")]
    pub template: String,
    #[schemars(description = "Path to the note to apply the template to")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDailyNoteRequest {
    #[schemars(description = "Date in YYYY-MM-DD format (optional, defaults to today)")]
    pub date: Option<String>,
}

#[derive(Clone)]
pub struct TemplateTools {
    pub vault: Arc<Vault>,
}

#[tool_router]
impl TemplateTools {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(description = "List available templates in the vault's templates folder.")]
    fn list_templates(
        &self,
        Parameters((): ()),
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_templates() {
            Ok(templates) => {
                let result = serde_json::json!({
                    "templates": templates,
                    "count": templates.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Apply a template to a note. Merges template frontmatter with existing.")]
    fn apply_template(
        &self,
        Parameters(ApplyTemplateRequest { template, path }): Parameters<ApplyTemplateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.apply_template(&template, &path) {
            Ok(()) => {
                let result = serde_json::json!({
                    "message": format!("Template '{}' applied to '{}'", template, path),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get or create today's daily note. Returns existing note or creates a new one.")]
    fn get_daily_note(
        &self,
        Parameters(GetDailyNoteRequest { date }): Parameters<GetDailyNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.get_daily_note(date.as_deref()) {
            Ok(note) => {
                let result = serde_json::json!({
                    "path": note.path,
                    "frontmatter": note.frontmatter,
                    "body": note.body,
                    "tags": note.tags,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/templates.rs
git commit -m "feat: add MCP template tools - list_templates, apply_template, get_daily_note"
```

---

### Task 13: MCP tools — graph tools

**Files:**
- Create: `src/tools/graph.rs`

- [ ] **Step 1: Write graph.rs**

```rust
use crate::graph::GraphAnalyzer;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool,
    tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphPathRequest {
    #[schemars(description = "Source note path")]
    pub from: String,
    #[schemars(description = "Target note path")]
    pub to: String,
}

#[derive(Clone)]
pub struct GraphTools {
    pub analyzer: Arc<GraphAnalyzer>,
}

#[tool_router]
impl GraphTools {
    pub fn new(analyzer: Arc<GraphAnalyzer>) -> Self {
        Self { analyzer }
    }

    #[tool(description = "Get vault graph statistics: node count, edge count, density.")]
    fn graph_stats(
        &self,
        Parameters((): ()),
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.stats() {
            Ok(stats) => {
                let result = serde_json::json!({
                    "node_count": stats.node_count,
                    "edge_count": stats.edge_count,
                    "density": stats.density,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get communities (connected components) from Obsidian's note graph.")]
    fn graph_communities(
        &self,
        Parameters((): ()),
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.communities() {
            Ok(communities) => {
                let result: Vec<serde_json::Value> = communities.iter().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "node_count": c.nodes.len(),
                        "nodes": c.nodes,
                    })
                }).collect();
                let output = serde_json::json!({
                    "communities": result,
                    "count": result.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(output.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find the shortest path between two notes in the vault graph.")]
    fn graph_path(
        &self,
        Parameters(GraphPathRequest { from, to }): Parameters<GraphPathRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.shortest_path(&from, &to) {
            Ok(path) => {
                let result = serde_json::json!({
                    "path": path,
                    "hops": path.len().saturating_sub(1),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/graph.rs
git commit -m "feat: add MCP graph tools - graph_stats, graph_communities, graph_path"
```

---

### Task 14: Wire up main.rs — assemble the server

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the complete main.rs**

```rust
mod config;
mod parse;
mod vault;
mod graph;
mod tools;

use config::Config;
use vault::Vault;
use graph::GraphAnalyzer;
use tools::{read, search, write, links, templates, graph};
use rmcp::{ServiceExt, transport::stdio};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Obsidian MCP server");

    let config = Config::from_env()?;
    tracing::info!("Vault: {}", config.vault_path.display());

    let vault = Arc::new(Vault::new(config.clone()));
    let analyzer = Arc::new(GraphAnalyzer::new(config));

    // Build the tool router by combining all tool groups
    let read_tools = read::ReadTools::new(vault.clone());
    let search_tools = search::SearchTools::new(vault.clone());
    let write_tools = write::WriteTools::new(vault.clone());
    let link_tools = links::LinkTools::new(vault.clone());
    let template_tools = templates::TemplateTools::new(vault.clone());
    let graph_tools = graph::GraphTools::new(analyzer);

    // Use the first tool router as the main one, merge others
    let service = read_tools.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo build 2>&1
```

Expected: Compilation may fail because we need to merge tool routers properly. The `rmcp` SDK may require a different approach for combining multiple tool routers. Let me fix this in the next step.

- [ ] **Step 3: Fix — if compilation fails, restructure to use a single service struct**

If the multi-router approach doesn't compile, restructure `main.rs` to use a single `ObsidianMcp` struct that delegates to sub-modules:

```rust
mod config;
mod parse;
mod vault;
mod graph;
mod tools;

use config::Config;
use vault::Vault;
use graph::GraphAnalyzer;
use rmcp::{
    handler::server::tool::Parameters,
    model::*,
    schemars, tool, tool_handler, tool_router,
    ServiceExt, ServerHandler, transport::stdio,
    handler::server::router::tool::ToolRouter,
};
use std::sync::Arc;

#[derive(Clone)]
struct ObsidianMcp {
    vault: Arc<Vault>,
    analyzer: Arc<GraphAnalyzer>,
    tool_router: ToolRouter<ObsidianMcp>,
}

#[tool_router]
impl ObsidianMcp {
    fn new(vault: Arc<Vault>, analyzer: Arc<GraphAnalyzer>) -> Self {
        Self {
            vault,
            analyzer,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Read a note by path. Returns markdown content, parsed frontmatter, and resolved links.")]
    fn read_note(
        &self,
        Parameters(req): Parameters<tools::read::ReadNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::read::read_note(&self.vault, req)
    }

    #[tool(description = "List vault structure — folders and notes.")]
    fn list_vault(
        &self,
        Parameters(req): Parameters<tools::read::ListVaultRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::read::list_vault(&self.vault, req)
    }

    // ... (all other tools follow the same pattern)
}

#[tool_handler]
impl ServerHandler for ObsidianMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Obsidian vault management MCP server. Read, write, search notes, manage links, templates, and graph analysis.".to_string()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let config = Config::from_env()?;
    tracing::info!("Vault: {}", config.vault_path.display());

    let vault = Arc::new(Vault::new(config.clone()));
    let analyzer = Arc::new(GraphAnalyzer::new(config));

    let service = ObsidianMcp::new(vault, analyzer).serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
```

Adjust the tool method signatures to match the actual rmcp patterns from the calculator example. Each tool method should accept `Parameters(RequestStruct)` and return `Result<CallToolResult, rmcp::ErrorData>`.

- [ ] **Step 4: Final compilation check**

```bash
cargo build --release 2>&1
```

Expected: Clean build.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: wire up MCP server with all tools in main.rs"
```

---

### Task 15: Integration test

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Create test vault fixture**

Create `tests/fixtures/test-vault/` with sample notes:

`tests/fixtures/test-vault/note1.md`:
```markdown
---
tags: project, test
status: active
---
# Note 1

This is test note one. It links to [[note2]] and [[note3|Note Three]].

#project #test
```

`tests/fixtures/test-vault/note2.md`:
```markdown
---
tags: test
---
# Note 2

This links back to [[note1]].
```

`tests/fixtures/test-vault/note3.md`:
```markdown
# Note 3

No frontmatter here. Links to [[note1]] and [[nonexistent]].
```

`tests/fixtures/test-vault/templates/meeting.md`:
```markdown
---
type: meeting
date: ""
attendees: ""
---
# Meeting Notes

## Agenda


## Action Items

```

- [ ] **Step 2: Write integration tests**

`tests/integration.rs`:

```rust
use std::collections::HashMap;
use std::path::PathBuf;

// Adjust paths to match your actual module structure
// These tests verify the vault and parser logic without MCP transport

#[test]
fn test_frontmatter_parsing() {
    let content = "---\ntags: project, test\nstatus: active\n---\n# Note 1\n\nBody here.";
    let parsed = obsidian_mcp::parse::frontmatter::parse(content);
    assert_eq!(parsed.frontmatter.get("tags").unwrap(), "project, test");
    assert_eq!(parsed.frontmatter.get("status").unwrap(), "active");
    assert!(parsed.body.contains("# Note 1"));
}

#[test]
fn test_wikilink_extraction() {
    let content = "Link to [[note2]] and [[note3|Note Three]].";
    let links = obsidian_mcp::parse::wikilink::extract_wikilinks(content);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target, "note2");
    assert_eq!(links[1].target, "note3");
    assert_eq!(links[1].alias, Some("Note Three".to_string()));
}

#[test]
fn test_tag_extraction() {
    let content = "Some text #project #test/tags here.";
    let tags = obsidian_mcp::parse::tags::extract_tags(content);
    assert!(tags.contains("project"));
    assert!(tags.contains("test/tags"));
}

#[test]
fn test_vault_read_note() {
    let vault_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-vault");
    let config = obsidian_mcp::config::Config { vault_path };
    let vault = obsidian_mcp::vault::Vault::new(config);

    let note = vault.read_note("note1.md").unwrap();
    assert!(note.body.contains("Note 1"));
    assert!(note.tags.contains(&"project".to_string()));
    assert!(note.links.contains(&"note2".to_string()));
}

#[test]
fn test_vault_search() {
    let vault_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-vault");
    let config = obsidian_mcp::config::Config { vault_path };
    let vault = obsidian_mcp::vault::Vault::new(config);

    let results = vault.search_notes("test note", 10).unwrap();
    assert!(results.len() >= 1);
}

#[test]
fn test_vault_create_and_read() {
    let vault_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-vault");
    let config = obsidian_mcp::config::Config { vault_path: vault_path.clone() };
    let vault = obsidian_mcp::vault::Vault::new(config);

    let mut fm = HashMap::new();
    fm.insert("status".to_string(), "new".to_string());

    let note = vault.create_note("_test-created.md", "Created by test", Some(&fm)).unwrap();
    assert_eq!(note.frontmatter.get("status").unwrap(), "new");

    // Cleanup
    let _ = std::fs::remove_file(vault_path.join("_test-created.md"));
}

#[test]
fn test_backlinks() {
    let vault_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-vault");
    let config = obsidian_mcp::config::Config { vault_path };
    let vault = obsidian_mcp::vault::Vault::new(config);

    let backlinks = vault.backlinks("note1.md").unwrap();
    assert!(backlinks.len() >= 2); // note2 and note3 both link to note1
}
```

- [ ] **Step 3: Make modules public for testing**

Ensure `src/lib.rs` or `src/main.rs` exposes the modules. Add `src/lib.rs`:

```rust
pub mod config;
pub mod parse;
pub mod vault;
pub mod graph;
pub mod tools;
```

And update `src/main.rs` to use `obsidian_mcp::` paths or keep the `mod` declarations and add `pub` to each.

- [ ] **Step 4: Run tests**

```bash
cargo test 2>&1
```

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add tests/
git commit -m "test: add integration tests for parsers and vault operations"
```

---

### Task 16: Final build + verify

- [ ] **Step 1: Build release binary**

```bash
cargo build --release 2>&1
```

- [ ] **Step 2: Verify binary exists**

```bash
ls -la target/release/obsidian-mcp.exe
```

- [ ] **Step 3: Test MCP handshake (manual)**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' | OBSIDIAN_VAULT="C:/path/to/test/vault" ./target/release/obsidian-mcp.exe
```

Expected: JSON response with server info and capabilities.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: final build verification"
```

---

## Configuration for MCP Clients

After building, configure in your MCP client (e.g. OpenCode):

```json
{
  "mcp": {
    "servers": {
      "obsidian": {
        "command": "C:/Users/Aryaman/Documents/AI Tool/Obsidian-MCP/target/release/obsidian-mcp.exe",
        "env": {
          "OBSIDIAN_VAULT": "C:/Users/Aryaman/Documents/YourVault"
        }
      }
    }
  }
}
```
