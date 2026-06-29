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
    #[allow(dead_code)]
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
