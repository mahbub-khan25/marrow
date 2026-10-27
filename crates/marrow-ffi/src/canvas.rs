//! Canvas boards: `.canvas` files in the open JSON Canvas format (the same
//! files Obsidian and the desktop app open), with Marrow's shapes and pen
//! drawings. Fields the apps don't know are kept as they were.

use serde_json::{json, Map, Value};

use crate::{fail, vp, Marrow, Result};

/// A pen stroke or a free arrow, drawn inside the card's box.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Ink {
    /// The box the points were drawn in (the card scales it).
    pub w: f64,
    pub h: f64,
    /// Line width.
    pub size: f64,
    /// x, y, x, y… inside that box.
    pub points: Vec<f64>,
    pub arrow: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CanvasNode {
    pub id: String,
    /// "text", "file", "link" or "group".
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    /// A vault path, for a "file" card.
    pub file: String,
    pub url: String,
    /// A group's name.
    pub label: String,
    /// "1"–"6" (red, orange, yellow, green, cyan, purple) or "#rrggbb", or "".
    pub color: String,
    /// "rect", "ellipse" or "diamond" for a shape, else "".
    pub shape: String,
    pub ink: Option<Ink>,
    /// Other fields, as JSON, kept when saving.
    pub extra: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CanvasEdge {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    /// "top", "right", "bottom", "left" or "".
    pub from_side: String,
    pub to_side: String,
    pub label: String,
    pub color: String,
    pub extra: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Board {
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
}

const NODE_KEYS: &[&str] = &["id", "type", "x", "y", "width", "height", "text", "file", "url", "label", "color", "shape", "ink"];
const EDGE_KEYS: &[&str] = &["id", "fromNode", "toNode", "fromSide", "toSide", "label", "color"];
/// What a drawing says in apps that can't show it.
const INK_TEXT: &str = "✏️ Drawing — open this canvas in Marrow to see it.";

fn s(o: &Map<String, Value>, k: &str) -> String {
    o.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn n(o: &Map<String, Value>, k: &str) -> f64 {
    o.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn extra(o: &Map<String, Value>, known: &[&str]) -> String {
    let rest: Map<String, Value> = o.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    if rest.is_empty() {
        String::new()
    } else {
        Value::Object(rest).to_string()
    }
}

fn put(o: &mut Map<String, Value>, k: &str, v: &str) {
    if !v.is_empty() {
        o.insert(k.into(), v.into());
    }
}

fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

pub fn parse_canvas(text: &str) -> Result<Board> {
    if text.trim().is_empty() {
        return Ok(Board { nodes: vec![], edges: vec![] });
    }
    let v: Value = serde_json::from_str(text).map_err(|e| fail(format!("This canvas file can't be read: {e}")))?;
    let mut c = Board { nodes: vec![], edges: vec![] };
    for o in v.get("nodes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object) {
        let ink = o.get("ink").and_then(Value::as_object).and_then(|i| {
            let pts: Vec<f64> = i.get("points")?.as_str()?.split_whitespace().filter_map(|p| p.split_once(',')).filter_map(|(x, y)| Some([x.parse().ok()?, y.parse().ok()?])).flatten().collect();
            Some(Ink { w: n(i, "w").max(1.0), h: n(i, "h").max(1.0), size: n(i, "size").max(1.0), points: pts, arrow: i.get("arrow").and_then(Value::as_bool).unwrap_or(false) })
        });
        c.nodes.push(CanvasNode {
            id: s(o, "id"),
            kind: s(o, "type"),
            x: n(o, "x"),
            y: n(o, "y"),
            width: n(o, "width"),
            height: n(o, "height"),
            text: s(o, "text"),
            file: s(o, "file"),
            url: s(o, "url"),
            label: s(o, "label"),
            color: s(o, "color"),
            shape: s(o, "shape"),
            ink,
            extra: extra(o, NODE_KEYS),
        });
    }
    for o in v.get("edges").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object) {
        c.edges.push(CanvasEdge {
            id: s(o, "id"),
            from_node: s(o, "fromNode"),
            to_node: s(o, "toNode"),
            from_side: s(o, "fromSide"),
            to_side: s(o, "toSide"),
            label: s(o, "label"),
            color: s(o, "color"),
            extra: extra(o, EDGE_KEYS),
        });
    }
    Ok(c)
}

pub fn canvas_json(c: &Board) -> String {
    let rest = |e: &str| -> Map<String, Value> { serde_json::from_str::<Value>(e).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default() };
    let nodes: Vec<Value> = c
        .nodes
        .iter()
        .map(|nd| {
            let mut o = Map::new();
            o.insert("id".into(), nd.id.clone().into());
            o.insert("type".into(), nd.kind.clone().into());
            for (k, v) in [("x", nd.x), ("y", nd.y), ("width", nd.width), ("height", nd.height)] {
                o.insert(k.into(), num(v.round()));
            }
            let text = if nd.ink.is_some() && nd.text.is_empty() { INK_TEXT.to_string() } else { nd.text.clone() };
            if nd.kind == "text" {
                o.insert("text".into(), text.into());
            }
            put(&mut o, "file", &nd.file);
            put(&mut o, "url", &nd.url);
            put(&mut o, "label", &nd.label);
            put(&mut o, "color", &nd.color);
            put(&mut o, "shape", &nd.shape);
            if let Some(i) = &nd.ink {
                let pts: Vec<String> = i.points.chunks(2).filter(|p| p.len() == 2).map(|p| format!("{},{}", (p[0] * 10.0).round() / 10.0, (p[1] * 10.0).round() / 10.0)).collect();
                let mut ink = json!({ "w": num(i.w.round()), "h": num(i.h.round()), "size": num(i.size), "points": pts.join(" ") });
                if i.arrow {
                    ink["arrow"] = true.into();
                }
                o.insert("ink".into(), ink);
            }
            o.extend(rest(&nd.extra));
            Value::Object(o)
        })
        .collect();
    let edges: Vec<Value> = c
        .edges
        .iter()
        .map(|e| {
            let mut o = Map::new();
            o.insert("id".into(), e.id.clone().into());
            o.insert("fromNode".into(), e.from_node.clone().into());
            o.insert("toNode".into(), e.to_node.clone().into());
            put(&mut o, "fromSide", &e.from_side);
            put(&mut o, "toSide", &e.to_side);
            put(&mut o, "label", &e.label);
            put(&mut o, "color", &e.color);
            o.extend(rest(&e.extra));
            Value::Object(o)
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "nodes": nodes, "edges": edges })).unwrap_or_default()
}

#[uniffi::export]
impl Marrow {
    pub fn read_canvas(&self, path: String) -> Result<Board> {
        let p = vp(&path)?;
        let text = self.with(|e| Ok(e.read_note(&p)?))?;
        parse_canvas(&text)
    }

    /// Saves a canvas; `expected` is the canvas as read, so a change made
    /// elsewhere meanwhile isn't overwritten (`ChangedElsewhere`).
    pub fn write_canvas(&self, path: String, canvas: Board, expected: Option<Board>) -> Result<()> {
        let p = vp(&path)?;
        let events = self.with(|e| {
            if let Some(exp) = &expected {
                let disk = e.read_note(&p).ok().map(|t| parse_canvas(&t));
                if let Some(Ok(d)) = disk {
                    if &d != exp {
                        return Err(crate::MarrowError::ChangedElsewhere);
                    }
                }
            }
            Ok(e.write_note(&p, &canvas_json(&canvas))?)
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(())
    }

    /// Makes a new, empty canvas in `folder`; returns its path.
    pub fn create_canvas(&self, folder: String, name: String) -> Result<String> {
        self.create_file(folder, name, "canvas".into(), "{\n  \"nodes\": [],\n  \"edges\": []\n}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvases_round_trip_and_keep_unknown_fields() {
        let t = r#"{"nodes":[{"id":"a","type":"text","text":"Hi","x":0,"y":10,"width":200,"height":80,"color":"4","styleAttributes":{"x":1}},
            {"id":"b","type":"file","file":"Note.md","x":300,"y":0,"width":250,"height":120},
            {"id":"c","type":"text","text":"✏️","x":0,"y":0,"width":10,"height":10,"ink":{"w":10,"h":10,"size":3,"points":"1,2 3.5,4"}}],
            "edges":[{"id":"e","fromNode":"a","toNode":"b","toSide":"left","toEnd":"arrow"}]}"#;
        let c = parse_canvas(t).unwrap();
        assert_eq!(c.nodes[0].color, "4");
        assert_eq!(c.nodes[1].file, "Note.md");
        assert_eq!(c.nodes[2].ink.as_ref().unwrap().points, [1.0, 2.0, 3.5, 4.0]);
        assert_eq!(c.edges[0].to_side, "left");
        let again = parse_canvas(&canvas_json(&c)).unwrap();
        assert_eq!(again, c);
        let out = canvas_json(&c);
        assert!(out.contains("styleAttributes") && out.contains("\"toEnd\": \"arrow\""));
        assert_eq!(parse_canvas("").unwrap().nodes.len(), 0);
    }
}
