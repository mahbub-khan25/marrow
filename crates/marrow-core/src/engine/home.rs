//! What the Home board's cards ask for: tasks picked by date range, tag,
//! folder, note or words, and one section of a note.

use serde::{Deserialize, Serialize};

use super::{Engine, TaskItem};

/// Which tasks a task card shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskFilter {
    /// `all`, `today`, `days` (today and the next `days - 1` days) or
    /// `range` (`from` to `to`, either may be empty), or `none` (only tasks
    /// with no date). Empty means `all`.
    pub range: String,
    pub days: u32,
    pub from: String,
    pub to: String,
    /// Also tasks due before the range that aren't done.
    pub overdue: bool,
    /// Also tasks with no date.
    pub undated: bool,
    /// Also tasks that are done.
    pub done: bool,
    /// Only tasks tagged so (on the task or its note), with or without `#`.
    pub tag: String,
    /// Only tasks in this folder (and the folders inside it).
    pub folder: String,
    /// Only tasks in this note (its path).
    pub note: String,
    /// Only tasks whose words contain this.
    pub text: String,
}

impl Engine {
    /// The tasks a card with filter `f` shows, on day `today` (`YYYY-MM-DD`):
    /// overdue first, then by date, then those with no date.
    pub fn task_view(&mut self, today: &str, f: &TaskFilter) -> Vec<TaskItem> {
        let (start, end) = window(today, f);
        let tag = f.tag.trim().trim_start_matches('#').to_lowercase();
        let folder = f.folder.trim().trim_matches('/').to_string();
        let note = f.note.trim().to_string();
        let words = f.text.trim().to_lowercase();
        let mut out: Vec<TaskItem> = self
            .tasks(f.done)
            .into_iter()
            .filter(|t| folder.is_empty() || t.path.as_str().starts_with(&format!("{folder}/")))
            .filter(|t| note.is_empty() || t.path.as_str() == note || t.name == note)
            .filter(|t| words.is_empty() || t.text.to_lowercase().contains(&words))
            .filter(|t| tag.is_empty() || has_tag(&t.text, &tag) || self.note_has_tag(t, &tag))
            .filter(|t| match &t.due {
                None => f.undated || f.range == "none",
                Some(_) if f.range == "none" => false,
                Some(d) => {
                    let before = start.as_deref().is_some_and(|s| d.as_str() < s);
                    let after = end.as_deref().is_some_and(|e| d.as_str() > e);
                    (!before && !after) || (before && f.overdue && !t.done)
                }
            })
            .collect();
        out.sort_by(|a, b| match (&a.due, &b.due) {
            (Some(x), Some(y)) => x.cmp(y).then(a.path.as_str().cmp(b.path.as_str())).then(a.line.cmp(&b.line)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.path.as_str().cmp(b.path.as_str()).then(a.line.cmp(&b.line)),
        });
        out
    }

    fn note_has_tag(&self, t: &TaskItem, tag: &str) -> bool {
        self.meta(&t.path).is_some_and(|m| {
            m.all_tags().iter().any(|x| {
                let x = x.trim_start_matches('#').to_lowercase();
                x == tag || x.starts_with(&format!("{tag}/"))
            })
        })
    }
}

/// The first and last day a filter covers (inclusive; None = open).
fn window(today: &str, f: &TaskFilter) -> (Option<String>, Option<String>) {
    match f.range.as_str() {
        "today" => (Some(today.to_string()), Some(today.to_string())),
        "days" => {
            let n = f.days.max(1) as i64;
            (Some(today.to_string()), add_days(today, n - 1))
        }
        "range" => (non_empty(&f.from), non_empty(&f.to)),
        _ => (None, None),
    }
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// `#tag` (or `#tag/sub`) written in a task's words.
fn has_tag(text: &str, tag: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').any(|w| {
        w.strip_prefix('#').is_some_and(|w| {
            let w = w.trim_end_matches(['.', ')', '!', '?', ':']).to_lowercase();
            w == tag || w.starts_with(&format!("{tag}/"))
        })
    })
}

