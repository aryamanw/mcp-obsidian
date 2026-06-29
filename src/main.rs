mod config;
mod graph;
mod parse;
mod tools;
mod vault;

use config::Config;
use graph::GraphAnalyzer;
use parse::wikilink;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use std::sync::Arc;
use vault::Vault;

#[derive(Clone)]
struct ObsidianMcp {
    vault: Arc<Vault>,
    analyzer: Arc<GraphAnalyzer>,
    #[allow(dead_code)]
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

    // ===== Read Tools =====

    #[tool(description = "Read a note by path. Returns markdown content, parsed frontmatter, and resolved links.")]
    fn read_note(
        &self,
        Parameters(req): Parameters<tools::read::ReadNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&req.path) {
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

    #[tool(description = "List vault structure \u{2014} folders and notes.")]
    fn list_vault(
        &self,
        Parameters(req): Parameters<tools::read::ListVaultRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.list_vault(req.path.as_deref(), req.depth) {
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
        Parameters(req): Parameters<tools::read::GetMetadataRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&req.path) {
            Ok(note) => {
                let backlinks = self.vault.backlinks(&req.path).unwrap_or_default();
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

    // ===== Search Tools =====

    #[tool(description = "Full-text search across the vault. Returns matching notes with snippets.")]
    fn search_notes(
        &self,
        Parameters(req): Parameters<tools::search::SearchNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max = req.limit.unwrap_or(20);
        match self.vault.search_notes(&req.query, max) {
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
        Parameters(req): Parameters<tools::search::SearchByTagRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mode = req.match_mode.unwrap_or_else(|| "any".to_string());
        match self.vault.search_by_tag(&req.tags, &mode) {
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
        Parameters(req): Parameters<tools::search::SearchByFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.search_by_frontmatter(&req.filters) {
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

    // ===== Write Tools =====

    #[tool(description = "Create a new note with optional content and frontmatter.")]
    fn create_note(
        &self,
        Parameters(req): Parameters<tools::write::CreateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = req.content.unwrap_or_default();
        match self.vault.create_note(&req.path, &body, req.frontmatter.as_ref()) {
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
        Parameters(req): Parameters<tools::write::UpdateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.update_note(&req.path, &req.content, &req.mode) {
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
        Parameters(req): Parameters<tools::write::SetFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.set_frontmatter(&req.path, &req.fields) {
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

    // ===== Link Tools =====

    #[tool(description = "Resolve all [[wikilinks]] in a note to their actual file paths.")]
    fn resolve_links(
        &self,
        Parameters(req): Parameters<tools::links::ResolveLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.read_note(&req.path) {
            Ok(note) => {
                let resolved: Vec<serde_json::Value> = note.links.iter().map(|link| {
                    let resolved_path = wikilink::resolve_wikilink(
                        link,
                        &self.vault.config.vault_path,
                    );
                    serde_json::json!({
                        "target": link,
                        "resolved": resolved_path.as_ref().map(|p| {
                            wikilink::relative_path(p, &self.vault.config.vault_path)
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
        Parameters(req): Parameters<tools::links::BacklinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.backlinks(&req.path) {
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

    #[tool(description = "Get the link graph for a note \u{2014} its outgoing links and their links, up to N hops.")]
    fn link_graph(
        &self,
        Parameters(req): Parameters<tools::links::LinkGraphRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max_depth = req.depth.unwrap_or(1);
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
            if depth > max_depth || visited.contains(note_path) {
                return;
            }
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

        explore(
            &req.path,
            0,
            max_depth,
            &self.vault,
            &mut visited,
            &mut nodes,
            &mut edges,
        );

        let result = serde_json::json!({
            "nodes": nodes,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    // ===== Template Tools =====

    #[tool(description = "List available templates in the vault's templates folder.")]
    fn list_templates(
        &self,
        Parameters(()): Parameters<()>,
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
        Parameters(req): Parameters<tools::templates::ApplyTemplateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.apply_template(&req.template, &req.path) {
            Ok(()) => {
                let result = serde_json::json!({
                    "message": format!("Template '{}' applied to '{}'", req.template, req.path),
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Get or create today's daily note. Returns existing note or creates a new one.")]
    fn get_daily_note(
        &self,
        Parameters(req): Parameters<tools::templates::GetDailyNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.vault.get_daily_note(req.date.as_deref()) {
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

    // ===== Graph Tools =====

    #[tool(description = "Get vault graph statistics: node count, edge count, density.")]
    fn graph_stats(
        &self,
        Parameters(()): Parameters<()>,
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
        Parameters(()): Parameters<()>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.communities() {
            Ok(communities) => {
                let result: Vec<serde_json::Value> = communities
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "node_count": c.nodes.len(),
                            "nodes": c.nodes,
                        })
                    })
                    .collect();
                let output = serde_json::json!({
                    "communities": result,
                    "count": result.len(),
                });
                Ok(CallToolResult::success(vec![Content::text(
                    output.to_string(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(description = "Find the shortest path between two notes in the vault graph.")]
    fn graph_path(
        &self,
        Parameters(req): Parameters<tools::graph::GraphPathRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match self.analyzer.shortest_path(&req.from, &req.to) {
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

#[tool_handler]
impl ServerHandler for ObsidianMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Obsidian vault management MCP server. Read, write, search notes, manage links, templates, and graph analysis.",
            )
    }
}

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

    let service = ObsidianMcp::new(vault, analyzer)
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
