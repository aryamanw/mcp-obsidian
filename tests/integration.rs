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
    fm.insert("tags".to_string(), "alpha, beta".to_string());
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
    fm.insert("status".to_string(), "new".to_string());

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
    fm.insert("priority".to_string(), "high".to_string());
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
fn test_vault_bulk_tag() {
    let vault = obsidian_mcp::vault::Vault::new(test_config());

    let _ = std::fs::remove_file(test_vault_path().join("_test-bt-note.md"));

    vault.create_note("_test-bt-note.md", "bulk-taggable content", None).unwrap();

    let count = vault.bulk_tag("bulk-taggable", &["new-tag".to_string()], &[]).unwrap();
    assert_eq!(count, 1);

    let note = vault.read_note("_test-bt-note.md").unwrap();
    assert_eq!(note.frontmatter.get("tags").unwrap(), "new-tag");

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
    assert!(note.frontmatter.get("tags").map_or(true, |t| t.is_empty()));

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
