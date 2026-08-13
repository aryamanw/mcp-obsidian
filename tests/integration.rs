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
