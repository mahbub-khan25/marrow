//! Bringing notes in from other apps: Notion (Markdown & CSV export),
//! Evernote (.enex), Word and OneNote (.docx), and web pages (.html).
//!
//! Each importer turns its input into a list of files (notes and
//! attachments) with vault paths under a destination folder; the app writes
//! them. Nothing here touches the vault.

pub mod docx;
pub mod enex;
pub mod html;
pub mod notion;

use std::collections::HashSet;

use serde::Serialize;

/// One file to write.
#[derive(Debug, Clone)]
pub struct OutFile {
    /// Vault path, '/'-separated.
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub notes: usize,
    pub attachments: usize,
    /// Things that didn't come across, in plain words.
    pub warnings: Vec<String>,
}

/// Files being imported, with unique names.
#[derive(Default)]
pub struct Output {
    pub files: Vec<OutFile>,
    pub summary: Summary,
    used: HashSet<String>,
}

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif"];

pub fn is_image(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| IMAGE_EXT.contains(&e.to_lowercase().as_str()))
}

/// A file or folder name that works on every system.
pub fn clean_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if r#"\/:*?"<>|#^[]"#.contains(c) || c.is_control() { ' ' } else { c })
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    let out = out.trim_start_matches('.').trim().to_string();
    if out.is_empty() {
        "Untitled".into()
    } else if out.chars().count() > 120 {
        out.chars().take(120).collect::<String>().trim().to_string()
    } else {
        out
    }
}

impl Output {
    /// Reserves a free path for `folder/name.ext` ("name 2.ext" if taken).
    pub fn reserve(&mut self, folder: &str, name: &str, ext: &str) -> String {
        let base = clean_name(name);
        for n in 1.. {
            let file = match (n, ext.is_empty()) {
                (1, true) => base.clone(),
                (1, false) => format!("{base}.{ext}"),
                (n, true) => format!("{base} {n}"),
                (n, false) => format!("{base} {n}.{ext}"),
            };
            let path = if folder.is_empty() { file } else { format!("{folder}/{file}") };
            if self.used.insert(path.to_lowercase()) {
                return path;
            }
        }
        unreachable!()
    }

    pub fn note(&mut self, path: String, text: String) {
        self.summary.notes += 1;
        self.files.push(OutFile { path, bytes: text.into_bytes() });
    }

    pub fn attachment(&mut self, path: String, bytes: Vec<u8>) {
        self.summary.attachments += 1;
        self.files.push(OutFile { path, bytes });
    }

    pub fn warn(&mut self, w: impl Into<String>) {
        if self.summary.warnings.len() < 50 {
            self.summary.warnings.push(w.into());
        }
    }
}

/// YAML frontmatter for simple key/values (strings, lists).
pub fn frontmatter(props: &[(String, PropValue)]) -> String {
    if props.is_empty() {
        return String::new();
    }
    let mut map = serde_yaml_ng::Mapping::new();
    for (k, v) in props {
        let val = match v {
            PropValue::Text(t) => serde_yaml_ng::Value::String(t.clone()),
            PropValue::List(l) => serde_yaml_ng::Value::Sequence(l.iter().map(|x| serde_yaml_ng::Value::String(x.clone())).collect()),
            PropValue::Bool(b) => serde_yaml_ng::Value::Bool(*b),
            PropValue::Number(n) => serde_yaml_ng::to_value(n).unwrap_or(serde_yaml_ng::Value::Null),
        };
        map.insert(serde_yaml_ng::Value::String(k.clone()), val);
    }
    let yaml = serde_yaml_ng::to_string(&map).unwrap_or_default();
    format!("---\n{}---\n", yaml)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Text(String),
    List(Vec<String>),
    Bool(bool),
    Number(f64),
}

/// A value as Notion or a CSV writes it, typed when it's clearly a number,
/// a yes/no or a list.
pub fn guess_value(s: &str) -> PropValue {
    let t = s.trim();
    match t {
        "Yes" | "yes" | "true" | "TRUE" => return PropValue::Bool(true),
        "No" | "no" | "false" | "FALSE" => return PropValue::Bool(false),
        _ => {}
    }
    if let Ok(n) = t.parse::<f64>() {
        if t.len() < 16 && !t.starts_with('0') || t == "0" || t.starts_with("0.") {
            return PropValue::Number(n);
        }
    }
    if let Some(d) = notion_date(t) {
        return PropValue::Text(d);
    }
    PropValue::Text(t.to_string())
}

/// "October 15, 2026" or "October 15, 2026 3:05 PM" → "2026-10-15".
pub fn notion_date(s: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let mut parts = s.split_whitespace();
    let m = parts.next()?;
    let month = MONTHS.iter().position(|x| *x == m)? + 1;
    let day: u32 = parts.next()?.trim_end_matches(',').parse().ok()?;
    let year: u32 = parts.next()?.parse().ok()?;
    if !(1..=31).contains(&day) || !(1000..=9999).contains(&year) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// HTML to Markdown, tidied: no runs of blank lines, no trailing spaces.
pub fn html_to_md(html: &str) -> String {
    let md = htmd::convert(html).unwrap_or_default();
    tidy(&md)
}

/// "*   item" → "- item", "1.  item" → "1. item".
fn list_marker(line: &str) -> String {
    let t = line.trim_start();
    let indent = &line[..line.len() - t.len()];
    if let Some(rest) = t.strip_prefix("*   ").or_else(|| t.strip_prefix("+   ")).or_else(|| t.strip_prefix("-   ")) {
        return format!("{indent}- {rest}");
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && t[digits..].starts_with(".  ") {
        return format!("{indent}{}. {}", &t[..digits], t[digits + 1..].trim_start());
    }
    line.to_string()
}

fn is_item(l: &str) -> bool {
    let t = l.trim_start();
    t.starts_with("- ") || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

pub fn tidy(md: &str) -> String {
    let lines: Vec<String> = md.lines().map(list_marker).collect();
    let mut out = String::new();
    let mut blank = 0;
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim_end();
        // No blank line between two items of the same list.
        if l.is_empty() && i > 0 && is_item(&lines[i - 1]) && lines.get(i + 1).is_some_and(|n| is_item(n)) {
            continue;
        }
        if l.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(l);
        out.push('\n');
    }
    out.trim().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_values() {
        assert_eq!(clean_name("  a/b: c?. "), "a b c");
        assert_eq!(clean_name("..."), "Untitled");
        let mut o = Output::default();
        assert_eq!(o.reserve("X", "Note", "md"), "X/Note.md");
        assert_eq!(o.reserve("X", "note", "md"), "X/note 2.md");
        assert_eq!(guess_value("12"), PropValue::Number(12.0));
        assert_eq!(guess_value("007"), PropValue::Text("007".into()));
        assert_eq!(guess_value("Yes"), PropValue::Bool(true));
        assert_eq!(guess_value("October 5, 2026 3:05 PM"), PropValue::Text("2026-10-05".into()));
        let fm = frontmatter(&[("tags".into(), PropValue::List(vec!["a".into()])), ("done".into(), PropValue::Bool(true))]);
        assert!(fm.starts_with("---\n") && fm.contains("tags:\n- a") && fm.contains("done: true"));
    }

    #[test]
    fn html_conversion() {
        let md = html_to_md("<h1>Title</h1><p>Some <b>bold</b> and <i>it</i>.</p><ul><li>one</li><li>two</li></ul><p><a href=\"https://x.org\">link</a></p>");
        assert!(md.contains("# Title"));
        assert!(md.contains("**bold**"));
        assert!(md.contains("one") && md.contains("two"));
        assert!(md.contains("[link](https://x.org)"));
    }
}
