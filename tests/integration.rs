use obsidian_mcp::parse::frontmatter::FrontmatterValue;
use std::collections::HashMap;
use std::path::PathBuf;

fn test_vault_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-vault")
}

fn test_config() -> obsidian_mcp::config::Config {
    obsidian_mcp::config::Config { vault_path: test_vault_path() }
}

#[test]
fn test_frontmatter_parsing() {
    let content = "---\ntags: project, test\nstatus: active\n---\n# Note 1\n\nBody here.";
    let parsed = obsidian_mcp::parse::frontmatter::parse(content);
    assert_eq!(parsed.frontmatter.get("tags").unwrap(), "project, test");
    assert_eq!(parsed.frontmatter.get("status").unwrap(), "active");
    assert!(parsed.body.contains("# Note 1"));
}

#[test]
fn test_frontmatter_rejects_yaml_anchors_and_aliases() {
    // Regression test for a confirmed DoS: a small anchor/alias-based YAML
    // payload amplifies into millions of elements ("billion laughs").
    // Frontmatter containing anchors/aliases must be treated as absent
    // (same fallback as oversized frontmatter), not parsed.
    let content = "---\na: &a [\"x\",\"x\"]\nb: [*a,*a]\n---\n# Note\n\nBody.";
    let parsed = obsidian_mcp::parse::frontmatter::parse(content);
    assert!(parsed.frontmatter.is_empty(), "anchor/alias frontmatter should be rejected, not parsed: {:?}", parsed.frontmatter);
}

#[test]
fn test_frontmatter_anchor_alias_amplification_is_bounded() {
    // Empirical regression test: the exact payload shape that measured
    // 5.8s / 10^7 elements before the fix must now return near-instantly,
    // because it's rejected before ever reaching YamlLoader.
    let mut yaml = String::new();
    yaml.push_str("a: &a [\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\"]\n");
    let mut prev = 'a';
    for cur in ['b', 'c', 'd', 'e', 'f', 'g'] {
        yaml.push_str(&format!(
            "{}: &{} [*{},*{},*{},*{},*{},*{},*{},*{},*{},*{}]\n",
            cur, cur, prev, prev, prev, prev, prev, prev, prev, prev, prev, prev
        ));
        prev = cur;
    }
    assert!(yaml.len() < 8192);
    let content = format!("---\n{}---\n# Body\n", yaml);

    let start = std::time::Instant::now();
    let parsed = obsidian_mcp::parse::frontmatter::parse(&content);
    let elapsed = start.elapsed();

    assert!(parsed.frontmatter.is_empty());
    assert!(elapsed.as_millis() < 500, "amplification payload took {:?}, expected near-instant rejection", elapsed);
}

#[test]
fn test_frontmatter_no_frontmatter() {
    let content = "# Just a heading\n\nNo frontmatter.";
    let parsed = obsidian_mcp::parse::frontmatter::parse(content);
    assert!(parsed.frontmatter.is_empty());
    assert!(parsed.body.contains("# Just a heading"));
}

#[test]
fn test_wikilink_extraction() {
    let content = "Link to [[note2]] and [[note3|Note Three]].";
    let links = obsidian_mcp::parse::wikilink::extract_wikilinks(content);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target, "note2");
    assert_eq!(links[0].alias, None);
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
fn test_tag_extraction_from_frontmatter() {
    let mut fm = HashMap::new();
    fm.insert("tags".to_string(), FrontmatterValue::String("alpha, beta".to_string()));
    let tags = obsidian_mcp::parse::tags::extract_tags_from_frontmatter(&fm);
    assert!(tags.contains("alpha"));
    assert!(tags.contains("beta"));
}

#[test]
fn test_tag_extraction_from_frontmatter_list() {
    // A real YAML sequence (`tags: [alpha, beta]`) must yield one tag per
    // element, without comma-splitting each element as the scalar form does.
    let mut fm = HashMap::new();
    fm.insert("tags".to_string(), FrontmatterValue::List(vec!["alpha".to_string(), "beta".to_string()]));
    let tags = obsidian_mcp::parse::tags::extract_tags_from_frontmatter(&fm);
    assert!(tags.contains("alpha"));
    assert!(tags.contains("beta"));
}

#[test]
fn test_vault_read_note() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let note = vault.read_note("note1.md").unwrap();
    assert!(note.body.contains("Note 1"));
    assert!(note.tags.contains(&"project".to_string()));
    assert!(note.tags.contains(&"test".to_string()));
    assert!(note.links.contains(&"note2".to_string()));
    assert!(note.links.contains(&"note3".to_string()));
}

#[test]
fn test_vault_read_note_without_extension() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let note = vault.read_note("note1").unwrap();
    assert!(note.body.contains("Note 1"));
}

#[test]
fn test_vault_list_vault() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let entries = vault.list_vault(None, None).unwrap();
    assert!(entries.iter().any(|e| e.contains("note1")));
    assert!(entries.iter().any(|e| e.contains("note2")));
    assert!(entries.iter().any(|e| e.contains("note3")));
}

#[test]
fn test_vault_search() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let results = vault.search_notes("test note", 10).unwrap();
    assert!(results.len() >= 1);
    assert!(results.iter().any(|n| n.body.contains("test note one")));
}

#[test]
fn test_vault_search_by_tag() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let results = vault.search_by_tag(&["project".to_string()], "any").unwrap();
    assert!(results.len() >= 1);
    assert!(results.iter().any(|n| n.path.contains("note1")));
}

