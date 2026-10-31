//! Drawings inside notes. A drawing is a small canvas (a `.canvas` file in
//! the attachments folder, the same JSON Canvas format as boards) shown in
//! the note where its embed sits, at the height given after the bar:
//!
//! ```text
//! ![[Drawing 2026-10-31 174312.canvas|360]]
//! ```
//!
//! The apps draw on it in place. These helpers turn a finger or pen stroke,
//! or a dragged shape, into the canvas's cards the same way on every device
//! (and the same way the desktop app does).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::canvas::{CanvasNode, Ink};
use crate::{Marrow, Result};

/// How tall a new drawing is in a note (points / dp / CSS pixels).
pub const DEFAULT_HEIGHT: u32 = 360;
const MIN_HEIGHT: u32 = 120;
const MAX_HEIGHT: u32 = 2000;

/// Where a drawing sits in a note, read from its embed line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DrawingRef {
    /// The link as written (`Drawing 2026-10-31 174312.canvas`).
    pub target: String,
    /// Its height in the note.
    pub height: u32,
}

fn new_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seed = format!("{t}-{}-{:p}", N.fetch_add(1, Ordering::Relaxed), &N);
    marrow_core::cache::hash_bytes(seed.as_bytes())[..16].to_string()
}

/// Drops points that barely change the line (Ramer–Douglas–Peucker).
fn simplify(pts: &[(f64, f64)], eps: f64) -> Vec<(f64, f64)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(1e-9);
    let (mut far, mut at) = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = ((b.1 - a.1) * p.0 - (b.0 - a.0) * p.1 + b.0 * a.1 - b.1 * a.0).abs() / len;
        if d > far {
            far = d;
            at = i;
        }
    }
    if far <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&pts[..=at], eps);
    left.pop();
    left.extend(simplify(&pts[at..], eps));
    left
}

/// A pen stroke (or, with `arrow`, a straight arrow from the first point to
/// the last) as a canvas card. `points` are x, y, x, y… on the drawing;
/// `zoom` is how much the drawing is enlarged on screen (to keep strokes as
/// smooth as they look). `color` is "1"–"6", "#rrggbb" or "".
#[uniffi::export]
pub fn stroke_node(points: Vec<f64>, size: f64, arrow: bool, color: String, zoom: f64) -> Option<CanvasNode> {
    let pts: Vec<(f64, f64)> = points.chunks(2).filter(|p| p.len() == 2 && p[0].is_finite() && p[1].is_finite()).map(|p| (p[0], p[1])).collect();
    let (first, last) = (*pts.first()?, *pts.last()?);
    let mut ps = if arrow {
        if ((last.0 - first.0).powi(2) + (last.1 - first.1).powi(2)).sqrt() < 8.0 {
            return None;
        }
        vec![first, last]
    } else {
        simplify(&pts, 0.8 / zoom.max(0.1))
    };
    if ps.len() == 1 {
        ps.push((ps[0].0 + 0.5, ps[0].1 + 0.5));
    }
    let size = if arrow { 2.5 } else { size.clamp(1.0, 40.0) };
    let pad = size + if arrow { 12.0 } else { 4.0 };
    let x0 = (ps.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - pad).round();
    let y0 = (ps.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - pad).round();
    let w = (ps.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) + pad - x0).round().max(1.0);
    let h = (ps.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) + pad - y0).round().max(1.0);
    let r = |v: f64| (v * 10.0).round() / 10.0;
    Some(CanvasNode {
        id: new_id(),
        kind: "text".into(),
        x: x0,
        y: y0,
        width: w,
        height: h,
        text: String::new(),
        file: String::new(),
        url: String::new(),
        label: String::new(),
        color,
        shape: String::new(),
        ink: Some(Ink { w, h, size, points: ps.iter().flat_map(|p| [r(p.0 - x0), r(p.1 - y0)]).collect(), arrow }),
        extra: String::new(),
    })
}

/// A rectangle, ellipse or diamond dragged from one corner to the other (a
/// tap makes a standard size). `shape` is "rect", "ellipse" or "diamond".
#[uniffi::export]
pub fn shape_node(shape: String, x0: f64, y0: f64, x1: f64, y1: f64, color: String) -> CanvasNode {
    let small = (x1 - x0).abs() < 8.0 && (y1 - y0).abs() < 8.0;
    let (x, y, w, h) = if small { (x0 - 80.0, y0 - 50.0, 160.0, 100.0) } else { (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()) };
    let shape = if matches!(shape.as_str(), "ellipse" | "diamond") { shape } else { "rect".into() };
    CanvasNode {
        id: new_id(),
        kind: "text".into(),
        x: x.round(),
        y: y.round(),
        width: w.round().max(40.0),
        height: h.round().max(30.0),
        text: String::new(),
        file: String::new(),
        url: String::new(),
        label: String::new(),
        color,
        shape,
        ink: None,
        extra: String::new(),
    }
}

/// A text card at a point on the drawing.
#[uniffi::export]
pub fn text_node(text: String, x: f64, y: f64, color: String) -> CanvasNode {
    CanvasNode {
        id: new_id(),
        kind: "text".into(),
        x: (x - 90.0).round(),
        y: (y - 30.0).round(),
        width: 180.0,
        height: 60.0,
        text,
        file: String::new(),
        url: String::new(),
        label: String::new(),
        color,
        shape: String::new(),
        ink: None,
        extra: String::new(),
    }
}

