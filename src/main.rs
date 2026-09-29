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

/// Adapts a `Vault`/`GraphAnalyzer` call's `anyhow::Result<T>` into the
/// `CallToolResult` shape every `#[tool]` method below needs: success
/// becomes a JSON text blob built from `f`, failure becomes an error blob
/// carrying the error's message. Centralizing this means individual tool
/// methods only need to say *what* JSON a success produces.
fn respond<T>(
    result: anyhow::Result<T>,
    f: impl FnOnce(T) -> serde_json::Value,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match result {
        Ok(v) => Ok(CallToolResult::success(vec![Content::text(f(v).to_string())])),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
    }
}

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
        respond(self.vault.read_note(&req.path), |note| {
            serde_json::json!({
                "path": note.path,
                "frontmatter": note.frontmatter,
                "body": note.body,
                "tags": note.tags,
                "links": note.links,
            })
        })
    }

    #[tool(description = "List vault structure \u{2014} folders and notes.")]
    fn list_vault(
        &self,
        Parameters(req): Parameters<tools::read::ListVaultRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.list_vault(req.path.as_deref(), req.depth), |entries| {
            serde_json::json!({
                "entries": entries,
                "count": entries.len(),
            })
        })
    }

    #[tool(description = "Get metadata for a note: frontmatter fields, tags, outgoing links, and backlink count.")]
    fn get_metadata(
        &self,
        Parameters(req): Parameters<tools::read::GetMetadataRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_note(&req.path), |note| {
            let backlinks = self.vault.backlinks(&req.path).unwrap_or_default();
            serde_json::json!({
                "path": note.path,
                "frontmatter": note.frontmatter,
                "tags": note.tags,
                "outgoing_links": note.links,
                "backlink_count": backlinks.len(),
                "backlinks": backlinks,
            })
        })
    }

    #[tool(description = "List notes sorted by last-modified time, newest first.")]
    fn list_recent_notes(
        &self,
        Parameters(req): Parameters<tools::read::ListRecentNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let limit = req.limit.unwrap_or(20);
        respond(self.vault.list_recent_notes(limit), |notes| {
            let results: Vec<serde_json::Value> = notes.iter().map(|(path, modified)| {
                let modified: chrono::DateTime<chrono::Utc> = (*modified).into();
                serde_json::json!({
                    "path": path,
                    "modified": modified.to_rfc3339(),
                })
            }).collect();
            serde_json::json!({
                "notes": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Get the text under a heading in a note (e.g. '## Tasks'), up to the next heading of equal or higher level.")]
    fn get_section(
        &self,
        Parameters(req): Parameters<tools::read::GetSectionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.get_section(&req.path, &req.heading), |section| {
            serde_json::json!({
                "path": req.path,
                "heading": req.heading,
                "content": section,
            })
        })
    }

    // ===== Search Tools =====

    #[tool(description = "Full-text search across the vault. Returns matching notes with snippets.")]
    fn search_notes(
        &self,
        Parameters(req): Parameters<tools::search::SearchNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max = req.limit.unwrap_or(20);
        respond(self.vault.search_notes(&req.query, max), |notes| {
            let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                serde_json::json!({
                    "path": n.path,
                    "tags": n.tags,
                    "body_preview": n.body.chars().take(200).collect::<String>(),
                })
            }).collect();
            serde_json::json!({
                "results": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Find notes with specific tags.")]
    fn search_by_tag(
        &self,
        Parameters(req): Parameters<tools::search::SearchByTagRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mode = req.match_mode.unwrap_or_else(|| "any".to_string());
        respond(self.vault.search_by_tag(&req.tags, &mode), |notes| {
            let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                serde_json::json!({
                    "path": n.path,
                    "tags": n.tags,
                })
            }).collect();
            serde_json::json!({
                "results": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Filter notes by frontmatter fields.")]
    fn search_by_frontmatter(
        &self,
        Parameters(req): Parameters<tools::search::SearchByFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.search_by_frontmatter(&req.filters), |notes| {
            let results: Vec<serde_json::Value> = notes.iter().map(|n| {
                serde_json::json!({
                    "path": n.path,
                    "frontmatter": n.frontmatter,
                })
            }).collect();
            serde_json::json!({
                "results": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "List all tags used across the vault, each with a usage count (number of notes containing it).")]
    fn list_tags(
        &self,
        Parameters(_req): Parameters<tools::search::ListTagsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.list_tags(), |tags| {
            let results: Vec<serde_json::Value> = tags.iter().map(|(tag, count)| {
                serde_json::json!({ "tag": tag, "count": count })
            }).collect();
            serde_json::json!({
                "tags": results,
                "count": results.len(),
            })
        })
    }

    // ===== Write Tools =====

    #[tool(description = "Create a new folder in the vault.")]
    fn create_folder(
        &self,
        Parameters(req): Parameters<tools::write::CreateFolderRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.create_folder(&req.path), |_| {
            serde_json::json!({
                "path": req.path,
                "message": "Folder created successfully",
            })
        })
    }

    #[tool(description = "Rename or move a note. Updates all [[wikilinks]] that point to the old path.")]
    fn rename_note(
        &self,
        Parameters(req): Parameters<tools::write::RenameNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.rename_note(&req.source, &req.dest), |note| {
            serde_json::json!({
                "path": note.path,
                "message": format!("Note renamed from '{}' to '{}'", req.source, req.dest),
            })
        })
    }

    #[tool(description = "Merge source note into destination note. Appends source body with a heading separator, then deletes source.")]
    fn merge_notes(
        &self,
        Parameters(req): Parameters<tools::write::MergeNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.merge_notes(&req.source, &req.dest), |note| {
            serde_json::json!({
                "path": note.path,
                "message": format!("Merged '{}' into '{}'", req.source, req.dest),
            })
        })
    }

    #[tool(description = "Add or remove tags across all notes matching a full-text search query.")]
    fn bulk_tag(
        &self,
        Parameters(req): Parameters<tools::write::BulkTagRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let add = req.add_tags.unwrap_or_default();
        let remove = req.remove_tags.unwrap_or_default();
        respond(self.vault.bulk_tag(&req.query, &add, &remove), |count| {
            serde_json::json!({
                "notes_updated": count,
                "message": format!("Updated tags on {} note(s)", count),
            })
        })
    }

    #[tool(description = "Find notes related by content similarity and add a '## Related' section with [[wikilinks]].")]
    fn link_related_notes(
        &self,
        Parameters(req): Parameters<tools::write::LinkRelatedNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.link_related_notes(&req.path), |note| {
            let related = note.links;
            serde_json::json!({
                "path": note.path,
                "related_count": related.len(),
                "links": related,
                "message": format!("Linked to {} related note(s)", related.len()),
            })
        })
    }

    #[tool(description = "Create a new note with optional content and frontmatter.")]
    fn create_note(
        &self,
        Parameters(req): Parameters<tools::write::CreateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = req.content.unwrap_or_default();
        respond(self.vault.create_note(&req.path, &body, req.frontmatter.as_ref()), |note| {
            serde_json::json!({
                "path": note.path,
                "frontmatter": note.frontmatter,
                "message": "Note created successfully",
            })
        })
    }

    #[tool(description = "Update an existing note. Use 'append' to add content or 'replace' to overwrite (preserves frontmatter).")]
    fn update_note(
        &self,
        Parameters(req): Parameters<tools::write::UpdateNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.update_note(&req.path, &req.content, &req.mode), |note| {
            serde_json::json!({
                "path": note.path,
                "message": "Note updated successfully",
            })
        })
    }

    #[tool(description = "Set frontmatter fields on a note. Merges with existing frontmatter.")]
    fn set_frontmatter(
        &self,
        Parameters(req): Parameters<tools::write::SetFrontmatterRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.set_frontmatter(&req.path, &req.fields), |note| {
            serde_json::json!({
                "path": note.path,
                "frontmatter": note.frontmatter,
                "message": "Frontmatter updated successfully",
            })
        })
    }

    #[tool(description = "Update just one section of a note, addressed by heading (e.g. '## Tasks'). Creates the section at the end of the note if the heading doesn't exist yet. Use 'append' to add to the section or 'replace' to overwrite it.")]
    fn update_section(
        &self,
        Parameters(req): Parameters<tools::write::UpdateSectionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.update_section(&req.path, &req.heading, &req.content, &req.mode), |note| {
            serde_json::json!({
                "path": note.path,
                "heading": req.heading,
                "message": "Section updated successfully",
            })
        })
    }

    #[tool(description = "Move a note to the vault's .trash folder instead of deleting it. Collisions in .trash get a numeric suffix.")]
    fn trash_note(
        &self,
        Parameters(req): Parameters<tools::write::TrashNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.trash_note(&req.path), |_| {
            serde_json::json!({
                "path": req.path,
                "message": "Note moved to .trash",
            })
        })
    }

    // ===== Link Tools =====

    #[tool(description = "Resolve all [[wikilinks]] in a note to their actual file paths.")]
    fn resolve_links(
        &self,
        Parameters(req): Parameters<tools::links::ResolveLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_note(&req.path), |note| {
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
            serde_json::json!({
                "links": resolved,
                "count": resolved.len(),
            })
        })
    }

    #[tool(description = "Find all notes that link to a given note (backlinks).")]
    fn backlinks(
        &self,
        Parameters(req): Parameters<tools::links::BacklinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.backlinks(&req.path), |links| {
            serde_json::json!({
                "backlinks": links,
                "count": links.len(),
            })
        })
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

    #[tool(description = "Find all [[wikilinks]] across the vault that don't resolve to an existing note.")]
    fn find_broken_links(
        &self,
        Parameters(_req): Parameters<tools::links::FindBrokenLinksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.find_broken_links(), |broken| {
            let results: Vec<serde_json::Value> = broken.iter().map(|b| {
                serde_json::json!({ "source": b.source, "target": b.target })
            }).collect();
            serde_json::json!({
                "broken_links": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Find notes with no backlinks — nothing else in the vault links to them.")]
    fn find_orphan_notes(
        &self,
        Parameters(_req): Parameters<tools::links::FindOrphanNotesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.find_orphan_notes(), |orphans| {
            serde_json::json!({
                "orphans": orphans,
                "count": orphans.len(),
            })
        })
    }

    // ===== Template Tools =====

    #[tool(description = "List available templates in the vault's templates folder.")]
    fn list_templates(
        &self,
        Parameters(_req): Parameters<tools::templates::ListTemplatesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.list_templates(), |templates| {
            serde_json::json!({
                "templates": templates,
                "count": templates.len(),
            })
        })
    }

    #[tool(description = "Apply a template to a note. Merges template frontmatter with existing.")]
    fn apply_template(
        &self,
        Parameters(req): Parameters<tools::templates::ApplyTemplateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.apply_template(&req.template, &req.path), |_| {
            serde_json::json!({
                "message": format!("Template '{}' applied to '{}'", req.template, req.path),
            })
        })
    }

    #[tool(description = "Get or create today's daily note. Returns existing note or creates a new one.")]
    fn get_daily_note(
        &self,
        Parameters(req): Parameters<tools::templates::GetDailyNoteRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.get_daily_note(req.date.as_deref()), |note| {
            serde_json::json!({
                "path": note.path,
                "frontmatter": note.frontmatter,
                "body": note.body,
                "tags": note.tags,
            })
        })
    }

    // ===== Graph Tools =====

    #[tool(description = "Get vault graph statistics: node count, edge count, density.")]
    fn graph_stats(
        &self,
        Parameters(_req): Parameters<tools::graph::GraphStatsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.analyzer.stats(), |stats| {
            serde_json::json!({
                "node_count": stats.node_count,
                "edge_count": stats.edge_count,
                "density": stats.density,
            })
        })
    }

    #[tool(description = "Get communities (connected components) from Obsidian's note graph.")]
    fn graph_communities(
        &self,
        Parameters(_req): Parameters<tools::graph::GraphCommunitiesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.analyzer.communities(), |communities| {
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
            serde_json::json!({
                "communities": result,
                "count": result.len(),
            })
        })
    }

    #[tool(description = "Find the shortest path between two notes in the vault graph.")]
    fn graph_path(
        &self,
        Parameters(req): Parameters<tools::graph::GraphPathRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.analyzer.shortest_path(&req.from, &req.to), |path| {
            serde_json::json!({
                "path": path,
                "hops": path.len().saturating_sub(1),
            })
        })
    }

    // ===== Excalidraw Tools =====

    #[tool(description = "Read an Excalidraw drawing: its text elements, embedded files, element links, and a summary of every shape (id, type, position, size, text/label, arrow from/to). Handles compressed drawings.")]
    fn read_drawing(
        &self,
        Parameters(req): Parameters<tools::excalidraw::ReadDrawingRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_drawing(&req.path), |d| {
            let pairs = |v: &[(String, String)], k: &str, val: &str| -> Vec<serde_json::Value> {
                v.iter().map(|(a, b)| serde_json::json!({ k: a, val: b })).collect()
            };
            let elements = parse::excalidraw::summarize(&d.scene);
            let mut out = serde_json::json!({
                "path": d.path,
                "compressed": d.compressed,
                "text_elements": pairs(&d.text_elements, "id", "text"),
                "embedded_files": pairs(&d.embedded_files, "id", "file"),
                "element_links": pairs(&d.element_links, "id", "link"),
                "element_count": elements.len(),
                "elements": elements,
            });
            if req.include_raw.unwrap_or(false) {
                out["scene"] = d.scene;
            }
            out
        })
    }

    #[tool(description = "Create a new Excalidraw drawing (.excalidraw.md) from simple elements: rectangles, ellipses, diamonds, text, and arrows/lines. Shapes with 'text' get a centered label; arrows with 'from'/'to' element ids are bound between those shapes.")]
    fn create_drawing(
        &self,
        Parameters(req): Parameters<tools::excalidraw::CreateDrawingRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.create_drawing(&req.path, &req.elements), |(path, count)| {
            serde_json::json!({
                "path": path,
                "element_count": count,
                "message": "Drawing created successfully",
            })
        })
    }

    #[tool(description = "Add elements to an existing Excalidraw drawing. Arrows can connect to existing elements by id (see read_drawing).")]
    fn add_drawing_elements(
        &self,
        Parameters(req): Parameters<tools::excalidraw::AddDrawingElementsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.add_drawing_elements(&req.path, &req.elements), |(path, ids)| {
            serde_json::json!({
                "path": path,
                "added_ids": ids,
                "message": format!("Added {} element(s)", ids.len()),
            })
        })
    }

    // ===== Table Tools (Advanced Tables) =====

    #[tool(description = "Read every markdown table in a note as structured data: headers, column alignments, rows, and the heading each table sits under.")]
    fn read_tables(
        &self,
        Parameters(req): Parameters<tools::tables::ReadTablesRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_tables(&req.path), |tables| {
            let results: Vec<serde_json::Value> = tables.iter().enumerate().map(|(i, t)| {
                serde_json::json!({
                    "index": i,
                    "heading": t.heading,
                    "headers": t.headers,
                    "alignments": t.alignments,
                    "rows": t.rows,
                    "row_count": t.rows.len(),
                })
            }).collect();
            serde_json::json!({
                "tables": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Write a markdown table, formatted the way Advanced Tables formats it (aligned, padded columns). Replaces an existing table by index, or inserts a new one under a heading or at the end of the note.")]
    fn write_table(
        &self,
        Parameters(req): Parameters<tools::tables::WriteTableRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = self.vault.write_table(
            &req.path, &req.headers, &req.rows, req.alignments.as_deref(), req.table_index, req.heading.as_deref(),
        );
        respond(result, |(path, index)| {
            serde_json::json!({
                "path": path,
                "table_index": index,
                "message": "Table written successfully",
            })
        })
    }

    #[tool(description = "Append rows to an existing markdown table and reformat it (Advanced Tables style).")]
    fn add_table_rows(
        &self,
        Parameters(req): Parameters<tools::tables::AddTableRowsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.add_table_rows(&req.path, req.table_index, &req.rows), |(path, row_count)| {
            serde_json::json!({
                "path": path,
                "table_index": req.table_index,
                "row_count": row_count,
                "message": format!("Added {} row(s)", req.rows.len()),
            })
        })
    }

    // ===== Kanban Tools =====

    #[tool(description = "Read a Kanban plugin board: its lanes (in order) with their cards and checked state, plus archived cards. Find boards with search_by_frontmatter {\"kanban-plugin\": \"board\"}.")]
    fn read_kanban(
        &self,
        Parameters(req): Parameters<tools::kanban::ReadKanbanRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_kanban(&req.path), |(path, board)| board_json(&path, &board))
    }

    #[tool(description = "Create a new Kanban plugin board with the given lanes and optional initial cards.")]
    fn create_kanban(
        &self,
        Parameters(req): Parameters<tools::kanban::CreateKanbanRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let lanes: Vec<vault::NewLane> = req.lanes.into_iter().map(|l| vault::NewLane {
            title: l.title,
            complete: l.complete.unwrap_or(false),
            cards: l.cards.unwrap_or_default(),
        }).collect();
        respond(self.vault.create_kanban(&req.path, &lanes), |(path, board)| board_json(&path, &board))
    }

    #[tool(description = "Add a card to a lane of a Kanban plugin board.")]
    fn add_kanban_card(
        &self,
        Parameters(req): Parameters<tools::kanban::AddKanbanCardRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let at_top = match req.position.as_deref() {
            None | Some("bottom") => false,
            Some("top") => true,
            Some(other) => return Ok(CallToolResult::error(vec![Content::text(
                format!("Invalid position: {} (use 'top' or 'bottom')", other),
            )])),
        };
        respond(
            self.vault.add_kanban_card(&req.path, &req.lane, &req.text, req.checked, at_top),
            |(path, board)| board_json(&path, &board),
        )
    }

    #[tool(description = "Move, edit, check/uncheck, or archive a card on a Kanban plugin board. The card is found by exact text or a unique substring. Moving into or out of a 'complete' lane checks/unchecks the card unless 'checked' is given.")]
    fn update_kanban_card(
        &self,
        Parameters(req): Parameters<tools::kanban::UpdateKanbanCardRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let update = vault::CardUpdate {
            to_lane: req.to_lane.as_deref(),
            new_text: req.new_text.as_deref(),
            checked: req.checked,
            archive: req.archive.unwrap_or(false),
        };
        respond(
            self.vault.update_kanban_card(&req.path, &req.card, req.lane.as_deref(), update),
            |(path, board)| board_json(&path, &board),
        )
    }

    // ===== Diagram & Chart Tools (Mermaid, Charts) =====

    #[tool(description = "List fenced code blocks in a note, optionally filtered by language — e.g. 'mermaid' for Mermaid diagrams or 'chart' for Obsidian Charts. Each block's index is what write_mermaid_diagram/write_chart use to replace it.")]
    fn list_code_blocks(
        &self,
        Parameters(req): Parameters<tools::diagrams::ListCodeBlocksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.list_code_blocks(&req.path, req.language.as_deref()), |blocks| {
            let results: Vec<serde_json::Value> = blocks.iter().enumerate().map(|(i, b)| {
                serde_json::json!({
                    "index": i,
                    "language": b.language,
                    "content": b.content,
                })
            }).collect();
            serde_json::json!({
                "blocks": results,
                "count": results.len(),
            })
        })
    }

    #[tool(description = "Insert or replace a Mermaid diagram (```mermaid block) in a note. Validates that the diagram starts with a known Mermaid diagram type (flowchart, sequenceDiagram, classDiagram, stateDiagram-v2, erDiagram, gantt, pie, mindmap, timeline, gitGraph, ...).")]
    fn write_mermaid_diagram(
        &self,
        Parameters(req): Parameters<tools::diagrams::WriteMermaidRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let Some(diagram_type) = parse::codeblocks::mermaid_diagram_type(&req.diagram) else {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Diagram must start with a Mermaid diagram type (one of: {})",
                parse::codeblocks::MERMAID_DIAGRAM_TYPES.join(", "),
            ))]));
        };
        let result = self.vault.write_code_block(&req.path, "mermaid", &req.diagram, req.index, req.heading.as_deref());
        respond(result, |(path, index)| {
            serde_json::json!({
                "path": path,
                "index": index,
                "diagram_type": diagram_type,
                "message": "Mermaid diagram written successfully",
            })
        })
    }

    #[tool(description = "Insert or replace an Obsidian Charts plugin chart (```chart block) built from labels and data series: bar, line, pie, doughnut, radar, or polarArea.")]
    fn write_chart(
        &self,
        Parameters(req): Parameters<tools::diagrams::WriteChartRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut options = serde_json::Map::new();
        let typed = [
            ("width", req.width.map(serde_json::Value::from)),
            ("beginAtZero", req.begin_at_zero.map(serde_json::Value::from)),
            ("stacked", req.stacked.map(serde_json::Value::from)),
            ("fill", req.fill.map(serde_json::Value::from)),
            ("tension", req.tension.map(serde_json::Value::from)),
        ];
        for (key, value) in typed {
            if let Some(v) = value {
                options.insert(key.to_string(), v);
            }
        }
        options.extend(req.extra_options.unwrap_or_default());

        let series: Vec<parse::codeblocks::ChartSeries> = req.series.iter()
            .map(|s| parse::codeblocks::ChartSeries { title: s.title.as_deref(), data: &s.data })
            .collect();
        let result = parse::codeblocks::build_chart_yaml(&req.chart_type, &req.labels, &series, &options)
            .and_then(|yaml| self.vault.write_code_block(&req.path, "chart", &yaml, req.index, req.heading.as_deref()));
        respond(result, |(path, index)| {
            serde_json::json!({
                "path": path,
                "index": index,
                "message": "Chart written successfully",
            })
        })
    }

    // ===== Canvas Tools (Canvas / Advanced Canvas) =====

    #[tool(description = "Read an Obsidian .canvas file: all nodes (text, file, link, group) and edges, including Advanced Canvas styleAttributes.")]
    fn read_canvas(
        &self,
        Parameters(req): Parameters<tools::canvas::ReadCanvasRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        respond(self.vault.read_canvas(&req.path), |(path, doc)| {
            let count = |k: &str| doc[k].as_array().map_or(0, Vec::len);
            serde_json::json!({
                "path": path,
                "node_count": count("nodes"),
                "edge_count": count("edges"),
                "nodes": doc["nodes"],
                "edges": doc["edges"],
            })
        })
    }

    #[tool(description = "Create a new Obsidian .canvas file with nodes and edges. Nodes without x/y are auto-placed. Supports Advanced Canvas styleAttributes (node shapes/borders, edge path/arrow styles).")]
    fn create_canvas(
        &self,
        Parameters(req): Parameters<tools::canvas::CreateCanvasRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let nodes = req.nodes.unwrap_or_default();
        let edges = req.edges.unwrap_or_default();
        respond(self.vault.create_canvas(&req.path, &nodes, &edges), |(path, summary)| {
            serde_json::json!({
                "path": path,
                "created": summary.created,
                "message": "Canvas created successfully",
            })
        })
    }

    #[tool(description = "Edit an Obsidian .canvas file: add or update nodes/edges (matched by id; only given fields change) and remove nodes/edges by id. Supports Advanced Canvas styleAttributes.")]
    fn edit_canvas(
        &self,
        Parameters(req): Parameters<tools::canvas::EditCanvasRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let nodes = req.nodes.unwrap_or_default();
        let edges = req.edges.unwrap_or_default();
        let remove = req.remove_ids.unwrap_or_default();
        respond(self.vault.edit_canvas(&req.path, &nodes, &edges, &remove), |(path, summary)| {
            serde_json::json!({
                "path": path,
                "created": summary.created,
                "updated": summary.updated,
                "removed": summary.removed,
                "message": "Canvas updated successfully",
            })
        })
    }
}

fn board_json(path: &str, board: &parse::kanban::Board) -> serde_json::Value {
    serde_json::json!({
        "path": path,
        "lanes": board.lanes,
        "archive": board.archive.as_ref().map(|a| &a.cards),
    })
}

#[tool_handler]
impl ServerHandler for ObsidianMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Obsidian vault management MCP server. Read, write, search notes, manage links, templates, and graph analysis. Also supports plugin formats: Excalidraw drawings, Advanced Tables (markdown tables), Kanban boards, Mermaid diagrams, Obsidian Charts, and Canvas / Advanced Canvas files.",
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