#[test]
fn test_vault_search_by_frontmatter() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let mut filters = HashMap::new();
    filters.insert("status".to_string(), "active".to_string());
    let results = vault.search_by_frontmatter(&filters).unwrap();
    assert!(results.len() >= 1);
    assert!(results.iter().any(|n| n.path.contains("note1")));
}

#[test]
fn test_vault_create_and_read() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    // Ensure clean state
    let _ = std::fs::remove_file(test_vault_path().join("_test-created.md"));

    let mut fm = HashMap::new();
    fm.insert("status".to_string(), FrontmatterValue::String("new".to_string()));

    let note = vault.create_note("_test-created.md", "Created by test", Some(&fm)).unwrap();
    assert_eq!(note.frontmatter.get("status").unwrap(), "new");
    assert!(note.body.contains("Created by test"));

    // Read it back
    let read_back = vault.read_note("_test-created.md").unwrap();
    assert_eq!(read_back.frontmatter.get("status").unwrap(), "new");

    // Cleanup
    let _ = std::fs::remove_file(test_vault_path().join("_test-created.md"));
}

#[test]
fn test_vault_create_note_writes_array_frontmatter_as_yaml_sequence() {
    // Regression test: `{"tags": ["mba", "index", "home"]}` used to be
    // rejected by the schema (forced to a string), and even a
    // caller-joined `"mba, index, home"` string got written as a scalar,
    // not a YAML sequence — which Obsidian's Properties panel reads as one
    // invalid tag literal full of commas.
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-array-fm.md"));

    let mut fm = HashMap::new();
    fm.insert("tags".to_string(), FrontmatterValue::List(vec![
        "mba".to_string(), "index".to_string(), "home".to_string(),
    ]));

    let note = vault.create_note("_test-array-fm.md", "Body", Some(&fm)).unwrap();
    assert_eq!(
        note.frontmatter.get("tags"),
        Some(&FrontmatterValue::List(vec!["mba".to_string(), "index".to_string(), "home".to_string()])),
    );

    // The file on disk must contain a real YAML sequence, not a
    // comma-joined scalar.
    let raw = std::fs::read_to_string(test_vault_path().join("_test-array-fm.md")).unwrap();
    assert!(raw.contains("tags:\n  - mba\n  - index\n  - home\n"), "expected a YAML sequence, got:\n{}", raw);
    assert!(!raw.contains("tags: mba, index, home"), "tags were written as a joined scalar, not a sequence:\n{}", raw);

    // And it round-trips back to the same list on read.
    let read_back = vault.read_note("_test-array-fm.md").unwrap();
    assert_eq!(
        read_back.frontmatter.get("tags"),
        Some(&FrontmatterValue::List(vec!["mba".to_string(), "index".to_string(), "home".to_string()])),
    );
    assert!(read_back.tags.contains(&"mba".to_string()));
    assert!(read_back.tags.contains(&"index".to_string()));
    assert!(read_back.tags.contains(&"home".to_string()));

    let _ = std::fs::remove_file(test_vault_path().join("_test-array-fm.md"));
}

#[test]
fn test_vault_write_tools_reject_dotfile_paths() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    // A note-CRUD tool has no legitimate reason to target a vault
    // config/state file — must be rejected, not silently allowed through
    // the vault-boundary check (which only guards against escaping the
    // vault, not against targeting internal dotfiles within it).
    let err = vault.create_note(".obsidian/graph.json", "{}", None).unwrap_err();
    assert!(err.to_string().contains("Access denied"), "expected rejection, got: {}", err);

    let err2 = vault.read_note(".trash/whatever.md");
    assert!(err2.is_err());

    // A leading "./" is a normal relative-path spelling and must NOT be
    // mistaken for a dotfile component.
    let _ = std::fs::remove_file(test_vault_path().join("_test-dotslash.md"));
    let created = vault.create_note("./_test-dotslash.md", "fine", None);
    assert!(created.is_ok(), "a leading './' should not be rejected: {:?}", created.err());
    let _ = std::fs::remove_file(test_vault_path().join("_test-dotslash.md"));
}

#[test]
fn test_vault_write_tools_reject_empty_path() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let err = vault.create_note("", "content", None).unwrap_err();
    assert!(err.to_string().contains("empty"), "expected an empty-path error, got: {}", err);
}

#[test]
fn test_vault_create_folder_nested() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_dir_all(test_vault_path().join("_test-nested-a"));

    // None of the intermediate directories exist yet — create_folder must
    // still support deep creation (this is the behavior validate_parent
    // deliberately does NOT support, since it requires an existing single
    // parent; create_folder needs its own ancestor-walking validator).
    vault.create_folder("_test-nested-a/b/c").unwrap();
    assert!(test_vault_path().join("_test-nested-a/b/c").is_dir());

    let _ = std::fs::remove_dir_all(test_vault_path().join("_test-nested-a"));
}

#[test]
fn test_vault_update_note_append() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    // Create a test note
    let _ = vault.create_note("_test-update.md", "Original content", None);

    // Append
    let updated = vault.update_note("_test-update.md", "Appended content", "append").unwrap();
    assert!(updated.body.contains("Original content"));
    assert!(updated.body.contains("Appended content"));

    // Cleanup
    let _ = std::fs::remove_file(test_vault_path().join("_test-update.md"));
}

#[test]
fn test_vault_backlinks() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let backlinks = vault.backlinks("note1.md").unwrap();
    assert!(backlinks.len() >= 2); // note2 and note3 both link to note1
    assert!(backlinks.iter().any(|b| b.contains("note2")));
    assert!(backlinks.iter().any(|b| b.contains("note3")));
}

