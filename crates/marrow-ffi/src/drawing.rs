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

/// A pen stroke as a canvas card. `points` are x, y, x, y… on the drawing;
/// `zoom` is how much the drawing is enlarged on screen (to keep strokes as
/// smooth as they look). `color` is "1"–"6", "#rrggbb" or "".
///
/// `kind` is:
/// - "pen": the stroke as drawn;
/// - "line": a straight line from the first point to the last;
/// - "arrow" / "double": a straight arrow, or one with a head at each end;
/// - "highlighter": a wide, see-through stroke.
#[uniffi::export]
pub fn stroke_node(points: Vec<f64>, size: f64, kind: String, color: String, zoom: f64) -> Option<CanvasNode> {
    let arrow = kind == "arrow" || kind == "double";
    let start_arrow = kind == "double";
    let highlight = kind == "highlighter";
    let straight = kind == "line";
    let pts: Vec<(f64, f64)> = points.chunks(2).filter(|p| p.len() == 2 && p[0].is_finite() && p[1].is_finite()).map(|p| (p[0], p[1])).collect();
    let (first, last) = (*pts.first()?, *pts.last()?);
    let straight = straight || arrow || start_arrow;
    let mut ps = if straight {
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
    let size = if arrow || start_arrow { 2.5 } else if highlight { size.clamp(1.0, 40.0).max(14.0) } else { size.clamp(1.0, 40.0) };
    let pad = size + if arrow || start_arrow { 12.0 } else { 4.0 };
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
        fill: String::new(),
        ink: Some(Ink { w, h, size, points: ps.iter().flat_map(|p| [r(p.0 - x0), r(p.1 - y0)]).collect(), arrow, start_arrow, highlight }),
        extra: String::new(),
    })
}

/// A shape dragged from one corner to the other (a tap makes a standard
/// size). `shape` is one of `shape_kinds()`; `fill` is "" (light), "none"
/// (see-through) or "solid".
#[uniffi::export]
pub fn shape_node(shape: String, x0: f64, y0: f64, x1: f64, y1: f64, color: String, fill: String) -> CanvasNode {
    let small = (x1 - x0).abs() < 8.0 && (y1 - y0).abs() < 8.0;
    let (x, y, w, h) = if small { (x0 - 80.0, y0 - 50.0, 160.0, 100.0) } else { (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()) };
    let shape = if shape_geometry(shape.clone()).is_some() { shape } else { "rect".into() };
    let fill = if matches!(fill.as_str(), "none" | "solid") { fill } else { String::new() };
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
        fill,
        ink: None,
        extra: String::new(),
    }
}

/// A shape's outline in a 100 × 100 box (scaled to the card), from the one
/// list every app uses (`marrow-core/data/shapes.json`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ShapeGeometry {
    pub kind: String,
    /// Its name ("Rounded rectangle").
    pub name: String,
    /// The outline, as SVG path data using only M, L, C and Z.
    pub outline: String,
    /// More lines drawn on it, not filled (the top of a cylinder), or "".
    pub extra: String,
    /// Where words go: x0, y0, x1, y1 in the same 100 × 100 box.
    pub label: Vec<f64>,
}

fn shapes() -> &'static [ShapeGeometry] {
    static SHAPES: std::sync::OnceLock<Vec<ShapeGeometry>> = std::sync::OnceLock::new();
    SHAPES.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../marrow-core/data/shapes.json")).unwrap_or_default();
        v["shapes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| ShapeGeometry {
                kind: s["kind"].as_str().unwrap_or_default().to_string(),
                name: s["name"].as_str().unwrap_or_default().to_string(),
                outline: s["d"].as_str().unwrap_or_default().to_string(),
                extra: s["extra"].as_str().unwrap_or_default().to_string(),
                label: s["label"].as_array().into_iter().flatten().filter_map(|x| x.as_f64()).collect(),
            })
            .collect()
    })
}

/// Every shape that can be drawn, in the order the apps show them.
#[uniffi::export]
pub fn shape_kinds() -> Vec<ShapeGeometry> {
    shapes().to_vec()
}