/// The line that puts a drawing (by its path) into a note.
#[uniffi::export]
pub fn drawing_embed(path: String, height: u32) -> String {
    let name = path.rsplit('/').next().unwrap_or(&path);
    format!("![[{name}|{}]]", height.clamp(MIN_HEIGHT, MAX_HEIGHT))
}

/// The drawing on a line that is only a drawing's embed, if it is one.
#[uniffi::export]
pub fn drawing_in_line(line: String) -> Option<DrawingRef> {
    let t = line.trim();
    let inner = t.strip_prefix("![[")?.strip_suffix("]]")?;
    let (target, size) = match inner.split_once('|') {
        Some((a, b)) => (a.trim(), b.trim()),
        None => (inner.trim(), ""),
    };
    if !target.to_ascii_lowercase().ends_with(".canvas") || target.contains("]]") {
        return None;
    }
    let height = size.parse::<u32>().map(|h| h.clamp(MIN_HEIGHT, MAX_HEIGHT)).unwrap_or(DEFAULT_HEIGHT);
    Some(DrawingRef { target: target.to_string(), height })
}

/// The same embed line with a new height (other text on it is kept).
#[uniffi::export]
pub fn drawing_line_with_height(line: String, height: u32) -> String {
    let Some(d) = drawing_in_line(line.clone()) else { return line };
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    format!("{indent}![[{}|{}]]", d.target, height.clamp(MIN_HEIGHT, MAX_HEIGHT))
}

#[uniffi::export]
impl Marrow {
    /// Makes a new, empty drawing in the attachments folder and returns its
    /// path. `stamp` names it ("2026-10-31 174312").
    pub fn create_drawing(&self, stamp: String) -> Result<String> {
        self.create_canvas("Attachments".into(), format!("Drawing {}", stamp.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_lines() {
        assert_eq!(drawing_embed("Attachments/Drawing 1.canvas".into(), 360), "![[Drawing 1.canvas|360]]");
        assert_eq!(drawing_embed("a.canvas".into(), 5), "![[a.canvas|120]]");
        let d = drawing_in_line("  ![[Drawing 1.canvas|420]] ".into()).unwrap();
        assert_eq!((d.target.as_str(), d.height), ("Drawing 1.canvas", 420));
        assert_eq!(drawing_in_line("![[Board.canvas]]".into()).unwrap().height, DEFAULT_HEIGHT);
        assert!(drawing_in_line("![[photo.png|300]]".into()).is_none());
        assert!(drawing_in_line("see ![[a.canvas]]".into()).is_none());
        assert_eq!(drawing_line_with_height("  ![[a.canvas|360]]".into(), 500), "  ![[a.canvas|500]]");
        assert_eq!(drawing_line_with_height("text".into(), 500), "text");
    }

    #[test]
    fn strokes_and_shapes() {
        // A nearly straight stroke keeps only its ends; the card wraps it with room for the line.
        let n = stroke_node(vec![10.0, 10.0, 20.0, 10.2, 30.0, 10.0, 40.0, 10.1], 4.0, false, "5".into(), 1.0).unwrap();
        let ink = n.ink.as_ref().unwrap();
        assert_eq!(ink.points.len(), 4);
        assert_eq!((n.x, n.y, n.width), (2.0, 2.0, 46.0));
        assert_eq!(&ink.points[..2], &[8.0, 8.0]);
        assert_eq!(n.color, "5");
        // A dot still draws; a tiny arrow doesn't.
        assert!(stroke_node(vec![5.0, 5.0], 4.0, false, "".into(), 1.0).is_some());
        assert!(stroke_node(vec![5.0, 5.0, 7.0, 6.0], 4.0, true, "".into(), 1.0).is_none());
        let a = stroke_node(vec![0.0, 0.0, 50.0, 80.0, 100.0, 0.0], 4.0, true, "".into(), 1.0).unwrap();
        assert!(a.ink.unwrap().arrow);
        // Shapes: dragged, or a tap for a standard size.
        let s = shape_node("ellipse".into(), 100.0, 50.0, 20.0, 10.0, "".into());
        assert_eq!((s.x, s.y, s.width, s.height, s.shape.as_str()), (20.0, 10.0, 80.0, 40.0, "ellipse"));
        let t = shape_node("star".into(), 100.0, 100.0, 101.0, 101.0, "".into());
        assert_eq!((t.width, t.height, t.shape.as_str()), (160.0, 100.0, "rect"));
        // They save as the desktop app writes them.
        let out = crate::canvas::canvas_json(&crate::canvas::Board { nodes: vec![n, s], edges: vec![] });
        assert!(out.contains("\"ink\"") && out.contains("\"shape\": \"ellipse\"") && out.contains("Drawing —"));
    }

    #[test]
    fn drawings_in_notes() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[]);
        let p = m.create_drawing("2026-10-31 174312".into()).unwrap();
        assert_eq!(p, "Attachments/Drawing 2026-10-31 174312.canvas");
        let line = drawing_embed(p.clone(), DEFAULT_HEIGHT);
        m.write_note("Idea.md".into(), format!("# Idea\n\nScreens:\n\n{line}\n\nMore text\n"), None).unwrap();
        let text = m.read_note("Idea.md".into()).unwrap();
        let b = m.render_note("Idea.md".into(), text).unwrap();
        let d = b.iter().find(|b| b.kind == crate::render::BlockKind::Drawing).expect("a drawing block");
        assert_eq!((d.target.as_str(), d.number, d.local, d.line), (p.as_str(), DEFAULT_HEIGHT, true, 4));
        assert_eq!(m.read_canvas(p).unwrap().nodes.len(), 0);
    }
}