#[test]
fn test_vault_set_frontmatter() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = vault.create_note("_test-fm.md", "Test content", None);

    let mut fm = HashMap::new();
    fm.insert("priority".to_string(), FrontmatterValue::String("high".to_string()));
    let updated = vault.set_frontmatter("_test-fm.md", &fm).unwrap();
    assert_eq!(updated.frontmatter.get("priority").unwrap(), "high");

    // Cleanup
    let _ = std::fs::remove_file(test_vault_path().join("_test-fm.md"));
}

#[test]
fn test_vault_list_templates() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let templates = vault.list_templates().unwrap();
    assert!(templates.contains(&"meeting".to_string()));
}

#[test]
fn test_vault_rename_note() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-rename-source.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-rename-dest.md"));

    let _ = vault.create_note("_test-rename-source.md", "Content to rename", None);

    let renamed = vault.rename_note("_test-rename-source.md", "_test-rename-dest.md").unwrap();
    assert!(renamed.body.contains("Content to rename"));
    assert!(!test_vault_path().join("_test-rename-source.md").exists());
    assert!(test_vault_path().join("_test-rename-dest.md").exists());

    let _ = std::fs::remove_file(test_vault_path().join("_test-rename-dest.md"));
}

#[test]
fn test_vault_rename_updates_backlinks() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-bl-source.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-bl-linker.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-bl-renamed.md"));

    vault.create_note("_test-bl-source.md", "Source note", None).unwrap();
    vault.create_note("_test-bl-linker.md", "Links to [[_test-bl-source]]", None).unwrap();

    vault.rename_note("_test-bl-source.md", "_test-bl-renamed.md").unwrap();

    let linker = vault.read_note("_test-bl-linker.md").unwrap();
    assert!(linker.body.contains("[[_test-bl-renamed]]"), "Backlink not updated: {}", linker.body);
    assert!(!linker.body.contains("[[_test-bl-source]]"), "Old link still present");

    let _ = std::fs::remove_file(test_vault_path().join("_test-bl-renamed.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-bl-linker.md"));
}

#[test]
fn test_vault_merge_notes() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-source.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-dest.md"));

    vault.create_note("_test-merge-source.md", "Content from source", None).unwrap();
    vault.create_note("_test-merge-dest.md", "Content from dest", None).unwrap();

    let merged = vault.merge_notes("_test-merge-source.md", "_test-merge-dest.md").unwrap();
    assert!(merged.body.contains("Content from dest"));
    assert!(merged.body.contains("Content from source"));
    assert!(merged.body.contains("Merged from _test-merge-source"));
    assert!(!test_vault_path().join("_test-merge-source.md").exists());

    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-dest.md"));
}

#[test]
fn test_vault_merge_notes_rejects_self_merge() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-self.md"));

    vault.create_note("_test-merge-self.md", "Original content", None).unwrap();

    let err = vault.merge_notes("_test-merge-self.md", "_test-merge-self.md").unwrap_err();
    assert!(err.to_string().contains("same"), "expected a same-file error, got: {}", err);

    // The note must survive untouched.
    let note = vault.read_note("_test-merge-self.md").unwrap();
    assert!(note.body.contains("Original content"));

    // Also verify the "same note, different spelling" case (with/without .md).
    let err2 = vault.merge_notes("_test-merge-self.md", "_test-merge-self").unwrap_err();
    assert!(err2.to_string().contains("same"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-self.md"));
}

#[test]
fn test_vault_merge_notes_redirects_backlinks() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-bl-source.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-bl-dest.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-bl-linker.md"));

    vault.create_note("_test-merge-bl-source.md", "Source content", None).unwrap();
    vault.create_note("_test-merge-bl-dest.md", "Dest content", None).unwrap();
    vault.create_note("_test-merge-bl-linker.md", "Links to [[_test-merge-bl-source]]", None).unwrap();

    vault.merge_notes("_test-merge-bl-source.md", "_test-merge-bl-dest.md").unwrap();

    let linker = vault.read_note("_test-merge-bl-linker.md").unwrap();
    assert!(linker.body.contains("[[_test-merge-bl-dest]]"), "backlink was not redirected: {:?}", linker.body);
    assert!(!linker.body.contains("_test-merge-bl-source"), "dangling reference to merged-away source remains: {:?}", linker.body);

    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-bl-dest.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-merge-bl-linker.md"));
}

#[test]
fn test_vault_bulk_tag() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-bt-note.md"));

    vault.create_note("_test-bt-note.md", "bulk-taggable content", None).unwrap();

    let count = vault.bulk_tag("bulk-taggable", &["new-tag".to_string()], &[]).unwrap();
    assert_eq!(count, 1);

    let note = vault.read_note("_test-bt-note.md").unwrap();
    // bulk_tag now writes tags as a real YAML sequence, not a comma-joined
    // scalar (see test_vault_create_note_writes_array_frontmatter_as_yaml_sequence).
    assert_eq!(note.frontmatter.get("tags"), Some(&FrontmatterValue::List(vec!["new-tag".to_string()])));

    let _ = std::fs::remove_file(test_vault_path().join("_test-bt-note.md"));
}

