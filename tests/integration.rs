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
