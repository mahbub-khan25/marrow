//! Daily notes, the calendar, quick capture, templates and resurfacing.
//! Dates are `YYYY-MM-DD` strings in the person's own time zone: the apps
//! pass "today", so nothing here needs to know about time zones.

use std::collections::BTreeMap;

use crate::files::{display_name, preview_of};
use crate::settings::read_vault_settings;
use crate::{fail, Marrow, Result};

/// Folder names are fixed English, so they never change with the language.
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// What a new daily note starts with (the desktop app's too).
const DAY_TEMPLATE: &str = "## Log\n- \n\n## Tasks\n- [ ] \n";

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn parse_day(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i32 = s[..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..].parse().ok()?;
    ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then_some((y, m, d))
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// True for a real calendar date written `YYYY-MM-DD`.
pub fn is_day(s: &str) -> bool {
    parse_day(s).is_some()
}

fn clean(folder: &str) -> String {
    folder.trim_matches('/').to_string()
}

/// Where a day's note belongs.
pub(crate) fn day_path(folder: &str, layout: &str, day: &str) -> String {
    let base = clean(folder);
    let mut parts: Vec<String> = vec![];
    if !base.is_empty() {
        parts.push(base);
    }
    if layout == "year-month" {
        let m: usize = day[5..7].parse().unwrap_or(1);
        parts.push(day[..4].to_string());
        parts.push(format!("{:02} {}", m, MONTHS[m.clamp(1, 12) - 1]));
    }
    parts.push(format!("{day}.md"));
    parts.join("/")
}

/// The day of a daily note: a note named by date anywhere in the daily folder.
pub(crate) fn day_of(path: &str, folder: &str) -> Option<String> {
    if !path.to_ascii_lowercase().ends_with(".md") {
        return None;
    }
    let base = clean(folder);
    if !base.is_empty() && !path.starts_with(&format!("{base}/")) {
        return None;
    }
    let stem = display_name(path);
    is_day(&stem).then_some(stem)
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DayNote {
    pub day: String,
    pub path: String,
}

/// A card on the home screen that brings back older notes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Resurfaced {
    /// A note not opened or changed in a long time, if any.
    pub forgotten: Option<Forgotten>,
    /// How many such notes there are ("Another" cycles through them).
    pub forgotten_count: u32,
    /// Daily notes from this date in earlier years, newest first.
    pub on_this_day: Vec<OnThisDay>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Forgotten {
    pub path: String,
    pub title: String,
    /// When it was last opened or changed (ms).
    pub last_ms: i64,
    pub preview: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct OnThisDay {
    pub day: String,
    pub path: String,
    pub years_ago: u32,
    pub preview: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Template {
    pub name: String,
    pub path: String,
}

/// A small, stable hash (FNV-1a over UTF-16, as the desktop app does), so
/// the same note is picked all day on every device.
fn hash(s: &str) -> u32 {
    let mut x: u32 = 2166136261;
    for u in s.encode_utf16() {
        x = (x ^ u as u32).wrapping_mul(16777619);
    }
    x
}

/// Adds `line` under `## <section>` (filling an empty placeholder item), or at the end.
pub(crate) fn add_under(text: &str, section: &str, line: &str) -> String {
    let trimmed = text.trim_end();
    let mut lines: Vec<String> = if trimmed.is_empty() { vec![] } else { trimmed.split('\n').map(str::to_string).collect() };
    let is_head = |l: &str| {
        let t = l.trim_start_matches('#');
        l.starts_with('#') && l.len() - t.len() <= 6 && t.starts_with(' ')
    };
    let head = lines.iter().position(|l| is_head(l) && l.trim_start_matches('#').trim().eq_ignore_ascii_case(section));
    match head {
        None => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(line.to_string());
        }
        Some(h) => {
            let mut end = h + 1;
            while end < lines.len() && !is_head(&lines[end]) {
                end += 1;
            }
            let empty = (h + 1..end).find(|&i| {
                let t = lines[i].trim();
                matches!(t, "-" | "*" | "+" | "- [ ]" | "* [ ]" | "+ [ ]")
            });
            match empty {
                Some(i) => lines[i] = line.to_string(),
                None => {
                    let mut at = end;
                    while at > h + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, line.to_string());
                }
            }
        }
    }
    lines.join("\n") + "\n"
}

impl Marrow {
    /// Daily notes by day (the first one wins a duplicate day).
    pub(crate) fn daily_index(&self) -> Result<BTreeMap<String, String>> {
        let root = self.root()?;
        let folder = read_vault_settings(&root).daily_folder;
        self.with(|e| {
            let mut found: Vec<(String, String)> = e.vault().markdown_files().filter_map(|p| day_of(p.as_str(), &folder).map(|d| (d, p.to_string()))).collect();
            found.sort();
            let mut map = BTreeMap::new();
            for (d, p) in found {
                map.entry(d).or_insert(p);
            }
            Ok(map)
        })
    }

    pub(crate) fn seen_moved(&self, root: &std::path::Path, from: &str, to: &str) {
        let key = root.to_string_lossy().into_owned();
        let _ = self.update_settings(|s| {
            if let Some(seen) = s.seen.get_mut(&key) {
                let moved: Vec<(String, i64)> = seen
                    .iter()
                    .filter(|(p, _)| *p == from || p.starts_with(&format!("{from}/")))
                    .map(|(p, t)| (format!("{to}{}", &p[from.len()..]), *t))
                    .collect();
                seen.retain(|p, _| !(p == from || p.starts_with(&format!("{from}/"))));
                seen.extend(moved);
            }
        });
    }
}

#[uniffi::export]
impl Marrow {
    /// The note for `day`, if there is one.
    pub fn daily_note(&self, day: String) -> Result<Option<String>> {
        Ok(self.daily_index()?.get(&day).cloned())
    }

    /// The note for `day`, made (with the usual Log and Tasks headings) if needed.
    pub fn open_day(&self, day: String) -> Result<String> {
        if !is_day(&day) {
            return Err(fail(format!("“{day}” isn't a date.")));
        }
        if let Some(p) = self.daily_index()?.get(&day) {
            return Ok(p.clone());
        }
        let root = self.root()?;
        let s = read_vault_settings(&root);
        let path = day_path(&s.daily_folder, &s.daily_layout, &day);
        let (folder, _) = path.rsplit_once('/').unwrap_or(("", ""));
        self.create_file(folder.to_string(), day, "md".into(), DAY_TEMPLATE.into())
    }

    /// Days in a month (`YYYY-MM`) that have a daily note, as day numbers.
    pub fn days_with_notes(&self, month: String) -> Result<Vec<u32>> {
        let prefix = format!("{}-", month.trim());
        Ok(self.daily_index()?.keys().filter(|d| d.starts_with(&prefix)).filter_map(|d| d[8..].parse().ok()).collect())
    }

    /// Every daily note, newest first.
    pub fn daily_notes(&self, limit: u32) -> Result<Vec<DayNote>> {
        Ok(self.daily_index()?.into_iter().rev().take(limit as usize).map(|(day, path)| DayNote { day, path }).collect())
    }

    /// Adds a quick note to today's note: a timestamped line under "Log", or
    /// a to-do under "Tasks". `extra` lines (embeds of photos or
    /// recordings) go under it, indented. Returns the daily note's path.
    pub fn capture(&self, today: String, time: String, text: String, as_task: bool, extra: Vec<String>) -> Result<String> {
        let path = self.open_day(today)?;
        let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut line = if as_task { format!("- [ ] {one}") } else { format!("- {time} {one}").trim_end().to_string() };
        for x in extra {
            line.push_str(&format!("\n  {x}"));
        }
        let before = self.read_note(path.clone())?;
        let after = add_under(&before, if as_task { "Tasks" } else { "Log" }, &line);
        self.write_note(path.clone(), after, Some(before))?;
        Ok(path)
    }

    /// Notes in the templates folder.
    pub fn templates(&self) -> Result<Vec<Template>> {
        let root = self.root()?;
        let f = clean(&read_vault_settings(&root).template_folder);
        if f.is_empty() {
            return Ok(vec![]);
        }
        self.with(|e| {
            let mut v: Vec<Template> = e
                .vault()
                .markdown_files()
                .filter(|p| p.as_str().starts_with(&format!("{f}/")))
                .map(|p| Template { name: display_name(p.as_str()), path: p.to_string() })
                .collect();
            v.sort_by(|a, b| crate::files::natural(&a.name, &b.name));
            Ok(v)
        })
    }

    /// A template's text with {{title}}, {{date}} and {{time}} filled in.
    pub fn template_text(&self, path: String, title: String, date: String, time: String) -> Result<String> {
        let t = self.read_note(path)?;
        Ok(t.replace("{{title}}", &title).replace("{{date}}", &date).replace("{{time}}", &time))
    }

    /// Notes that are opened count as fresh, so they aren't resurfaced.
    pub fn mark_seen(&self, path: String, now_ms: i64) -> Result<()> {
        let key = self.root()?.to_string_lossy().into_owned();
        self.update_settings(|s| {
            s.seen.entry(key).or_default().insert(path, now_ms);
        })
    }

    /// Never suggest this note again.
    pub fn skip_resurfacing(&self, path: String) -> Result<()> {
        let key = self.root()?.to_string_lossy().into_owned();
        self.update_settings(|s| {
            let v = s.skip.entry(key).or_default();
            if !v.contains(&path) {
                v.push(path);
            }
        })
    }

    /// Older notes worth another look. `offset` picks the next forgotten note
    /// ("Another"); the pick is the same all day.
    pub fn resurface(&self, today: String, now_ms: i64, offset: u32) -> Result<Resurfaced> {
        let root = self.root()?;
        let key = root.to_string_lossy().into_owned();
        let settings = self.app_settings();
        let seen = settings.seen.get(&key).cloned().unwrap_or_default();
        let skip = settings.skip.get(&key).cloned().unwrap_or_default();
        let vs = read_vault_settings(&root);
        let daily = self.daily_index()?;
        let daily_paths: std::collections::HashSet<&String> = daily.values().collect();
        let excluded = |p: &str| {
            let t = clean(&vs.template_folder);
            daily_paths.contains(&p.to_string()) || (!t.is_empty() && p.starts_with(&format!("{t}/")))
        };
        let rows: Vec<(String, i64)> = self.with(|e| {
            Ok(e.vault()
                .markdown_files()
                .filter(|p| !p.is_hidden())
                .filter_map(|p| e.vault().stat(p).map(|s| (p.to_string(), s.mtime_ms)))
                .collect())
        })?;
        let all: Vec<(String, i64)> = rows
            .into_iter()
            .filter(|(p, _)| !skip.contains(p) && !excluded(p))
            .map(|(p, m)| {
                let last = seen.get(&p).copied().unwrap_or(0).max(m);
                (p, last)
            })
            .collect();
        let mut old: Vec<(String, i64)> = vec![];
        for days in [365, 182, 91] {
            old = all.iter().filter(|(_, l)| now_ms - l >= days * DAY_MS).cloned().collect();
            if !old.is_empty() {
                old.sort_by(|a, b| hash(&format!("{today}\u{0}{}", a.0)).cmp(&hash(&format!("{today}\u{0}{}", b.0))).then_with(|| a.0.cmp(&b.0)));
                break;
            }
        }
        let forgotten = if old.is_empty() {
            None
        } else {
            let (path, last) = old[offset as usize % old.len()].clone();
            let text = self.read_note(path.clone()).unwrap_or_default();
            Some(Forgotten { title: display_name(&path), preview: preview_of(&text, 220), path, last_ms: last })
        };
        let mut on_this_day = vec![];
        if let Some((y, m, d)) = parse_day(&today) {
            let md = format!("{m:02}-{d:02}");
            for (day, path) in daily.iter().rev() {
                let Some((dy, dm, dd)) = parse_day(day) else { continue };
                let same = format!("{dm:02}-{dd:02}") == md || (m == 2 && d == 29 && dm == 2 && dd == 28 && !is_leap(dy)) || (m == 2 && d == 28 && !is_leap(y) && dm == 2 && dd == 29);
                if dy < y && same {
                    let text = self.read_note(path.clone()).unwrap_or_default();
                    on_this_day.push(OnThisDay { day: day.clone(), path: path.clone(), years_ago: (y - dy) as u32, preview: preview_of(&text, 140) });
                    if on_this_day.len() == 3 {
                        break;
                    }
                }
            }
        }
        Ok(Resurfaced { forgotten_count: old.len() as u32, forgotten, on_this_day })
    }
}

/// Where a day's note goes, for a vault's daily-note settings.
#[uniffi::export]
pub fn daily_path_for(folder: String, layout: String, day: String) -> String {
    day_path(&folder, &layout, &day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_paths() {
        assert_eq!(day_path("Daily", "year-month", "2026-10-24"), "Daily/2026/10 October/2026-10-24.md");
        assert_eq!(day_path("/", "flat", "2026-10-24"), "2026-10-24.md");
        assert_eq!(day_of("Daily/2026/10 October/2026-10-24.md", "Daily").as_deref(), Some("2026-10-24"));
        assert_eq!(day_of("Other/2026-10-24.md", "Daily"), None);
        assert!(!is_day("2026-02-30") && is_day("2024-02-29") && !is_day("2025-02-29"));
    }

    #[test]
    fn capture_goes_under_its_heading() {
        let t = add_under(DAY_TEMPLATE, "Log", "- 09:30 coffee");
        // Trailing spaces go, as on the desktop.
        assert_eq!(t, "## Log\n- 09:30 coffee\n\n## Tasks\n- [ ]\n");
        let t = add_under(&t, "Log", "- 10:00 call");
        assert_eq!(t, "## Log\n- 09:30 coffee\n- 10:00 call\n\n## Tasks\n- [ ]\n");
        let t = add_under(&t, "Tasks", "- [ ] buy milk");
        assert!(t.ends_with("## Tasks\n- [ ] buy milk\n"));
        assert_eq!(add_under("", "Log", "- x"), "- x\n");
        assert_eq!(add_under("Hello", "Log", "- x"), "Hello\n\n- x\n");
    }

    #[test]
    fn the_same_pick_as_the_desktop() {
        // FNV-1a of "a" over UTF-16, as JavaScript computes it.
        assert_eq!(hash("a"), 0xe40c292c);
    }

    #[test]
    fn days_capture_and_resurfacing() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("Old idea.md", "An idea from long ago"), ("Daily/2025/10 October/2025-10-26.md", "Last year's day")]);
        let old = dir.path().join("vault/Old idea.md");
        let f = std::fs::File::options().write(true).open(&old).unwrap();
        f.set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000)).unwrap();
        m.rescan().unwrap();
        let p = m.capture("2026-10-26".into(), "09:30".into(), "first  thought".into(), false, vec!["![[a.m4a]]".into()]).unwrap();
        assert_eq!(p, "Daily/2026/10 October/2026-10-26.md");
        assert!(m.read_note(p.clone()).unwrap().starts_with("## Log\n- 09:30 first thought\n  ![[a.m4a]]\n"));
        assert_eq!(m.open_day("2026-10-26".into()).unwrap(), p);
        assert_eq!(m.days_with_notes("2026-10".into()).unwrap(), [26]);
        let now = 1_790_000_000_000; // late 2026
        let r = m.resurface("2026-10-26".into(), now, 0).unwrap();
        assert_eq!(r.forgotten.unwrap().title, "Old idea");
        assert_eq!(r.on_this_day.len(), 1);
        assert_eq!(r.on_this_day[0].years_ago, 1);
        m.mark_seen("Old idea.md".into(), now).unwrap();
        assert!(m.resurface("2026-10-26".into(), now, 0).unwrap().forgotten.is_none());
    }
}