#[test]
fn test_vault_link_related_notes() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-lr-main.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-lr-related.md"));

    vault.create_note("_test-lr-main.md",
        "This note discusses machine learning and artificial intelligence.", None).unwrap();
    vault.create_note("_test-lr-related.md",
        "Another note about machine learning topics and artificial intelligence models.", None).unwrap();

    let linked = vault.link_related_notes("_test-lr-main.md").unwrap();
    assert!(linked.body.contains("## Related"));
    assert!(linked.body.contains("_test-lr-related"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-lr-main.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_test-lr-related.md"));
}

#[test]
fn test_vault_resolve_links() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let note = vault.read_note("note1.md").unwrap();
    assert_eq!(note.links.len(), 2);

    // Resolve each link
    for link in &note.links {
        let resolved = obsidian_mcp::parse::wikilink::resolve_wikilink(
            link,
            &test_vault_path(),
        );
        assert!(resolved.is_some(), "Failed to resolve link: {}", link);
    }
}

#[test]
fn test_search_by_tag_ignores_hash_prefix() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let results = vault.search_by_tag(&["#project".to_string()], "any").unwrap();
    assert!(results.iter().any(|n| n.path.contains("note1")));
}

#[test]
fn test_search_by_tag_case_insensitive() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let results = vault.search_by_tag(&["Project".to_string()], "any").unwrap();
    assert!(results.iter().any(|n| n.path.contains("note1")));
}

#[test]
fn test_bulk_tag_remove_is_case_insensitive() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-bt-case-note.md"));

    vault.create_note("_test-bt-case-note.md", "bt-case-taggable content", None).unwrap();
    vault.bulk_tag("bt-case-taggable", &["Project".to_string()], &[]).unwrap();
    vault.bulk_tag("bt-case-taggable", &[], &["project".to_string()]).unwrap();

    let note = vault.read_note("_test-bt-case-note.md").unwrap();
    assert!(note.frontmatter.get("tags").map_or(true, |t| match t {
        FrontmatterValue::String(s) => s.is_empty(),
        FrontmatterValue::List(items) => items.is_empty(),
    }));

    let _ = std::fs::remove_file(test_vault_path().join("_test-bt-case-note.md"));
}

#[test]
fn test_link_related_notes_finds_less_common_keyword() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_repro-main.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_repro-related.md"));

    // 10 filler words a-j (each mentioned once) sort alphabetically before
    // the real topical keyword "zephyr" (mentioned repeatedly, as a note's
    // actual subject would be), which previously got truncated out of
    // significance ranking purely by alphabetical bad luck before overlap
    // scoring ever ran.
    let filler = "apple banana cherry dolphin eagle falcon giraffe hedgehog iguana jellyfish";
    vault.create_note("_repro-main.md",
        &format!("{} zephyr. Zephyr is the topic. This note is about zephyr.", filler), None).unwrap();
    vault.create_note("_repro-related.md",
        "This note is entirely about zephyr and nothing else relevant.", None).unwrap();

    let linked = vault.link_related_notes("_repro-main.md").unwrap();
    assert!(linked.body.contains("## Related"));
    assert!(linked.body.contains("_repro-related"));

    let _ = std::fs::remove_file(test_vault_path().join("_repro-main.md"));
    let _ = std::fs::remove_file(test_vault_path().join("_repro-related.md"));
}

// --- Tag extraction correctness (false positives from non-tag '#' usage) ---

#[test]
fn test_tag_extraction_ignores_wikilink_heading_refs() {
    let content = "See [[Project Plan#Milestones]] for details.";
    let tags = obsidian_mcp::parse::tags::extract_tags(content);
    assert!(!tags.contains("Milestones"), "wikilink heading ref falsely extracted as tag: {:?}", tags);
}

#[test]
fn test_tag_extraction_ignores_url_anchors() {
    let content = "Source: https://example.com/page#section-two";
    let tags = obsidian_mcp::parse::tags::extract_tags(content);
    assert!(!tags.contains("section-two"), "URL anchor falsely extracted as tag: {:?}", tags);
}

#[test]
fn test_tag_extraction_ignores_code_blocks() {
    let content = "Inline `#ffffff` color and:\n```\n#include <stdio.h>\n```\nreal #tag here.";
    let tags = obsidian_mcp::parse::tags::extract_tags(content);
    assert!(!tags.contains("ffffff"), "inline code hash falsely extracted as tag: {:?}", tags);
    assert!(!tags.contains("include"), "fenced code hash falsely extracted as tag: {:?}", tags);
    assert!(tags.contains("tag"), "real tag missed: {:?}", tags);
}

#[test]
fn test_tag_extraction_rejects_numeric_only_tags() {
    let content = "Filed under #2024 and #project2024.";
    let tags = obsidian_mcp::parse::tags::extract_tags(content);
    assert!(!tags.contains("2024"), "purely numeric tag should be invalid: {:?}", tags);
    assert!(tags.contains("project2024"));
}

#[test]
fn test_vault_read_note_does_not_leak_frontmatter_hash_as_tag() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-fm-hash-leak.md"));

    vault.create_note(
        "_test-fm-hash-leak.md",
        "---\nsource: \"https://example.com/x#leaked-tag\"\n---\n# Note\n\nBody text.",
        None,
    ).unwrap();
    let note = vault.read_note("_test-fm-hash-leak.md").unwrap();
    assert!(!note.tags.contains(&"leaked-tag".to_string()), "frontmatter text leaked into tags: {:?}", note.tags);

    let _ = std::fs::remove_file(test_vault_path().join("_test-fm-hash-leak.md"));
}

