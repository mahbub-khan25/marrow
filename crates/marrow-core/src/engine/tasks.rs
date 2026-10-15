//! Every to-do in the vault (`- [ ] …` list items), for the Tasks view.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::Engine;
use crate::path::VaultPath;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub path: VaultPath,
    /// Note name without `.md`.
    pub name: String,
    /// Zero-based line number.
    pub line: u32,
    pub done: bool,
    /// The task's text, without the `- [ ]` and without the due date.
    pub text: String,
    /// `YYYY-MM-DD`, from `📅 2026-10-20`, `due: 2026-10-20`, `[[2026-10-20]]`
    /// or (for a task in a daily note without one) the note's own date.
    pub due: Option<String>,
    /// Whether `due` came from the daily note's name rather than the task.
    pub from_note: bool,
}

impl Engine {
    /// All tasks in the vault, sorted by note then line.
    pub fn tasks(&mut self, include_done: bool) -> Vec<TaskItem> {
        self.refresh_texts();
        let mut out: Vec<TaskItem> = self
            .texts
            .entries
            .par_iter()
            .flat_map_iter(|(p, c)| {
                let name = p.stem().to_string();
                let note_date = if is_date(&name) { Some(name.clone()) } else { None };
                scan(&c.text)
                    .into_iter()
                    .filter(move |t| include_done || !t.1)
                    .map(move |(line, done, text, due)| {
                        let from_note = due.is_none() && note_date.is_some();
                        TaskItem {
                            path: p.clone(),
                            name: name.clone(),
                            line,
                            done,
                            text,
                            due: due.or_else(|| note_date.clone()),
                            from_note,
                        }
                    })
            })
            .collect();
        out.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()).then(a.line.cmp(&b.line)));
        out
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Finds a due date in the task text and returns it with the text cleaned.
fn take_due(text: &str) -> (String, Option<String>) {
    let markers: [&str; 4] = ["📅", "due:", "due::", "@due("];
    for m in markers {
        if let Some(i) = text.find(m) {
            let rest = text[i + m.len()..].trim_start();
            let date: String = rest.chars().take(10).collect();
            if is_date(&date) {
                let mut end = i + m.len() + (text[i + m.len()..].len() - rest.len()) + 10;
                if m == "@due(" && text[end..].starts_with(')') {
                    end += 1;
                }
                let cleaned = format!("{}{}", &text[..i], &text[end..]);
                return (cleaned.split_whitespace().collect::<Vec<_>>().join(" "), Some(date));
            }
        }
    }
    // A link to a daily note: [[2026-10-20]]
    let mut from = 0;
    while let Some(i) = text[from..].find("[[") {
        let s = from + i + 2;
        if let Some(j) = text[s..].find("]]") {
            let inner = &text[s..s + j];
            let inner = inner.rsplit('/').next().unwrap_or(inner);
            if is_date(inner) {
                return (text.trim().to_string(), Some(inner.to_string()));
            }
            from = s + j + 2;
        } else {
            break;
        }
    }
    (text.trim().to_string(), None)
}

/// (line, done, text, due) for every task line outside code and frontmatter.
fn scan(text: &str) -> Vec<(u32, bool, String, Option<String>)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    let mut in_front = false;
    for (n, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if n == 0 && line.trim() == "---" {
            in_front = true;
            continue;
        }
        if in_front {
            if line.trim() == "---" || line.trim() == "..." {
                in_front = false;
            }
            continue;
        }
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if t.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        // Inside a quote or callout too: "> - [ ] x"
        let t = t.trim_start_matches(['>', ' ']);
        let rest = if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            r
        } else {
            let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits == 0 {
                continue;
            }
            match t[digits..].strip_prefix(". ").or_else(|| t[digits..].strip_prefix(") ")) {
                Some(r) => r,
                None => continue,
            }
        };
        let done = if rest.starts_with("[ ]") {
            false
        } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
            true
        } else {
            continue;
        };
        let body = rest[3..].trim();
        if body.is_empty() {
            continue;
        }
        let (text, due) = take_due(body);
        out.push((n as u32, done, text, due));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{scan, take_due};

    #[test]
    fn finds_tasks_and_dates() {
        let text = "---\ntodo: - [ ] not this\n---\n- [ ] Write intro 📅 2026-10-20\n  - [x] Done thing\n* [ ] Call Ana due: 2026-11-01 soon\n1. [ ] numbered\n```\n- [ ] in code\n```\n> - [ ] in a callout\n- [ ]\n- not a task [ ]\nSee [[2026-10-18]]\n- [ ] Review [[2026-10-18]]";
        let t = scan(text);
        let texts: Vec<&str> = t.iter().map(|x| x.2.as_str()).collect();
        assert_eq!(texts, vec!["Write intro", "Done thing", "Call Ana soon", "numbered", "in a callout", "Review [[2026-10-18]]"]);
        assert_eq!(t[0].3.as_deref(), Some("2026-10-20"));
        assert!(t[1].1);
        assert_eq!(t[2].3.as_deref(), Some("2026-11-01"));
        assert_eq!(t[3].3, None);
        assert_eq!(t[5].3.as_deref(), Some("2026-10-18"));
        assert_eq!(t[0].0, 3);
    }

    #[test]
    fn due_markers() {
        assert_eq!(take_due("Pay rent @due(2026-12-01) now"), ("Pay rent now".to_string(), Some("2026-12-01".to_string())));
        assert_eq!(take_due("No date here"), ("No date here".to_string(), None));
    }
}
