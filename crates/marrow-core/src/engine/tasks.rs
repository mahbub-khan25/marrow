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
    /// When to remind, `YYYY-MM-DD HH:MM` in local time, from
    /// `⏰ 2026-10-30 09:00` (or `⏰ 09:00` on the task's due date).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remind: Option<String>,
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
                    .map(move |(line, done, text, due, remind)| {
                        let from_note = due.is_none() && note_date.is_some();
                        let due = due.or_else(|| note_date.clone());
                        // A time alone is on the task's date.
                        let remind = remind.and_then(|(d, t)| d.or_else(|| due.clone()).map(|d| format!("{d} {t}")));
                        TaskItem {
                            path: p.clone(),
                            name: name.clone(),
                            line,
                            done,
                            text,
                            due,
                            from_note,
                            remind,
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

/// The reminder marker.
pub const REMIND: &str = "⏰";

fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5 && b[2] == b':' && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit) && s[..2] <= *"23" && s[3..] <= *"59"
}

/// Finds a reminder (`⏰ 2026-10-30 09:00`, `⏰ 2026-10-30T09:00` or
/// `⏰ 09:00`) and returns the text without it, and the (date, time).
pub(crate) fn take_remind(text: &str) -> (String, Option<(Option<String>, String)>) {
    let Some(i) = text.find(REMIND) else { return (text.to_string(), None) };
    let after = &text[i + REMIND.len()..];
    let rest = after.trim_start();
    let skipped = after.len() - rest.len();
    let (date, time, used) = if rest.len() >= 16 && is_date(&rest[..10]) && matches!(rest.as_bytes()[10], b' ' | b'T') && is_time(&rest[11..16]) {
        (Some(rest[..10].to_string()), rest[11..16].to_string(), 16)
    } else if rest.len() >= 5 && is_time(&rest[..5]) {
        (None, rest[..5].to_string(), 5)
    } else {
        return (text.to_string(), None);
    };
    let end = i + REMIND.len() + skipped + used;
    let cleaned = format!("{}{}", &text[..i], &text[end..]);
    (cleaned.split_whitespace().collect::<Vec<_>>().join(" "), Some((date, time)))
}

/// A task line with its reminder set to `at` (`YYYY-MM-DD HH:MM`), or
/// removed with `None`. Other text stays as it was.
pub fn set_remind(line: &str, at: Option<&str>) -> String {
    let mut out = line.to_string();
    if let Some(i) = out.find(REMIND) {
        let after = &out[i + REMIND.len()..];
        let rest = after.trim_start();
        let skipped = after.len() - rest.len();
        let used = if rest.len() >= 16 && is_date(&rest[..10]) && matches!(rest.as_bytes()[10], b' ' | b'T') && is_time(&rest[11..16]) {
            16
        } else if rest.len() >= 5 && is_time(&rest[..5]) {
            5
        } else {
            0
        };
        let end = i + REMIND.len() + skipped + used;
        // Take one space before the marker with it.
        let start = if out[..i].ends_with(' ') { i - 1 } else { i };
        out = format!("{}{}", &out[..start], &out[end..]);
    }
    match at {
        Some(at) => format!("{} {REMIND} {at}", out.trim_end()),
        None => out,
    }
}

/// The reminder on one task line, `YYYY-MM-DD HH:MM` (a time alone counts
/// on the line's due date).
pub fn remind_of(line: &str) -> Option<String> {
    let t = scan(line).into_iter().next()?;
    let (d, time) = t.4?;
    Some(format!("{} {time}", d.or(t.3)?))
}

/// A task line's text as the Tasks view shows it (no box, dates or
/// reminder), to recognise the task after lines move.
pub fn task_text_of(line: &str) -> Option<String> {
    scan(line).into_iter().next().map(|t| t.2)
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

type Remind = Option<(Option<String>, String)>;

/// (line, done, text, due, reminder) for every task line outside code and frontmatter.
fn scan(text: &str) -> Vec<(u32, bool, String, Option<String>, Remind)> {
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
        let (body, remind) = take_remind(body);
        let (text, due) = take_due(&body);
        out.push((n as u32, done, text, due, remind));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{scan, take_due, take_remind};

    #[test]
    fn setting_reminders() {
        use super::{set_remind, task_text_of};
        let l = "  - [ ] Call the dentist 📅 2026-11-03";
        let a = set_remind(l, Some("2026-11-03 09:00"));
        assert_eq!(a, "  - [ ] Call the dentist 📅 2026-11-03 ⏰ 2026-11-03 09:00");
        assert_eq!(set_remind(&a, Some("2026-11-03 09:10")), "  - [ ] Call the dentist 📅 2026-11-03 ⏰ 2026-11-03 09:10");
        assert_eq!(set_remind(&a, None), l);
        assert_eq!(set_remind("- [ ] A ⏰ 08:00 later", None), "- [ ] A later");
        assert_eq!(task_text_of(&a).as_deref(), Some("Call the dentist"));
    }

    #[test]
    fn reminders() {
        let t = scan("- [ ] Call the dentist ⏰ 2026-11-03 09:00\n- [ ] Pay rent 📅 2026-12-01 ⏰ 08:30\n- [ ] Bad ⏰ 25:00\n- [x] Old ⏰ 2026-01-01T07:05");
        assert_eq!(t[0].2, "Call the dentist");
        assert_eq!(t[0].4, Some((Some("2026-11-03".into()), "09:00".into())));
        assert_eq!(t[1].2, "Pay rent");
        assert_eq!(t[1].3.as_deref(), Some("2026-12-01"));
        assert_eq!(t[1].4, Some((None, "08:30".into())));
        assert_eq!(t[2].4, None);
        assert_eq!(t[3].4, Some((Some("2026-01-01".into()), "07:05".into())));
        assert_eq!(take_remind("x").1, None);
    }

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