#[test]
fn test_bulk_tag_remove_strips_inline_body_tag() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-remove-body-tag.md"));

    vault.create_note(
        "_test-remove-body-tag.md",
        "remove-body-tag-content #legacytag in the body.",
        None,
    ).unwrap();
    let before = vault.read_note("_test-remove-body-tag.md").unwrap();
    assert!(before.tags.contains(&"legacytag".to_string()));

    vault.bulk_tag("remove-body-tag-content", &[], &["legacytag".to_string()]).unwrap();

    let after = vault.read_note("_test-remove-body-tag.md").unwrap();
    assert!(!after.tags.contains(&"legacytag".to_string()), "bulk_tag remove did not strip inline body tag: {:?}", after.tags);
    assert!(!after.body.contains("#legacytag"), "inline tag text should be removed from body: {:?}", after.body);

    let _ = std::fs::remove_file(test_vault_path().join("_test-remove-body-tag.md"));
}

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

#[test]
fn test_vault_find_broken_links() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let broken = vault.find_broken_links().unwrap();

    assert!(broken.iter().any(|b| b.source.contains("note3") && b.target == "nonexistent"));
    assert!(!broken.iter().any(|b| b.target == "note1"));
}

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

    // Verify byte ordering: content added before the next section
    let pos_one = updated.body.find("- one").expect("- one not found");
    let pos_two = updated.body.find("- two").expect("- two not found");
    let pos_notes = updated.body.find("## Notes").expect("## Notes not found");
    assert!(pos_one < pos_two, "- one should come before - two");
    assert!(pos_two < pos_notes, "- two should come before ## Notes section");

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

    // Verify byte ordering: appended content is inside Tasks section before next heading
    let pos_subtask = updated.body.find("### Subtask").expect("### Subtask not found");
    let pos_detail = updated.body.find("Detail.").expect("Detail. not found");
    let pos_appended = updated.body.find("- appended").expect("- appended not found");
    let pos_notes = updated.body.find("## Notes").expect("## Notes not found");
    assert!(pos_subtask < pos_detail, "### Subtask should come before Detail.");
    assert!(pos_detail < pos_appended, "Detail. should come before - appended");
    assert!(pos_appended < pos_notes, "- appended should come before ## Notes section");

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

#[test]
fn test_vault_update_section_replace_no_trailing_newline() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-replace-eof.md"));

    vault.create_note("_test-update-section-replace-eof.md", "## Tasks", None).unwrap();

    let updated = vault.update_section("_test-update-section-replace-eof.md", "## Tasks", "- new", "replace").unwrap();
    assert!(updated.body.contains("## Tasks"));
    assert!(updated.body.contains("- new"));
    // Verify they are on separate lines (not corrupted concatenation)
    assert!(!updated.body.contains("## Tasksnew"));
    assert!(!updated.body.contains("## Tasks- new"));
    // Check byte ordering
    let pos_heading = updated.body.find("## Tasks").expect("## Tasks not found");
    let pos_content = updated.body.find("- new").expect("- new not found");
    assert!(pos_heading < pos_content, "heading should come before content");

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-replace-eof.md"));
}

#[test]
fn test_vault_update_section_invalid_heading_format() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-no-hash.md"));

    vault.create_note("_test-update-section-no-hash.md", "# Title\n\nBody.\n", None).unwrap();

    let err = vault.update_section("_test-update-section-no-hash.md", "Tasks", "content", "append").unwrap_err();
    assert!(err.to_string().contains("#"));

    let _ = std::fs::remove_file(test_vault_path().join("_test-update-section-no-hash.md"));
}

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

// ===== Plugin formats =====

use obsidian_mcp::parse::{canvas, codeblocks, excalidraw, kanban, tables};

fn write_fixture(name: &str, content: &str) {
    std::fs::write(test_vault_path().join(name), content).unwrap();
}

fn remove_fixture(name: &str) {
    let _ = std::fs::remove_file(test_vault_path().join(name));
}

#[test]
fn test_code_blocks_extract_and_nested_fence() {
    let body = "intro\n```mermaid\ngraph TD\n  A-->B\n```\n\n````md\n```js\nx\n```\n````\n~~~chart\ntype: bar\n~~~\n";
    let blocks = codeblocks::extract_code_blocks(body);
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].language, "mermaid");
    assert_eq!(blocks[0].content, "graph TD\n  A-->B\n");
    assert_eq!(blocks[1].language, "md");
    assert!(blocks[1].content.contains("```js"));
    assert_eq!(blocks[2].language, "chart");
    assert_eq!(&body[blocks[0].start..blocks[0].end], "```mermaid\ngraph TD\n  A-->B\n```\n");

    // Content containing ``` gets a longer fence so it can't close early.
    let rendered = codeblocks::render_code_block("md", "```js\nx\n```");
    assert!(rendered.starts_with("````md\n"));
    assert_eq!(codeblocks::extract_code_blocks(&rendered)[0].content, "```js\nx\n```\n");
}

#[test]
fn test_mermaid_diagram_type_detection() {
    assert_eq!(codeblocks::mermaid_diagram_type("flowchart LR\n A-->B"), Some("flowchart"));
    assert_eq!(codeblocks::mermaid_diagram_type("%%{init: {}}%%\n\nsequenceDiagram\n A->>B: hi"), Some("sequenceDiagram"));
    assert_eq!(codeblocks::mermaid_diagram_type("---\ntitle: T\n---\ngraph TD"), Some("graph"));
    assert_eq!(codeblocks::mermaid_diagram_type("pie title Pets\n \"Dogs\": 3"), Some("pie"));
    assert_eq!(codeblocks::mermaid_diagram_type("A --> B"), None);
}

