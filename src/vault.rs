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
        let templates_dir = self.config.vault_path.join("templates");
        if templates_dir.exists() { return templates_dir; }

        let templates_dir = self.config.vault_path.join("Templates");
        if templates_dir.exists() { return templates_dir; }

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

        let mut merged_fm = parsed_template.frontmatter;
        for (k, v) in &parsed_existing.frontmatter {
            merged_fm.insert(k.clone(), v.clone());
        }

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