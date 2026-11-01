//! The Home board on phones: its built-in templates (shared with the
//! desktop through `marrow-core/data/home-templates.json`), the board a new
//! Home starts with, and template files (`.marrowhome`) to share.
//!
//! A live card is a text node whose `extra` holds `widget`, `cfg` and
//! `style` (see `ui/src/home.ts` for the meaning of each).

use serde_json::{json, Map, Value};

use crate::canvas::{canvas_json, parse_canvas, Board};
use crate::drawing::new_id;
use crate::{MarrowError, Result};

const DATA: &str = include_str!("../../marrow-core/data/home-templates.json");

/// The file Home lives in, at the top of the vault.
#[uniffi::export]
pub fn home_file() -> String {
    "Home.canvas".into()
}

/// A built-in template.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HomeTemplate {
    pub name: String,
    /// A Phosphor icon name ("calendar-check").
    pub icon: String,
    pub about: String,
}

/// A template file read back.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HomeTemplateFile {
    pub name: String,
    pub board: Board,
}

fn data() -> &'static Value {
    static D: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    D.get_or_init(|| serde_json::from_str(DATA).expect("home-templates.json is valid"))
}

fn templates() -> &'static [Value] {
    data()["templates"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// The built-in templates, the starter board first.
#[uniffi::export]
pub fn home_templates() -> Vec<HomeTemplate> {
    templates()
        .iter()
        .map(|t| HomeTemplate {
            name: t["name"].as_str().unwrap_or_default().into(),
            icon: t["icon"].as_str().unwrap_or_default().into(),
            about: t["about"].as_str().unwrap_or_default().into(),
        })
        .collect()
}

/// A built-in template's board, with new card ids; None for a wrong index.
#[uniffi::export]
pub fn home_template_board(index: u32) -> Option<Board> {
    let t = templates().get(index as usize)?;
    fresh(&t["board"]).ok()
}

/// What a new Home starts with: the starter board, plus "Rediscover" (when
/// old notes are shown) and "Getting started" (for a nearly empty vault).
#[uniffi::export]
pub fn home_starter_board(rediscover: bool, welcome: bool) -> Board {
    let mut b = home_template_board(0).unwrap_or(Board { nodes: vec![], edges: vec![], extra: String::new() });
    for (on, key) in [(rediscover, "rediscover"), (welcome, "welcome")] {
        if on {
            if let Ok(x) = fresh(&json!({ "nodes": [data()["extras"][key].clone()], "edges": [] })) {
                b.nodes.extend(x.nodes);
            }
        }
    }
    b
}

/// A board (canvas JSON) with new ids for its cards, and edges that follow them.
fn fresh(board: &Value) -> Result<Board> {
    let mut b = parse_canvas(&board.to_string())?;
    let mut ids = std::collections::HashMap::new();
    b.nodes.retain(|n| n.x.is_finite() && n.y.is_finite());
    for n in &mut b.nodes {
        let id = new_id();
        ids.insert(std::mem::replace(&mut n.id, id.clone()), id);
        if n.width <= 0.0 {
            n.width = 240.0;
        }
        if n.height <= 0.0 {
            n.height = 160.0;
        }
    }
    b.edges.retain(|e| ids.contains_key(&e.from_node) && ids.contains_key(&e.to_node));
    for e in &mut b.edges {
        e.id = new_id();
        e.from_node = ids[&e.from_node].clone();
        e.to_node = ids[&e.to_node].clone();
    }
    Ok(b)
}

/// A board as a template file (`.marrowhome`). Sticky notes keep their words
/// only if `words`.
#[uniffi::export]
pub fn home_template_write(board: Board, name: String, words: bool, today: String) -> String {
    let mut b = board;
    if !words {
        for n in &mut b.nodes {
            if widget_of(&n.extra) == "sticky" {
                n.text.clear();
            }
        }
    }
    let inner: Value = serde_json::from_str(&canvas_json(&b)).unwrap_or(Value::Null);
    let mut o = Map::new();
    o.insert("marrowHome".into(), 1.into());
    o.insert("name".into(), name.into());
    o.insert("made".into(), today.into());
    o.insert("board".into(), inner);
    serde_json::to_string_pretty(&Value::Object(o)).unwrap_or_default()
}

/// A template file (or a plain canvas) read back, with new card ids.
#[uniffi::export]
pub fn home_template_read(text: String) -> Result<HomeTemplateFile> {
    let bad = || MarrowError::Failed { reason: "That file isn't a Marrow Home template.".into() };
    let v: Value = serde_json::from_str(&text).map_err(|_| bad())?;
    let (name, board) = if v.get("marrowHome").is_some() { (v["name"].as_str().unwrap_or_default().to_string(), v["board"].clone()) } else { (String::new(), v) };
    if !board.get("nodes").is_some_and(Value::is_array) {
        return Err(bad());
    }
    Ok(HomeTemplateFile { name, board: fresh(&board)? })
}

/// The `widget` a card's extra fields name ("" for an ordinary card).
#[uniffi::export]
pub fn widget_of(extra: &str) -> String {
    serde_json::from_str::<Value>(extra).ok().and_then(|v| v.get("widget").and_then(Value::as_str).map(String::from)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_starter() {
        let t = home_templates();
        assert_eq!(t[0].name, "Starter");
        assert!(t.len() >= 5);
        let a = home_starter_board(true, true);
        let b = home_starter_board(false, false);
        assert_eq!(a.nodes.len(), b.nodes.len() + 2);
        assert_ne!(a.nodes[0].id, b.nodes[0].id, "every board gets its own ids");
        assert!(a.nodes.iter().any(|n| widget_of(&n.extra) == "tasks"));
        assert!(a.nodes.iter().any(|n| widget_of(&n.extra) == "welcome"));
        assert!(home_template_board(99).is_none());
        // The week planner keeps its board settings.
        let w = home_template_board(1).unwrap();
        assert!(w.extra.contains("grid"));
    }

    #[test]
    fn template_files() {
        let mut board = home_template_board(1).unwrap();
        let sticky = board.nodes.iter().position(|n| widget_of(&n.extra) == "sticky").unwrap();
        board.nodes[sticky].text = "Private".into();
        let bare = home_template_write(board.clone(), "Mine".into(), false, "2026-11-01".into());
        assert!(!bare.contains("Private") && bare.contains("\"marrowHome\": 1"));
        let back = home_template_read(home_template_write(board.clone(), "Mine".into(), true, "2026-11-01".into())).unwrap();
        assert_eq!(back.name, "Mine");
        assert_eq!(back.board.nodes.len(), board.nodes.len());
        assert_eq!(back.board.nodes[sticky].text, "Private");
        assert_ne!(back.board.nodes[0].id, board.nodes[0].id);
        assert!(home_template_read("{\"hello\": 1}".into()).is_err());
        assert!(home_template_read("nope".into()).is_err());
    }
}