#[test]
fn test_chart_yaml_builder() {
    let labels = vec!["Mon".to_string(), "Tue: late".to_string()];
    let data = [1.0, 2.5];
    let series = [codeblocks::ChartSeries { title: Some("Hours"), data: &data }];
    let mut opts = serde_json::Map::new();
    opts.insert("width".into(), serde_json::json!("80%"));
    let yaml = codeblocks::build_chart_yaml("bar", &labels, &series, &opts).unwrap();
    assert_eq!(yaml, "type: bar\nlabels: [\"Mon\",\"Tue: late\"]\nseries:\n  - title: \"Hours\"\n    data: [1.0,2.5]\nwidth: \"80%\"\n");

    let bad = [codeblocks::ChartSeries { title: None, data: &[1.0] }];
    assert!(codeblocks::build_chart_yaml("bar", &labels, &bad, &opts).is_err());
    assert!(codeblocks::build_chart_yaml("scatter", &labels, &series, &opts).is_err());
}

#[test]
fn test_tables_parse_and_format() {
    let body = "## Budget\n\n| Item | Cost |\n|:-----|-----:|\n| [[Food\\|Groceries]] | 10 |\n| Rent |\n\n```\n| not | a |\n|---|---|\n```\n";
    let found = tables::extract_tables(body);
    assert_eq!(found.len(), 1, "tables inside code blocks are ignored");
    let t = &found[0];
    assert_eq!(t.headers, vec!["Item", "Cost"]);
    assert_eq!(t.alignments, vec![tables::Alignment::Left, tables::Alignment::Right]);
    assert_eq!(t.rows, vec![vec!["[[Food\\|Groceries]]".to_string(), "10".to_string()], vec!["Rent".to_string(), "".to_string()]]);
    assert_eq!(t.heading.as_deref(), Some("## Budget"));

    let formatted = tables::format_table(&t.headers, &t.alignments, &t.rows);
    assert_eq!(
        formatted,
        "| Item                | Cost |\n| :------------------ | ---: |\n| [[Food\\|Groceries]] |   10 |\n| Rent                |      |\n"
    );
    // A formatted table parses back to the same data.
    assert_eq!(tables::extract_tables(&formatted)[0].rows, t.rows);
    // Raw pipes and newlines in new cells are escaped.
    assert_eq!(tables::escape_cell("a|b\nc"), "a\\|b<br>c");
}

#[test]
fn test_vault_tables_write_and_add_rows() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-tables.md";
    write_fixture(name, "---\nstatus: draft\n---\n# Doc\n\nIntro.\n\n## Other\n\nText.\n");

    let (_, idx) = vault.write_table(name, &["A".into(), "B".into()], &[vec!["1".into(), "2".into()]], None, None, Some("## Data")).unwrap();
    assert_eq!(idx, 0);
    vault.add_table_rows(name, 0, &[vec!["3".into(), "4".into()]]).unwrap();

    let t = &vault.read_tables(name).unwrap()[0];
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.heading.as_deref(), Some("## Data"));

    // Replace keeps the section structure and frontmatter intact.
    vault.write_table(name, &["X".into()], &[vec!["y".into()]], None, Some(0), None).unwrap();
    let content = std::fs::read_to_string(test_vault_path().join(name)).unwrap();
    assert!(content.starts_with("---\nstatus: draft\n---\n"));
    assert!(content.contains("## Other\n\nText."));
    assert!(content.contains("| X   |\n| --- |\n| y   |\n"));
    assert!(vault.write_table(name, &["X".into()], &[], None, Some(5), None).is_err());

    remove_fixture(name);
}

#[test]
fn test_vault_write_mermaid_and_chart_blocks() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-diagrams.md";
    write_fixture(name, "# Doc\n\n## Diagrams\n\nSee below.\n\n## Notes\n\nEnd.\n");

    let (_, i) = vault.write_code_block(name, "mermaid", "graph TD\n A-->B", None, Some("## Diagrams")).unwrap();
    assert_eq!(i, 0);
    let (_, i) = vault.write_code_block(name, "mermaid", "graph TD\n C-->D", None, None).unwrap();
    assert_eq!(i, 1);
    vault.write_code_block(name, "mermaid", "graph LR\n A-->Z", Some(0), None).unwrap();

    let blocks = vault.list_code_blocks(name, Some("mermaid")).unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].content, "graph LR\n A-->Z\n");
    let content = std::fs::read_to_string(test_vault_path().join(name)).unwrap();
    let diagrams_at = content.find("## Diagrams").unwrap();
    let block_at = content.find("graph LR").unwrap();
    let notes_at = content.find("## Notes").unwrap();
    assert!(diagrams_at < block_at && block_at < notes_at, "block inserted inside its section");

    assert!(vault.write_code_block(name, "chart", "type: bar", Some(0), None).is_err());
    remove_fixture(name);
}

const SAMPLE_BOARD: &str = "---\n\nkanban-plugin: board\n\n---\n\n## To Do\n\n- [ ] Write spec\n- [ ] Review [[Plan]]\n\tsecond line\n\n\n## Done\n\n**Complete**\n- [x] Setup\n\n\n***\n\n## Archive\n\n- [x] Old thing\n\n%% kanban:settings\n```\n{\"kanban-plugin\":\"board\",\"list-collapse\":[false,false]}\n```\n%%";

#[test]
fn test_kanban_parse_and_render_round_trip() {
    let body = obsidian_mcp::parse::frontmatter::split_raw(SAMPLE_BOARD).1;
    let board = kanban::parse(body);
    assert_eq!(board.lanes.len(), 2);
    assert_eq!(board.lanes[0].title, "To Do");
    assert_eq!(board.lanes[0].cards[1].text, "Review [[Plan]]\nsecond line");
    assert!(board.lanes[1].complete);
    assert!(board.lanes[1].cards[0].checked);
    assert_eq!(board.archive.as_ref().unwrap().cards[0].text, "Old thing");
    assert!(board.settings.as_ref().unwrap().contains("list-collapse"));

    let rendered = kanban::render(&board);
    assert_eq!(kanban::parse(&rendered), board);
    assert!(rendered.contains("- [ ] Review [[Plan]]\n\tsecond line\n"));
    assert!(rendered.contains("***\n\n## Archive"));
}

