//! Reading and saving notes, and what's around a note: links, backlinks,
//! tags, outline, tasks and search.

use marrow_core::vault_locks as vl;
use marrow_core::VaultPath;

use crate::files::display_name;
use crate::{fail, vp, Marrow, Result, CHANGED};

/// A new version is kept at most this often while a note is being edited.
const HISTORY_GAP_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, uniffi::Record)]
pub struct Backlink {
    /// The note that links here.
    pub source: String,
    pub title: String,
    /// The lines with the links, trimmed, for context.
    pub lines: Vec<LineRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct LineRef {
    /// Zero-based line number.
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: u32,
}

/// Everything a note's info sheet shows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NoteDetails {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub words: u32,
    pub characters: u32,
    pub outline: Vec<Heading>,
    pub backlinks: Vec<Backlink>,
    /// Notes this one links to.
    pub outgoing: Vec<String>,
    /// Links that point to notes that don't exist yet.
    pub unresolved: Vec<String>,
    /// Other notes that mention this one by name without linking.
    pub mentions: Vec<Mention>,
    pub starred: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Mention {
    pub source: String,
    pub title: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub matches: u32,
    pub lines: Vec<SearchLine>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchLine {
    pub line: u32,
    pub text: String,
    /// Highlighted parts of `text`, in UTF-16 units: start, end, start, end…
    pub ranges: Vec<u32>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Notes that matched, including any beyond the limit.
    pub total: u32,
}

/// A note found by name, for the quick switcher and `[[` suggestions.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FileHit {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub is_note: bool,
    /// What to put inside `[[…]]` when linking from the note being edited.
    pub link_text: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskItem {
    pub path: String,
    pub title: String,
    /// Zero-based line number.
    pub line: u32,
    pub done: bool,
    pub text: String,
    /// `YYYY-MM-DD`, if it has a due date (or sits in a daily note).
    pub due: Option<String>,
}

/// Text kept from a save that failed, offered back.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Draft {
    pub text: String,
    pub saved_ms: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Merged {
    pub text: String,
    /// Places where both sides changed the same lines (both were kept).
    pub conflicts: u32,
}

fn words_in(text: &str) -> u32 {
    text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count() as u32
}

#[uniffi::export]
impl Marrow {
    /// A note's text: plain, even when it's locked and its password was
    /// entered (else the `Locked` error).
    pub fn read_note(&self, path: String) -> Result<String> {
        let p = vp(&path)?;
        let text = self.with(|e| Ok(e.read_note(&p)?))?;
        Ok(vl::reveal(&self.locks, text)?)
    }

    /// Saves a note. `expected` is the text it had when read: if the file
    /// changed since (sync, another app), nothing is written and the
    /// `ChangedElsewhere` error comes back, so the app can merge.
    pub fn write_note(&self, path: String, text: String, expected: Option<String>) -> Result<()> {
        let p = vp(&path)?;
        let backups = self.backups();
        let events = self.with(|e| {
            let root = e.vault().root().to_path_buf();
            let disk = e.read_note(&p).ok();
            if let (Some(expected), Some(current)) = (&expected, &disk) {
                if &vl::reveal(&self.locks, current.clone())? != expected {
                    return Err(CHANGED.to_string().into());
                }
            }
            let text = vl::conceal(&self.locks, &root, p.as_str(), disk.as_deref(), &text)?;
            if disk.as_deref() == Some(text.as_str()) {
                return Ok(Vec::new());
            }
            if let Some(old) = &disk {
                let h = marrow_core::history::History::for_vault(&root);
                let _ = h.before_save(p.as_str(), old, &text, marrow_core::history::now_ms(), HISTORY_GAP_MS);
            }
            match e.write_note(&p, &text) {
                Ok(events) => {
                    let _ = backups.remove(&root, p.as_str());
                    Ok(events)
                }
                Err(err) => {
                    let kept = backups.put(&root, p.as_str(), &text).is_ok();
                    Err(fail(if kept {
                        format!("Couldn't save: {err}. Your text was kept and will be offered when you open this note again.")
                    } else {
                        format!("Couldn't save: {err}. Copy your text somewhere safe.")
                    }))
                }
            }
        })?;
        let changed = !events.is_empty();
        self.tell(events);
        if changed {
            self.poke_sync();
        }
        Ok(())
    }

    /// Text kept from a failed save of this note, if any (and readable).
    pub fn draft_for(&self, path: String) -> Result<Option<Draft>> {
        let store = self.backups();
        let found = self.with(|e| Ok(store.get(e.vault().root(), &path)))?;
        Ok(found.and_then(|b| Some(Draft { text: vl::reveal(&self.locks, b.text).ok()?, saved_ms: b.saved_at })))
    }

    pub fn discard_draft(&self, path: String) -> Result<()> {
        let store = self.backups();
        self.with(|e| Ok(store.remove(e.vault().root(), &path)?))
    }

    /// Three-way merge: `base` = text when read, `mine` = edited, `theirs` = the file now.
    pub fn merge_text(&self, base: String, mine: String, theirs: String) -> Merged {
        let m = marrow_core::merge3(&base, &mine, &theirs);
        Merged { text: m.text, conflicts: m.conflicts as u32 }
    }

    /// Ticks or unticks the task on `line` of a note; returns the new text.
    pub fn toggle_task(&self, path: String, line: u32) -> Result<String> {
        let text = self.read_note(path.clone())?;
        let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let l = lines.get_mut(line as usize).ok_or_else(|| fail("That task isn't there any more."))?;
        let Some(open) = l.find('[') else { return Err(fail("That line isn't a task.")) };
        let mark = l[open..].chars().nth(1).unwrap_or(' ');
        let rest: String = l[open..].chars().skip(2).collect();
        if !rest.starts_with(']') {
            return Err(fail("That line isn't a task."));
        }
        let new = if mark == ' ' { 'x' } else { ' ' };
        *l = format!("{}[{new}{}", &l[..open], rest);
        let next = lines.join("\n");
        self.write_note(path, next.clone(), Some(text))?;
        Ok(next)
    }

    /// The info sheet for a note.
    pub fn note_details(&self, path: String) -> Result<NoteDetails> {
        let p = vp(&path)?;
        let text = self.read_note(path.clone()).unwrap_or_default();
        let starred = crate::settings::read_vault_settings(&self.root()?).starred.contains(&path);
        self.with(|e| {
            let meta = e.meta(&p);
            let stat = e.vault().stat(&p);
            let outline = meta
                .as_ref()
                .map(|m| m.headings.iter().map(|h| Heading { level: h.level, text: h.text.clone(), line: h.pos.start.line }).collect())
                .unwrap_or_default();
            let mut tags: Vec<String> = Vec::new();
            for t in meta.as_ref().map(|m| m.all_tags()).unwrap_or_default() {
                if !tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                    tags.push(t);
                }
            }
            let mut backlinks = Vec::new();
            for b in e.backlinks(&p) {
                let src = vl::reveal(&self.locks, e.read_note(&b.source).unwrap_or_default()).unwrap_or_default();
                let lines: Vec<&str> = src.lines().collect();
                let mut refs: Vec<LineRef> = b
                    .refs
                    .iter()
                    .map(|r| LineRef { line: r.pos.start.line, text: lines.get(r.pos.start.line as usize).copied().unwrap_or("").trim().chars().take(240).collect() })
                    .collect();
                refs.dedup_by_key(|r| r.line);
                backlinks.push(Backlink { title: display_name(b.source.as_str()), source: b.source.to_string(), lines: refs });
            }
            let name = display_name(&path);
            let mentions = e
                .unlinked_mentions(&p, std::slice::from_ref(&name))
                .into_iter()
                .take(50)
                .map(|m| Mention { title: display_name(m.path.as_str()), source: m.path.to_string(), line: m.line, text: m.text.trim().chars().take(240).collect() })
                .collect();
            Ok(NoteDetails {
                title: name,
                path: path.clone(),
                tags,
                modified_ms: stat.map(|s| s.mtime_ms).unwrap_or(0),
                created_ms: stat.map(|s| s.ctime_ms).unwrap_or(0),
                words: words_in(&text),
                characters: text.chars().count() as u32,
                outline,
                backlinks,
                outgoing: e.outgoing(&p).keys().map(|k| k.to_string()).collect(),
                unresolved: e.unresolved(&p).keys().cloned().collect(),
                mentions,
                starred,
            })
        })
    }

    /// Turns a mention of this note into a link, in the note that mentions it.
    pub fn link_mention(&self, path: String, source: String, line: u32) -> Result<()> {
        let name = display_name(&path);
        let text = self.read_note(source.clone())?;
        let target = vp(&path)?;
        let src = vp(&source)?;
        let link = self.with(|e| Ok(e.link_text(&target, &src)))?;
        let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let l = lines.get_mut(line as usize).ok_or_else(|| fail("That line changed."))?;
        let lower = l.to_lowercase();
        let i = lower.find(&name.to_lowercase()).ok_or_else(|| fail("That line changed."))?;
        let found = &l[i..i + name.len()];
        let repl = if link == found { format!("[[{found}]]") } else { format!("[[{link}|{found}]]") };
        l.replace_range(i..i + name.len(), &repl);
        self.write_note(source, lines.join("\n"), Some(text))
    }

    /// Where a link from `source` leads, or `None` if that note doesn't exist yet.
    pub fn resolve_link(&self, link: String, source: String) -> Result<Option<String>> {
        let src = vp(&source)?;
        self.with(|e| Ok(e.resolve_link(&link, &src).map(|p| p.to_string())))
    }

    /// Where a link leads, making the note if it doesn't exist yet.
    pub fn follow_link(&self, link: String, source: String) -> Result<String> {
        let src = vp(&source)?;
        let (path, events) = self.with(|e| {
            if let Some(p) = e.resolve_link(&link, &src) {
                return Ok((p.to_string(), Vec::new()));
            }
            let target = marrow_core::parse::split_subpath(&link).0.trim().trim_start_matches('/');
            if target.is_empty() {
                return Err(fail("That link is empty."));
            }
            let path = if target.to_lowercase().ends_with(".md") { vp(target)? } else { vp(&format!("{target}.md"))? };
            let ev = e.create_note(&path, "")?;
            Ok((path.to_string(), ev))
        })?;
        self.tell(events);
        Ok(path)
    }

    /// Notes (and other files) whose name matches `query`, best first.
    pub fn find_files(&self, query: String, source: Option<String>, limit: u32) -> Result<Vec<FileHit>> {
        let src = match source.as_deref() {
            Some(s) if !s.is_empty() => vp(s)?,
            _ => VaultPath::root(),
        };
        self.with(|e| {
            Ok(e.find_files(&query, limit as usize)
                .into_iter()
                .filter(|p| !p.is_hidden())
                .map(|p| FileHit {
                    name: display_name(p.as_str()),
                    folder: p.parent().to_string(),
                    is_note: p.is_markdown(),
                    link_text: e.link_text(&p, &src),
                    path: p.to_string(),
                })
                .collect())
        })
    }

    /// Searches the text of every note. Bangla spelling variants match.
    pub fn search(&self, query: String, limit: u32) -> Result<SearchResults> {
        self.with(|e| {
            let r = e.search(&query, limit as usize);
            Ok(SearchResults {
                total: r.total,
                hits: r
                    .hits
                    .into_iter()
                    .map(|h| SearchHit {
                        title: h.name,
                        path: h.path.to_string(),
                        matches: h.matches,
                        lines: h
                            .snippets
                            .into_iter()
                            .map(|s| SearchLine { line: s.line, text: s.text, ranges: s.ranges.into_iter().flat_map(|(a, b)| [a, b]).collect() })
                            .collect(),
                    })
                    .collect(),
            })
        })
    }

    /// Every tag and how many notes carry it, most used first.
    pub fn tags(&self) -> Result<Vec<TagCount>> {
        self.with(|e| {
            let mut v: Vec<TagCount> = e.tags().into_iter().map(|(tag, n)| TagCount { tag, count: n as u32 }).collect();
            v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.to_lowercase().cmp(&b.tag.to_lowercase())));
            Ok(v)
        })
    }

    /// Notes carrying `tag` (with or without `#`; `#a` also finds `#a/b`), newest first.
    pub fn notes_with_tag(&self, tag: String) -> Result<Vec<crate::FileItem>> {
        let want = format!("#{}", tag.trim_start_matches('#')).to_lowercase();
        let paths: Vec<String> = self.with(|e| {
            let r = e.query(&VaultPath::root(), true);
            let mut rows: Vec<_> = r
                .rows
                .into_iter()
                .filter(|row| row.tags.iter().any(|t| {
                    let t = t.to_lowercase();
                    t == want || t.starts_with(&format!("{want}/"))
                }))
                .collect();
            rows.sort_by_key(|r| std::cmp::Reverse(r.modified_ms));
            Ok(rows.into_iter().map(|r| r.path).collect())
        })?;
        Ok(paths.into_iter().filter_map(|p| self.file_item(p).ok()).collect())
    }

    /// Every task in every note.
    pub fn tasks(&self, include_done: bool) -> Result<Vec<TaskItem>> {
        self.with(|e| {
            Ok(e.tasks(include_done)
                .into_iter()
                .map(|t| TaskItem { title: t.name, path: t.path.to_string(), line: t.line, done: t.done, text: t.text, due: t.due })
                .collect())
        })
    }

    /// Notes changed most recently.
    pub fn recent_notes(&self, limit: u32) -> Result<Vec<crate::FileItem>> {
        let paths: Vec<String> = self.with(|e| Ok(e.recent_files(limit as usize * 2).into_iter().map(|r| r.path).collect()))?;
        Ok(paths
            .into_iter()
            .filter(|p| matches!(crate::files::kind_of(p, false), crate::FileKind::Note | crate::FileKind::Canvas))
            .take(limit as usize)
            .filter_map(|p| self.file_item(p).ok())
            .collect())
    }

    /// Starred notes that still exist.
    pub fn starred(&self) -> Result<Vec<crate::FileItem>> {
        let s = crate::settings::read_vault_settings(&self.root()?);
        Ok(s.starred.into_iter().filter_map(|p| self.file_item(p).ok()).collect())
    }

    /// Counts for the home screen.
    pub fn counts(&self) -> Result<Counts> {
        self.with(|e| {
            let s = e.vault_stats();
            Ok(Counts { notes: s.notes as u32, links: s.links as u32, open_tasks: e.tasks(false).len() as u32 })
        })
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Counts {
    pub notes: u32,
    pub links: u32,
    pub open_tasks: u32,
}

#[cfg(test)]
mod tests {

    #[test]
    fn notes_save_link_and_tick() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("Apple.md", "# Apple\nSee [[B]] and #work\n- [ ] call Sam\n"), ("B.md", "B mentions apple here.\n")]);
        let d = m.note_details("B.md".into()).unwrap();
        assert_eq!(d.backlinks.len(), 1);
        assert_eq!(d.backlinks[0].lines[0].text, "See [[B]] and #work");
        let a = m.note_details("Apple.md".into()).unwrap();
        assert_eq!(a.tags, ["#work"]);
        assert_eq!(a.outline[0].text, "Apple");
        assert_eq!(a.mentions.len(), 1);
        m.link_mention("Apple.md".into(), "B.md".into(), 0).unwrap();
        assert_eq!(m.read_note("B.md".into()).unwrap(), "B mentions [[Apple|apple]] here.\n");
        let t = m.toggle_task("Apple.md".into(), 2).unwrap();
        assert!(t.contains("- [x] call Sam"));
        assert_eq!(m.tasks(false).unwrap().len(), 0);
        assert!(matches!(m.write_note("Apple.md".into(), "x".into(), Some("stale".into())), Err(crate::MarrowError::ChangedElsewhere)));
        m.write_note("Apple.md".into(), "new".into(), Some(t)).unwrap();
        assert_eq!(m.search("call".into(), 10).unwrap().total, 0);
        assert_eq!(m.search("mentions".into(), 10).unwrap().hits[0].path, "B.md");
        assert_eq!(m.tags().unwrap().len(), 0);
        assert_eq!(m.follow_link("C".into(), "Apple.md".into()).unwrap(), "C.md");
        assert_eq!(m.find_files("c".into(), None, 5).unwrap()[0].path, "C.md");
    }
}
