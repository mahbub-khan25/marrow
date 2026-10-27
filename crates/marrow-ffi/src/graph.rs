//! The link graph, laid out here so both apps only draw it.

use std::collections::HashMap;

use marrow_core::engine::{GraphNodeKind, GraphOptions};

use crate::{vp, Marrow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NodeKind {
    Note,
    Attachment,
    /// A link to a note that doesn't exist yet.
    Missing,
    Tag,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct GraphNode {
    /// Vault path, `?link` for a missing note, or `#tag`.
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    /// Links in and out.
    pub degree: u32,
    /// Position, roughly within -1000…1000.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct GraphEdge {
    /// Indexes into `nodes`.
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// A force-directed layout: linked notes pull together, all notes push
/// apart (counted on a grid, so big vaults stay quick). Deterministic: the
/// same vault looks the same each time.
pub(crate) fn layout(n: usize, edges: &[(usize, usize)], iterations: usize) -> Vec<(f32, f32)> {
    if n == 0 {
        return vec![];
    }
    // Start on a sunflower spiral.
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let spread = 40.0 * (n as f32).sqrt();
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let r = spread * ((i as f32 + 0.5) / n as f32).sqrt();
            let a = i as f32 * golden;
            (r * a.cos(), r * a.sin())
        })
        .collect();
    let ideal = 90.0f32;
    let cell = ideal * 3.0;
    let mut temp = spread / 4.0 + ideal;
    for _ in 0..iterations {
        let mut disp = vec![(0f32, 0f32); n];
        // Repulsion between nearby nodes.
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, p) in pos.iter().enumerate() {
            grid.entry(((p.0 / cell).floor() as i32, (p.1 / cell).floor() as i32)).or_default().push(i);
        }
        for (&(gx, gy), members) in &grid {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(others) = grid.get(&(gx + dx, gy + dy)) else { continue };
                    for &i in members {
                        for &j in others {
                            if i == j {
                                continue;
                            }
                            let (ddx, ddy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                            let d2 = (ddx * ddx + ddy * ddy).max(0.01);
                            let f = ideal * ideal / d2;
                            disp[i].0 += ddx * f;
                            disp[i].1 += ddy * f;
                        }
                    }
                }
            }
        }
        // Attraction along links.
        for &(a, b) in edges {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = d / ideal;
            disp[a].0 -= dx * f / 2.0;
            disp[a].1 -= dy * f / 2.0;
            disp[b].0 += dx * f / 2.0;
            disp[b].1 += dy * f / 2.0;
        }
        // A gentle pull to the middle keeps islands from drifting off.
        for (i, p) in pos.iter_mut().enumerate() {
            let (mut dx, mut dy) = disp[i];
            dx -= p.0 * 0.06;
            dy -= p.1 * 0.06;
            let len = (dx * dx + dy * dy).sqrt().max(0.01);
            let step = len.min(temp);
            p.0 += dx / len * step;
            p.1 += dy / len * step;
        }
        temp = (temp * 0.95).max(1.0);
    }
    pos
}

#[uniffi::export]
impl Marrow {
    /// The whole vault's graph (or, with `around`, a note and what's within
    /// `depth` links of it), laid out.
    pub fn graph(&self, show_tags: bool, show_missing: bool, show_attachments: bool, around: Option<String>, depth: u32) -> Result<Graph> {
        let data = self.with(|e| Ok(e.graph_data(GraphOptions { show_unresolved: show_missing, show_attachments, show_tags })))?;
        let mut keep: Option<std::collections::HashSet<String>> = None;
        if let Some(center) = around {
            let c = vp(&center)?.to_string();
            let mut set = std::collections::HashSet::from([c]);
            for _ in 0..depth.max(1) {
                let add: Vec<String> = data
                    .edges
                    .iter()
                    .filter_map(|e| {
                        if set.contains(&e.source) {
                            Some(e.target.clone())
                        } else if set.contains(&e.target) {
                            Some(e.source.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                set.extend(add);
            }
            keep = Some(set);
        }
        let nodes: Vec<_> = data.nodes.into_iter().filter(|n| keep.as_ref().is_none_or(|k| k.contains(&n.id))).collect();
        let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
        let edges: Vec<(usize, usize)> = data.edges.iter().filter_map(|e| Some((*index.get(e.source.as_str())?, *index.get(e.target.as_str())?))).filter(|(a, b)| a != b).collect();
        let iterations = if nodes.len() > 2000 { 120 } else { 250 };
        let pos = layout(nodes.len(), &edges, iterations);
        Ok(Graph {
            nodes: nodes
                .iter()
                .zip(pos)
                .map(|(n, (x, y))| GraphNode {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    kind: match n.kind {
                        GraphNodeKind::Note => NodeKind::Note,
                        GraphNodeKind::Attachment => NodeKind::Attachment,
                        GraphNodeKind::Unresolved => NodeKind::Missing,
                        GraphNodeKind::Tag => NodeKind::Tag,
                    },
                    degree: n.degree,
                    x,
                    y,
                })
                .collect(),
            edges: edges.into_iter().map(|(a, b)| GraphEdge { from: a as u32, to: b as u32 }).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_nodes_end_up_closer() {
        let pos = layout(4, &[(0, 1), (2, 3)], 200);
        let d = |a: usize, b: usize| ((pos[a].0 - pos[b].0).powi(2) + (pos[a].1 - pos[b].1).powi(2)).sqrt();
        assert!(d(0, 1) < d(0, 2));
        assert!(pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    }

    #[test]
    fn graph_of_a_vault() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("A.md", "[[B]] [[Nowhere]] #t"), ("B.md", "[[C]]"), ("C.md", ""), ("D.md", "")]);
        let g = m.graph(true, true, false, None, 1).unwrap();
        assert_eq!(g.nodes.len(), 6);
        assert!(g.edges.len() >= 3, "{:?}", g.edges);
        let g = m.graph(false, false, false, Some("A.md".into()), 1).unwrap();
        let mut names: Vec<_> = g.nodes.iter().map(|n| n.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["A", "B"]);
    }
}