#[test]
fn test_vault_kanban_create_add_move_archive() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-board.md";
    remove_fixture(name);

    let lanes = vec![
        obsidian_mcp::vault::NewLane { title: "Backlog".into(), complete: false, cards: vec!["Plan trip".into()] },
        obsidian_mcp::vault::NewLane { title: "Done".into(), complete: true, cards: vec![] },
    ];
    vault.create_kanban("_test-board", &lanes).unwrap();
    assert!(vault.create_kanban("_test-board", &lanes).is_err(), "won't overwrite");

    vault.add_kanban_card(name, "backlog", "Book hotel", None, true).unwrap();
    let (_, board) = vault.read_kanban(name).unwrap();
    assert_eq!(board.lanes[0].cards[0].text, "Book hotel");

    // Moving into a complete lane checks the card.
    let update = obsidian_mcp::vault::CardUpdate { to_lane: Some("Done"), ..Default::default() };
    let (_, board) = vault.update_kanban_card(name, "hotel", None, update).unwrap();
    assert_eq!(board.lanes[1].cards[0], kanban::Card { text: "Book hotel".into(), checked: true });

    let update = obsidian_mcp::vault::CardUpdate { archive: true, ..Default::default() };
    let (_, board) = vault.update_kanban_card(name, "Plan trip", None, update).unwrap();
    assert!(board.lanes[0].cards.is_empty());
    assert_eq!(board.archive.unwrap().cards[0].text, "Plan trip");

    let missing = obsidian_mcp::vault::CardUpdate { checked: Some(true), ..Default::default() };
    assert!(vault.update_kanban_card(name, "nope", None, missing).is_err());
    assert!(vault.read_kanban("note1").is_err(), "non-board notes are rejected");

    let content = std::fs::read_to_string(test_vault_path().join(name)).unwrap();
    assert!(content.starts_with("---\n\nkanban-plugin: board\n\n---\n\n## Backlog"));
    assert!(content.contains("%% kanban:settings"));
    remove_fixture(name);
}

#[test]
fn test_excalidraw_build_elements_with_labels_and_bindings() {
    let specs: Vec<excalidraw::ElementSpec> = serde_json::from_value(serde_json::json!([
        { "id": "a", "type": "rectangle", "x": 0, "y": 0, "width": 100, "height": 50, "text": "Start" },
        { "id": "b", "type": "ellipse", "x": 300, "y": 0, "width": 100, "height": 50 },
        { "type": "arrow", "from": "a", "to": "b", "text": "next" },
    ])).unwrap();
    let mut elements = Vec::new();
    let texts = excalidraw::add_elements(&mut elements, &specs).unwrap();

    assert_eq!(elements.len(), 5, "2 shapes + 1 arrow + 2 bound labels");
    assert_eq!(texts.len(), 2);
    let arrow = elements.iter().find(|e| e["type"] == "arrow").unwrap();
    assert_eq!(arrow["startBinding"]["elementId"], "a");
    assert_eq!(arrow["endBinding"]["elementId"], "b");
    // Arrow runs from a's right edge to b's left edge (plus the binding gap).
    assert_eq!(arrow["x"], 108.0);
    assert_eq!(arrow["points"][1][0], 184.0);
    let a = &elements[0];
    let bound: Vec<&str> = a["boundElements"].as_array().unwrap().iter().map(|b| b["type"].as_str().unwrap()).collect();
    assert_eq!(bound, vec!["text", "arrow"]);

    let summary = excalidraw::summarize(&excalidraw::new_scene(elements.clone()));
    assert_eq!(summary.len(), 3, "bound labels fold into their containers");
    assert_eq!(summary[0]["label"], "Start");

    let dup: Vec<excalidraw::ElementSpec> = serde_json::from_value(serde_json::json!([{ "id": "a", "type": "text", "x": 0, "y": 0, "text": "x" }])).unwrap();
    assert!(excalidraw::add_elements(&mut elements, &dup).is_err());
}

#[test]
fn test_vault_excalidraw_create_read_add() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-drawing.excalidraw.md";
    remove_fixture(name);

    let specs: Vec<excalidraw::ElementSpec> = serde_json::from_value(serde_json::json!([
        { "id": "box1", "type": "rectangle", "x": 0, "y": 0, "text": "Hello" },
    ])).unwrap();
    let (path, _) = vault.create_drawing("_test-drawing", &specs).unwrap();
    assert_eq!(path, name);

    let more: Vec<excalidraw::ElementSpec> = serde_json::from_value(serde_json::json!([
        { "id": "box2", "type": "diamond", "x": 300, "y": 0, "text": "World" },
        { "type": "arrow", "from": "box1", "to": "box2" },
    ])).unwrap();
    vault.add_drawing_elements("_test-drawing", &more).unwrap();

    let d = vault.read_drawing(name).unwrap();
    let texts: Vec<&str> = d.text_elements.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(texts, vec!["Hello", "World"]);
    let summary = excalidraw::summarize(&d.scene);
    assert_eq!(summary.len(), 3);
    assert!(summary.iter().any(|e| e["from"] == "box1" && e["to"] == "box2"));

    let content = std::fs::read_to_string(test_vault_path().join(name)).unwrap();
    assert!(content.starts_with("---\n\nexcalidraw-plugin: parsed\n"));
    assert!(content.contains("## Text Elements\nHello ^"));
    assert!(content.contains("%%\n## Drawing\n```json\n"));
    remove_fixture(name);
}

