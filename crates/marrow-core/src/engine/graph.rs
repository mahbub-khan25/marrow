//! Data for the graph view: notes as nodes, links as edges.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphNodeKind {
    Note,
    /// An image, PDF or other file some note links to.
    Attachment,
    /// A link target that doesn't exist yet.
    Unresolved,
    /// A `#tag`, linked to every note that carries it.
    Tag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    /// Vault path, or `?<link text>` for unresolved targets.
    pub id: String,
    pub name: String,
    pub kind: GraphNodeKind,
    /// Links in and out, counting each linked note once.
    pub degree: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GraphOptions {
    pub show_unresolved: bool,
    pub show_attachments: bool,
    pub show_tags: bool,
}

impl Engine {
    pub fn graph_data(&self, opts: GraphOptions) -> GraphData {
        let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new();
        let mut edges: Vec<GraphEdge> = Vec::new();

        for (path, meta) in &self.metas {
            let mut tags: Vec<String> = meta
                .all_tags()
                .into_iter()
                .map(|t| t.to_lowercase())
                .collect();
            tags.sort();
            tags.dedup();
            nodes.insert(
                path.to_string(),
                GraphNode {
                    id: path.to_string(),
                    name: path.stem().to_string(),
                    kind: GraphNodeKind::Note,
                    degree: 0,
                    tags,
                },
            );
        }

        for (src, targets) in &self.graph.resolved {
            for t in targets.keys() {
                if t == src {
                    continue;
                }
                let id = t.to_string();
                if !nodes.contains_key(&id) {
                    if !opts.show_attachments || t.is_markdown() {
                        continue;
                    }
                    nodes.insert(
                        id.clone(),
                        GraphNode {
                            id: id.clone(),
                            name: t.name().to_string(),
                            kind: GraphNodeKind::Attachment,
                            degree: 0,
                            tags: vec![],
                        },
                    );
                }
                edges.push(GraphEdge {
                    source: src.to_string(),
                    target: id,
                });
            }
        }

        if opts.show_unresolved {
            for (src, names) in &self.graph.unresolved {
                for name in names.keys() {
                    let id = format!("?{}", name.to_lowercase());
                    nodes.entry(id.clone()).or_insert_with(|| GraphNode {
                        id: id.clone(),
                        name: name.rsplit('/').next().unwrap_or(name).to_string(),
                        kind: GraphNodeKind::Unresolved,
                        degree: 0,
                        tags: vec![],
                    });
                    edges.push(GraphEdge {
                        source: src.to_string(),
                        target: id,
                    });
                }
            }
        }

        if opts.show_tags {
            for (path, meta) in &self.metas {
                let mut tags: Vec<String> = meta
                    .all_tags()
                    .into_iter()
                    .map(|t| format!("#{}", t.trim_start_matches('#').to_lowercase()))
                    .collect();
                tags.sort();
                tags.dedup();
                for tag in tags {
                    nodes.entry(tag.clone()).or_insert_with(|| GraphNode {
                        id: tag.clone(),
                        name: tag.clone(),
                        kind: GraphNodeKind::Tag,
                        degree: 0,
                        tags: vec![],
                    });
                    edges.push(GraphEdge {
                        source: path.to_string(),
                        target: tag,
                    });
                }
            }
        }

        // Undirected degree, counting a pair once even if both link each other.
        let mut seen: HashMap<(String, String), ()> = HashMap::new();
        for e in &edges {
            let key = if e.source < e.target {
                (e.source.clone(), e.target.clone())
            } else {
                (e.target.clone(), e.source.clone())
            };
            if seen.insert(key, ()).is_none() {
                if let Some(n) = nodes.get_mut(&e.source) {
                    n.degree += 1;
                }
                if let Some(n) = nodes.get_mut(&e.target) {
                    n.degree += 1;
                }
            }
        }
        edges.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));
        edges.dedup();

        GraphData {
            nodes: nodes.into_values().collect(),
            edges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use std::fs;

    #[test]
    fn nodes_edges_and_options() {
        let d = tempfile::tempdir().unwrap();
        fs::write(
            d.path().join("A.md"),
            "[[B]] [[B]] [[Ghost]] ![[pic.png]] [[A]] #x\n",
        )
        .unwrap();
        fs::write(d.path().join("B.md"), "[[A]]\n").unwrap();
        fs::write(d.path().join("Lonely.md"), "no links\n").unwrap();
        fs::write(d.path().join("pic.png"), [0u8]).unwrap();
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();

        let g = e.graph_data(GraphOptions::default());
        let ids: Vec<_> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["A.md", "B.md", "Lonely.md"]);
        assert_eq!(g.edges.len(), 2); // A→B, B→A; no self-loop
        let a = &g.nodes[0];
        assert_eq!(a.degree, 1);
        assert_eq!(a.tags, vec!["#x"]);

        let g = e.graph_data(GraphOptions {
            show_unresolved: true,
            show_attachments: true,
            show_tags: false,
        });
        let ids: Vec<_> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["?ghost", "A.md", "B.md", "Lonely.md", "pic.png"]);
        assert_eq!(g.nodes[0].kind, GraphNodeKind::Unresolved);
        assert_eq!(g.nodes[4].kind, GraphNodeKind::Attachment);
        assert_eq!(g.nodes[1].degree, 3);

        // Tags become nodes linked to the notes that carry them.
        let g = e.graph_data(GraphOptions {
            show_tags: true,
            ..Default::default()
        });
        let tag = g.nodes.iter().find(|n| n.id == "#x").unwrap();
        assert_eq!(tag.kind, GraphNodeKind::Tag);
        assert!(g
            .edges
            .iter()
            .any(|e| e.source == "A.md" && e.target == "#x"));
    }

    #[test]
    fn note_info_and_stats() {
        let d = tempfile::tempdir().unwrap();
        fs::write(
            d.path().join("A.md"),
            "---\ntags: [Meta]\n---\n[[B]] #x #X #y\n",
        )
        .unwrap();
        fs::write(d.path().join("B.md"), "[[A]] [[A]] [[Missing]] [[#Top]]\n").unwrap();
        fs::write(d.path().join("Board.canvas"), "{}").unwrap();
        fs::write(d.path().join("pic.png"), "x").unwrap();
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        let a = crate::path::VaultPath::new("A.md").unwrap();
        let info = e.note_info(&a).unwrap();
        assert_eq!(info.tags, vec!["#x", "#y", "#Meta"]);
        assert_eq!(info.backlinks, 1);
        assert!(info.modified_ms > 0);
        let s = e.vault_stats();
        assert_eq!((s.notes, s.links), (2, 2));
        // A link to a heading in the same note isn't a backlink.
        let b = crate::path::VaultPath::new("B.md").unwrap();
        assert_eq!(e.note_info(&b).unwrap().backlinks, 1);
        // Notes and canvases, not other files.
        let mut recent: Vec<String> = e.recent_files(10).into_iter().map(|r| r.path).collect();
        recent.sort();
        assert_eq!(recent, vec!["A.md", "B.md", "Board.canvas"]);
        assert_eq!(e.recent_files(1).len(), 1);
    }
}
