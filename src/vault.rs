use crate::config::Config;
use crate::parse::{frontmatter, wikilink, tags, sections};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct NoteInfo {
    pub path: String,
    pub frontmatter: HashMap<String, String>,
    pub body: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BrokenLink {
    pub source: String,
    pub target: String,
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
        let note_tags: Vec<String> = tags::all_tags(&parsed.body, &parsed.frontmatter)
            .into_iter().collect();
        let note_links: Vec<String> = wikilink::extract_wikilinks(&content)
            .iter().map(|l| l.target.clone()).collect();

        Ok(NoteInfo {
            path: wikilink::relative_path(&full_path, &self.config.vault_path),
            frontmatter: parsed.frontmatter,
            body: parsed.body,
            tags: note_tags,
            links: note_links,
        })
    }

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

    pub fn list_vault(&self, subpath: Option<&str>, depth: Option<usize>) -> anyhow::Result<Vec<String>> {
        let start = match subpath {
            Some(p) => self.validate_path(p)?,
            None => self.config.vault_path.clone(),
        };
        let max_depth = depth.unwrap_or(10).min(20);

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

    pub fn create_folder(&self, folder_path: &str) -> anyhow::Result<()> {
        let vault_canonical = self.config.vault_path.canonicalize()
            .map_err(|_| anyhow::anyhow!("Vault path error"))?;

        let joined = self.config.vault_path.join(folder_path);

        if joined.exists() {
            return Err(anyhow::anyhow!("Folder already exists"));
        }

        let mut ancestor = joined.as_path();
        loop {
            if ancestor.exists() {
                let canonical = ancestor.canonicalize()
                    .map_err(|_| anyhow::anyhow!("Path resolution error"))?;
                if !canonical.starts_with(&vault_canonical) {
                    return Err(anyhow::anyhow!("Access denied: path outside vault"));
                }
                break;
            }
            ancestor = ancestor.parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
        }

        std::fs::create_dir_all(&joined)?;
        Ok(())
    }

    pub fn create_note(&self, note_path: &str, content: &str, frontmatter_fields: Option<&HashMap<String, String>>) -> anyhow::Result<NoteInfo> {
        let full_path = self.validate_parent(note_path)?;

        if full_path.exists() {
            return Err(anyhow::anyhow!("Note already exists"));
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

    pub fn update_section(&self, note_path: &str, heading: &str, content: &str, mode: &str) -> anyhow::Result<NoteInfo> {
        if mode != "append" && mode != "replace" {
            return Err(anyhow::anyhow!("Invalid mode: {} (use 'append' or 'replace')", mode));
        }

        if !heading.trim_start().starts_with('#') {
            return Err(anyhow::anyhow!("Heading must include '#' markers (e.g. '## Tasks')"));
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
                let mut heading_line = parsed.body[section.start..heading_line_end].to_string();

                if mode == "replace" {
                    if !heading_line.ends_with('\n') {
                        heading_line.push('\n');
                    }
                    format!(
                        "{}{}{}{}",
                        &parsed.body[..section.start],
                        heading_line,
                        content.trim_end(),
                        if parsed.body[section.end..].is_empty() { "\n".to_string() } else { format!("\n{}", &parsed.body[section.end..]) }
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
        // Tags are conventionally written with a leading '#' and are
        // case-insensitive in Obsidian, but are stored internally without
        // '#' and in their original case. Normalize the query the same way
        // note tags are normalized so lookups aren't silently empty.
        let normalized_search_tags: Vec<String> = search_tags.iter()
            .map(|t| t.trim().trim_start_matches('#').to_lowercase())
            .collect();

        let mut results = Vec::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                let note_tags: std::collections::HashSet<String> = tags::all_tags(&parsed.body, &parsed.frontmatter)
                    .into_iter()
                    .map(|t| t.to_lowercase())
                    .collect();

                let matches = match match_mode {
                    "all" => normalized_search_tags.iter().all(|t| note_tags.contains(t)),
                    _ => normalized_search_tags.iter().any(|t| note_tags.contains(t)),
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

    pub fn list_tags(&self) -> anyhow::Result<Vec<(String, usize)>> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                for tag in tags::all_tags(&parsed.body, &parsed.frontmatter) {
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

    pub fn rename_note(&self, source: &str, dest: &str) -> anyhow::Result<NoteInfo> {
        let source_full = self.resolve_note_path(source)?;
        let source_stem = source_full.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let source_rel = wikilink::relative_path(&source_full, &self.config.vault_path);

        let dest_full = self.validate_parent(dest)?;
        if dest_full.exists() {
            return Err(anyhow::anyhow!("Destination already exists"));
        }

        if let Some(parent) = dest_full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&source_full, &dest_full)?;

        let new_target = dest.trim_end_matches(".md").to_string();

        let backlinks = self.backlinks(&source_rel)?;
        for bl_path in &backlinks {
            if let Ok(full_path) = self.resolve_note_path(bl_path) {
                if let Ok(content) = std::fs::read_to_string(&full_path) {
                    let links = wikilink::extract_wikilinks(&content);
                    let mut new_content = content.clone();
                    let mut changed = false;
                    for link in &links {
                        let link_stem = Path::new(&link.target).file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&link.target);
                        if link_stem == source_stem {
                            let replacement = if let Some(alias) = &link.alias {
                                format!("[[{}|{}]]", new_target, alias)
                            } else {
                                format!("[[{}]]", new_target)
                            };
                            new_content = new_content.replace(&link.raw, &replacement);
                            changed = true;
                        }
                    }
                    if changed {
                        std::fs::write(&full_path, &new_content)?;
                    }
                }
            }
        }

        self.read_note(dest)
    }

    pub fn merge_notes(&self, source: &str, dest: &str) -> anyhow::Result<NoteInfo> {
        let source_full = self.resolve_note_path(source)?;
        let source_name = source_full.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source")
            .to_string();
        let source_body = {
            let content = std::fs::read_to_string(&source_full)?;
            frontmatter::parse(&content).body
        };

        let dest_full = self.resolve_note_path(dest)?;
        let dest_rel = wikilink::relative_path(&dest_full, &self.config.vault_path);
        let dest_content = std::fs::read_to_string(&dest_full)?;

        let merged = format!(
            "{}\n\n## Merged from {}\n\n{}",
            dest_content.trim_end(),
            source_name,
            source_body.trim()
        );
        std::fs::write(&dest_full, &merged)?;
        std::fs::remove_file(&source_full)?;

        self.read_note(&dest_rel)
    }

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

    pub fn bulk_tag(&self, query: &str, add_tags: &[String], remove_tags: &[String]) -> anyhow::Result<usize> {
        let notes = self.search_notes(query, 1000)?;
        let mut count = 0;

        for note in &notes {
            if let Ok(full_path) = self.resolve_note_path(&note.path) {
                if let Ok(content) = std::fs::read_to_string(&full_path) {
                    let mut parsed = frontmatter::parse(&content);
                    let fm_changed = update_frontmatter_tags(&mut parsed.frontmatter, add_tags, remove_tags);
                    let (new_body, body_changed) = tags::remove_inline_tags(&parsed.body, remove_tags);
                    if fm_changed || body_changed {
                        let fm_str = serialize_frontmatter(&parsed.frontmatter);
                        let new_content = if parsed.frontmatter.is_empty() {
                            new_body
                        } else {
                            format!("---\n{}---\n{}", fm_str, new_body)
                        };
                        std::fs::write(&full_path, &new_content)?;
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    pub fn link_related_notes(&self, note_path: &str) -> anyhow::Result<NoteInfo> {
        let note = self.read_note(note_path)?;
        let full_path = self.resolve_note_path(note_path)?;

        let significant = extract_significant_words(&note.body, 10);
        if significant.is_empty() {
            return Ok(note);
        }

        let source_key = note_path.trim_end_matches(".md");
        let mut related: Vec<(String, usize)> = Vec::new();
        for entry in WalkDir::new(&self.config.vault_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            let rel = wikilink::relative_path(entry.path(), &self.config.vault_path);
            if rel.trim_end_matches(".md") == source_key {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let parsed = frontmatter::parse(&content);
                let content_lower = parsed.body.to_lowercase();
                let score: usize = significant.iter()
                    .filter(|word| content_lower.contains(word.as_str()))
                    .count();
                if score > 0 {
                    let target = rel.strip_suffix(".md").unwrap_or(&rel).to_string();
                    related.push((target, score));
                }
            }
        }

        related.sort_by(|a, b| b.1.cmp(&a.1));
        related.truncate(5);

        if related.is_empty() {
            return Ok(note);
        }

        let mut body = note.body.trim().to_string();
        if !body.contains("## Related") {
            body.push_str("\n\n## Related\n\n");
            for (target, _) in &related {
                body.push_str(&format!("- [[{}]]\n", target));
            }

            let fm_str = serialize_frontmatter(&note.frontmatter);
            let new_content = if note.frontmatter.is_empty() {
                body
            } else {
                format!("---\n{}---\n{}", fm_str, body)
            };

            std::fs::write(&full_path, &new_content)?;
        }

        self.read_note(note_path)
    }

    pub fn get_templates_dir(&self) -> PathBuf {
        let templates_dir = self.config.vault_path.join("templates");
        if templates_dir.exists() { return templates_dir; }

        let templates_dir = self.config.vault_path.join("Templates");
        if templates_dir.exists() { return templates_dir; }

        let obsidian_config = self.config.vault_path.join(".obsidian").join("templates.json");
        if let Ok(config_str) = std::fs::read_to_string(&obsidian_config) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_str) {
                if let Some(folder) = config.get("folder").and_then(|v| v.as_str()) {
                    if let Ok(dir) = self.validate_path(folder) {
                        if dir.exists() { return dir; }
                    }
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
        let canonical_template = template_path.canonicalize()
            .map_err(|_| anyhow::anyhow!("Template not found"))?;
        let canonical_templates_dir = templates_dir.canonicalize()
            .map_err(|_| anyhow::anyhow!("Templates directory error"))?;
        if !canonical_template.starts_with(&canonical_templates_dir) {
            return Err(anyhow::anyhow!("Access denied: template path outside templates directory"));
        }

        let template_content = std::fs::read_to_string(&canonical_template)?;
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
            Some(d) => {
                if !d.chars().all(|c| c.is_ascii_digit() || c == '-') || d.len() != 10 {
                    return Err(anyhow::anyhow!("Invalid date format: use YYYY-MM-DD"));
                }
                let parts: Vec<&str> = d.split('-').collect();
                if parts.len() != 3
                    || parts[0].len() != 4
                    || parts[1].len() != 2
                    || parts[2].len() != 2
                {
                    return Err(anyhow::anyhow!("Invalid date format: use YYYY-MM-DD"));
                }
                let year: i32 = parts[0].parse().unwrap_or(0);
                let month: u32 = parts[1].parse().unwrap_or(0);
                let day: u32 = parts[2].parse().unwrap_or(0);
                if !(1..=12).contains(&month) || !(1..=31).contains(&day) || year < 1900 {
                    return Err(anyhow::anyhow!("Invalid date: out of range"));
                }
                d.to_string()
            }
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

    fn validate_path(&self, user_path: &str) -> anyhow::Result<PathBuf> {
        let vault_canonical = self.config.vault_path.canonicalize()
            .map_err(|_| anyhow::anyhow!("Vault path error"))?;

        let joined = self.config.vault_path.join(user_path);
        if let Ok(canonical) = joined.canonicalize() {
            if !canonical.starts_with(&vault_canonical) {
                return Err(anyhow::anyhow!("Access denied: path outside vault"));
            }
            return Ok(canonical);
        }

        let safe = self.resolve_new_path(&joined, &vault_canonical)?;
        Ok(safe)
    }

    fn validate_parent(&self, user_path: &str) -> anyhow::Result<PathBuf> {
        let vault_canonical = self.config.vault_path.canonicalize()
            .map_err(|_| anyhow::anyhow!("Vault path error"))?;

        let joined = self.config.vault_path.join(user_path);
        self.resolve_new_path(&joined, &vault_canonical)
    }

    fn resolve_new_path(&self, joined: &PathBuf, vault_canonical: &PathBuf) -> anyhow::Result<PathBuf> {
        let parent = joined.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;

        let canonical_parent = parent.canonicalize()
            .map_err(|_| anyhow::anyhow!("Parent directory not found"))?;

        if !canonical_parent.starts_with(vault_canonical) {
            return Err(anyhow::anyhow!("Access denied: path outside vault"));
        }

        let filename = joined.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;

        let safe = canonical_parent.join(filename);
        Ok(safe)
    }

    fn resolve_note_path(&self, note_path: &str) -> anyhow::Result<PathBuf> {
        let full_path = self.validate_path(note_path)?;
        if full_path.exists() {
            return Ok(full_path);
        }

        let with_ext = self.validate_path(&format!("{}.md", note_path))?;
        if with_ext.exists() {
            return Ok(with_ext);
        }

        Err(anyhow::anyhow!("Note not found"))
    }
}

fn update_frontmatter_tags(fm: &mut HashMap<String, String>, add_tags: &[String], remove_tags: &[String]) -> bool {
    if add_tags.is_empty() && remove_tags.is_empty() {
        return false;
    }

    let mut changed = false;
    let tag_str = fm.entry("tags".to_string()).or_default();
    let mut tags: Vec<String> = tag_str.split(',')
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();

    for tag in add_tags {
        let tag = tag.trim().trim_start_matches('#').to_string();
        if !tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
            tags.push(tag);
            changed = true;
        }
    }

    for tag in remove_tags {
        let tag = tag.trim().trim_start_matches('#').to_string();
        if let Some(pos) = tags.iter().position(|t| t.eq_ignore_ascii_case(&tag)) {
            tags.remove(pos);
            changed = true;
        }
    }

    if changed {
        *tag_str = tags.join(", ");
    }

    changed
}

fn extract_significant_words(text: &str, max_words: usize) -> Vec<String> {
    let stop_words = [
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had",
        "her", "was", "one", "our", "out", "has", "have", "been", "some", "same",
        "into", "than", "that", "them", "then", "they", "this", "just", "also",
        "with", "very", "what", "when", "from", "their", "there", "which",
        "about", "would", "could", "should", "other", "after", "first",
        "where", "these", "those", "being", "while", "over", "such", "each",
        "like", "well", "make", "made", "much", "more", "most", "many",
    ];
    let mut counts: HashMap<String, usize> = HashMap::new();
    for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.len() >= 3 && !stop_words.contains(&word) {
            *counts.entry(word.to_string()).or_insert(0) += 1;
        }
    }

    // Rank by frequency (most mentioned = most significant), breaking ties
    // alphabetically for determinism. Sorting alphabetically first and then
    // truncating would keep whichever words happen to start with 'a'-'j'
    // regardless of how central they are to the note.
    let mut words: Vec<(String, usize)> = counts.into_iter().collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    words.truncate(max_words);
    words.into_iter().map(|(w, _)| w).collect()
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