/// `YYYY-MM-DD` plus `n` days.
pub fn add_days(day: &str, n: i64) -> Option<String> {
    let mut p = day.split('-');
    let (y, m, d) = (p.next()?.parse::<i64>().ok()?, p.next()?.parse::<i64>().ok()?, p.next()?.parse::<i64>().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (y, m, d) = from_days(to_days(y, m, d) + n);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

// Days since 1970-01-01 and back (Howard Hinnant's civil calendar).
fn to_days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The part of a note under the heading `heading` (matched without case),
/// down to the next heading at the same level or above; the heading itself
/// is left out. None if there's no such heading.
pub fn note_section(text: &str, heading: &str) -> Option<String> {
    let want = heading.trim().trim_start_matches('#').trim().to_lowercase();
    let level = |l: &str| {
        let n = l.chars().take_while(|c| *c == '#').count();
        (n > 0 && n <= 6 && l[n..].starts_with(' ')).then_some(n)
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut fence = false;
    let mut start = None;
    for (i, l) in lines.iter().enumerate() {
        if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        if let Some(n) = level(l) {
            match start {
                None if l[n..].trim().to_lowercase() == want => start = Some((i + 1, n)),
                Some((from, at)) if n <= at => return Some(lines[from..i].join("\n").trim().to_string()),
                _ => {}
            }
        }
    }
    start.map(|(from, _)| lines[from..].join("\n").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_views() {
        use crate::cache::Cache;
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Work")).unwrap();
        std::fs::write(
            d.path().join("Work/Plan.md"),
            "#work\n- [ ] Old thing 📅 2026-10-20\n- [ ] Today thing 📅 2026-11-01\n- [ ] Next week 📅 2026-11-07\n- [ ] Far 📅 2026-11-08\n- [x] Done today 📅 2026-11-01\n- [ ] No date\n",
        )
        .unwrap();
        std::fs::write(d.path().join("Home.md"), "- [ ] Buy milk #shop 📅 2026-11-02\n- [ ] Undated home\n").unwrap();
        let mut e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        let names = |f: TaskFilter, e: &mut Engine| e.task_view("2026-11-01", &f).into_iter().map(|t| t.text).collect::<Vec<_>>();
        let week = TaskFilter { range: "days".into(), days: 7, ..Default::default() };
        assert_eq!(names(week.clone(), &mut e), vec!["Today thing", "Buy milk #shop", "Next week"]);
        assert_eq!(names(TaskFilter { overdue: true, ..week.clone() }, &mut e), vec!["Old thing", "Today thing", "Buy milk #shop", "Next week"]);
        assert_eq!(names(TaskFilter { range: "today".into(), done: true, ..Default::default() }, &mut e), vec!["Today thing", "Done today"]);
        assert_eq!(names(TaskFilter { tag: "#work".into(), range: "today".into(), ..Default::default() }, &mut e), vec!["Today thing"]);
        assert_eq!(names(TaskFilter { tag: "shop".into(), ..Default::default() }, &mut e), vec!["Buy milk #shop"]);
        assert_eq!(names(TaskFilter { folder: "Work".into(), undated: true, range: "range".into(), from: "2026-11-08".into(), ..Default::default() }, &mut e), vec!["Far", "No date"]);
        assert_eq!(names(TaskFilter { note: "Home.md".into(), undated: true, ..Default::default() }, &mut e), vec!["Buy milk #shop", "Undated home"]);
        assert_eq!(names(TaskFilter { text: "MILK".into(), ..Default::default() }, &mut e), vec!["Buy milk #shop"]);
        assert_eq!(names(TaskFilter { range: "none".into(), ..Default::default() }, &mut e), vec!["Undated home", "No date"]);
    }

    #[test]
    fn days() {
        assert_eq!(add_days("2026-10-31", 1).as_deref(), Some("2026-11-01"));
        assert_eq!(add_days("2026-12-28", 6).as_deref(), Some("2027-01-03"));
        assert_eq!(add_days("2028-02-28", 1).as_deref(), Some("2028-02-29"));
        assert_eq!(add_days("2026-03-01", -1).as_deref(), Some("2026-02-28"));
        assert_eq!(add_days("nope", 1), None);
    }

    #[test]
    fn tags_in_words() {
        assert!(has_tag("Call Sam #work", "work"));
        assert!(has_tag("Plan #work/q4.", "work"));
        assert!(!has_tag("Plan #workshop", "work"));
    }

    #[test]
    fn sections() {
        let t = "# Plan\nintro\n## This month\n- a\n### Detail\nx\n## Next\n- b\n```\n## not a heading\n```\n";
        assert_eq!(note_section(t, "This month").as_deref(), Some("- a\n### Detail\nx"));
        assert_eq!(note_section(t, "## next").as_deref(), Some("- b\n```\n## not a heading\n```"));
        assert_eq!(note_section(t, "Missing"), None);
    }
}