#[test]
fn test_vault_excalidraw_reads_compressed_drawing() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-compressed.excalidraw.md";
    let scene = serde_json::json!({
        "type": "excalidraw", "version": 2,
        "elements": [{ "id": "t1", "type": "text", "x": 1, "y": 2, "width": 10, "height": 5, "text": "Hi ✓" }],
        "files": {},
    });
    let compressed = lz_str::compress_to_base64(scene.to_string().as_str());
    // The plugin wraps compressed data across lines; decompression must ignore that.
    let wrapped: Vec<String> = compressed.as_bytes().chunks(64).map(|c| String::from_utf8(c.to_vec()).unwrap()).collect();
    write_fixture(name, &format!(
        "---\nexcalidraw-plugin: parsed\n---\n# Excalidraw Data\n\n## Text Elements\nHi ✓ ^t1\n\n## Embedded Files\nabc123: [[photo.png]]\n\n%%\n## Drawing\n```compressed-json\n{}\n```\n%%",
        wrapped.join("\n\n"),
    ));

    let d = vault.read_drawing(name).unwrap();
    assert!(d.compressed);
    assert_eq!(d.text_elements, vec![("t1".to_string(), "Hi ✓".to_string())]);
    assert_eq!(d.embedded_files, vec![("abc123".to_string(), "[[photo.png]]".to_string())]);
    assert_eq!(d.scene["elements"][0]["text"], "Hi ✓");
    remove_fixture(name);
}

#[test]
fn test_canvas_apply_edits() {
    let mut doc = canvas::empty_canvas();
    let nodes: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([
        { "id": "n1", "type": "text", "text": "Idea", "x": 0, "y": 0, "style_attributes": { "shape": "pill" } },
        { "id": "n2", "type": "file", "file": "note1.md" },
    ])).unwrap();
    let edges: Vec<canvas::EdgeSpec> = serde_json::from_value(serde_json::json!([
        { "id": "e1", "from_node": "n1", "to_node": "n2", "to_side": "top", "style_attributes": { "path": "dotted" } },
    ])).unwrap();
    let s = canvas::apply_edits(&mut doc, &nodes, &edges, &[]).unwrap();
    assert_eq!(s.created, vec!["n1", "n2", "e1"]);
    assert_eq!(doc["nodes"][0]["styleAttributes"]["shape"], "pill");
    assert_eq!(doc["nodes"][1]["y"], 140, "auto-placed below existing content");
    assert_eq!(doc["edges"][0]["styleAttributes"]["path"], "dotted");

    // Update merges fields; null removes a style attribute.
    let update: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([
        { "id": "n1", "color": "4", "style_attributes": { "shape": null, "border": "dashed" } },
    ])).unwrap();
    let s = canvas::apply_edits(&mut doc, &update, &[], &[]).unwrap();
    assert_eq!(s.updated, vec!["n1"]);
    assert_eq!(doc["nodes"][0]["text"], "Idea");
    assert_eq!(doc["nodes"][0]["styleAttributes"], serde_json::json!({ "border": "dashed" }));

    // Removing a node removes its edges.
    let s = canvas::apply_edits(&mut doc, &[], &[], &["n2".to_string()]).unwrap();
    assert_eq!(s.removed, vec!["n2", "e1"]);
    assert_eq!(doc["edges"].as_array().unwrap().len(), 0);

    let bad_edge: Vec<canvas::EdgeSpec> = serde_json::from_value(serde_json::json!([{ "from_node": "n1", "to_node": "ghost" }])).unwrap();
    assert!(canvas::apply_edits(&mut doc, &[], &bad_edge, &[]).is_err());
    let bad_color: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([{ "id": "n1", "color": "9" }])).unwrap();
    assert!(canvas::apply_edits(&mut doc, &bad_color, &[], &[]).is_err());
    let missing_text: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([{ "type": "text" }])).unwrap();
    assert!(canvas::apply_edits(&mut doc, &missing_text, &[], &[]).is_err());
}

#[test]
fn test_vault_canvas_create_edit_read() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());
    let name = "_test-board.canvas";
    remove_fixture(name);

    let nodes: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([{ "id": "a", "type": "text", "text": "A" }])).unwrap();
    let (path, _) = vault.create_canvas("_test-board", &nodes, &[]).unwrap();
    assert_eq!(path, name);

    let more: Vec<canvas::NodeSpec> = serde_json::from_value(serde_json::json!([{ "id": "b", "type": "link", "url": "https://obsidian.md" }])).unwrap();
    let edges: Vec<canvas::EdgeSpec> = serde_json::from_value(serde_json::json!([{ "from_node": "a", "to_node": "b" }])).unwrap();
    vault.edit_canvas(name, &more, &edges, &[]).unwrap();

    let (_, doc) = vault.read_canvas("_test-board").unwrap();
    assert_eq!(doc["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(doc["edges"][0]["toNode"], "b");
    let raw = std::fs::read_to_string(test_vault_path().join(name)).unwrap();
    assert!(raw.starts_with("{\n\t\"nodes\""), "tab-indented like Obsidian writes it");
    assert!(vault.list_vault(None, None).unwrap().iter().any(|e| e == name));
    assert!(vault.create_canvas("../escape", &[], &[]).is_err());

    remove_fixture(name);
}