/// One shape's outline, or None for a kind that isn't known.
#[uniffi::export]
pub fn shape_geometry(kind: String) -> Option<ShapeGeometry> {
    shapes().iter().find(|s| s.kind == kind).cloned()
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
        fill: String::new(),
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

/// The note's text without the embed line(s) of the drawing `target` (its
/// link or its vault path); when
/// that line sat between two blank lines, one of them goes too.
#[uniffi::export]
pub fn drawing_removed(text: String, target: String) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    // `target` may be the link as written or the file's vault path.
    let name = |t: &str| t.rsplit('/').next().unwrap_or(t).to_string();
    let want = name(&target);
    let is_it = |l: &str| drawing_in_line(l.to_string()).is_some_and(|d| name(&d.target) == want);
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if is_it(lines[i]) {
            let before_blank = out.last().is_some_and(|l| l.trim().is_empty());
            let after_blank = lines.get(i + 1).is_some_and(|l| l.trim().is_empty());
            i += if before_blank && after_blank { 2 } else { 1 };
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    out.join("\n")
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
    fn removing_a_drawing() {
        let t = "# Plan\n\nIntro.\n\n![[Drawing 1.canvas|360]]\n\nMore.\n![[Other.canvas]]\n";
        assert_eq!(drawing_removed(t.into(), "Drawing 1.canvas".into()), "# Plan\n\nIntro.\n\nMore.\n![[Other.canvas]]\n");
        assert_eq!(drawing_removed("a\n![[Drawing 1.canvas]]\nb".into(), "Attachments/Drawing 1.canvas".into()), "a\nb");
        assert_eq!(drawing_removed("a\nsee ![[Drawing 1.canvas]] here".into(), "Drawing 1.canvas".into()), "a\nsee ![[Drawing 1.canvas]] here");
    }

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
        let n = stroke_node(vec![10.0, 10.0, 20.0, 10.2, 30.0, 10.0, 40.0, 10.1], 4.0, "pen".into(), "5".into(), 1.0).unwrap();
        let ink = n.ink.as_ref().unwrap();
        assert_eq!(ink.points.len(), 4);
        assert_eq!((n.x, n.y, n.width), (2.0, 2.0, 46.0));
        assert_eq!(&ink.points[..2], &[8.0, 8.0]);
        assert_eq!(n.color, "5");
        // A dot still draws; a tiny arrow doesn't.
        assert!(stroke_node(vec![5.0, 5.0], 4.0, "pen".into(), "".into(), 1.0).is_some());
        assert!(stroke_node(vec![5.0, 5.0, 7.0, 6.0], 4.0, "arrow".into(), "".into(), 1.0).is_none());
        let a = stroke_node(vec![0.0, 0.0, 50.0, 80.0, 100.0, 0.0], 4.0, "double".into(), "".into(), 1.0).unwrap();
        let ink = a.ink.clone().unwrap();
        assert!(ink.arrow && ink.start_arrow && ink.points.len() == 4);
        // A straight line keeps only its ends; a highlighter is wide.
        let l = stroke_node(vec![0.0, 0.0, 50.0, 80.0, 100.0, 0.0], 4.0, "line".into(), "".into(), 1.0).unwrap();
        assert_eq!(l.ink.unwrap().points.len(), 4);
        let hl = stroke_node(vec![0.0, 0.0, 50.0, 80.0, 100.0, 0.0], 4.0, "highlighter".into(), "3".into(), 1.0).unwrap();
        assert!(hl.ink.as_ref().unwrap().highlight && hl.ink.unwrap().size >= 14.0);
        // Shapes: dragged, or a tap for a standard size.
        let s = shape_node("ellipse".into(), 100.0, 50.0, 20.0, 10.0, "".into(), "none".into());
        assert_eq!(s.fill, "none");
        assert_eq!((s.x, s.y, s.width, s.height, s.shape.as_str()), (20.0, 10.0, 80.0, 40.0, "ellipse"));
        let t = shape_node("blob".into(), 100.0, 100.0, 101.0, 101.0, "".into(), "glass".into());
        assert_eq!((t.width, t.height, t.shape.as_str(), t.fill.as_str()), (160.0, 100.0, "rect", ""));
        assert_eq!(shape_node("star".into(), 0.0, 0.0, 50.0, 50.0, "".into(), "solid".into()).shape, "star");
        // They save as the desktop app writes them.
        let board = crate::canvas::Board { nodes: vec![n, s, a, hl_clone()], edges: vec![] };
        let out = crate::canvas::canvas_json(&board);
        assert!(out.contains("\"ink\"") && out.contains("\"shape\": \"ellipse\"") && out.contains("\"fill\": \"none\"") && out.contains("\"arrowStart\": true") && out.contains("Drawing —"));
        // Read back the same (drawings also say "open in Marrow" for other apps).
        let mut back = crate::canvas::parse_canvas(&out).unwrap();
        for n in &mut back.nodes {
            if n.ink.is_some() {
                n.text.clear();
            }
        }
        assert_eq!(back, board);
    }

    fn hl_clone() -> CanvasNode {
        stroke_node(vec![0.0, 0.0, 30.0, 0.0], 4.0, "highlighter".into(), "".into(), 1.0).unwrap()
    }

    #[test]
    fn the_shape_list() {
        let all = shape_kinds();
        assert!(all.len() >= 11, "{}", all.len());
        for s in &all {
            assert!(!s.name.is_empty() && s.label.len() == 4, "{}", s.kind);
            // Only the commands every app draws.
            for c in s.outline.chars().chain(s.extra.chars()) {
                assert!(c.is_ascii_digit() || " ,.-MLCZ".contains(c), "{}: {c}", s.kind);
            }
            assert!(s.outline.starts_with('M') && s.outline.ends_with('Z'), "{}", s.kind);
        }
        assert!(!shape_geometry("cylinder".into()).unwrap().extra.is_empty());
        assert!(shape_geometry("blob".into()).is_none());
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
