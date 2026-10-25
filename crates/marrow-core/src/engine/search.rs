//! Full-text search and tag counts.
//!
//! Search scans note text kept in memory (as Obsidian does) instead of
//! maintaining an inverted index: results are always exact and current, and
//! a few thousand notes scan in milliseconds. Text is loaded lazily on the
//! first search and re-read only for notes whose modified time or size
//! changed since.
//!
//! Query syntax:
//!
//! | input            | matches notes that…                              |
//! |------------------|--------------------------------------------------|
//! | `rust cache`     | contain both words (any case, anywhere)          |
//! | `"exact phrase"` | contain the phrase                               |
//! | `-draft`         | do not contain the word                          |
//! | `tag:#idea` / `#idea` | carry the tag or a sub-tag (`#idea/later`)  |
//! | `path:projects`  | live under a path containing the text            |
//! | `file:2026`      | have a name containing the text                  |
//!
//! Bangla is matched forgivingly: the spellings people mix up (ি/ী, ু/ূ,
//! ন/ণ, শ/ষ/স, জ/য, ং/ঙ, ড়/র, ৎ/ত), chandrabindu, joiners and the two ways
//! of writing য়, ো and ৌ all count as the same, and Bangla digits match
//! 0-9.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::Engine;
use crate::path::VaultPath;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    Tag(String),
    Path(String),
    File(String),
}

/// A parsed search query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    include: Vec<Term>,
    exclude: Vec<Term>,
}

fn parse_query(q: &str) -> Query {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    let mut chars = q.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&first) = chars.peek() else { break };
        let negate = first == '-';
        if negate {
            chars.next();
        }
        // Read one token, honouring quotes anywhere in it (tag:"a b" or "a b").
        let mut token = String::new();
        let mut quoted = false;
        let mut in_quote = false;
        while let Some(&c) = chars.peek() {
            if c == '"' {
                in_quote = !in_quote;
                quoted = true;
                chars.next();
                continue;
            }
            if c.is_whitespace() && !in_quote {
                break;
            }
            token.push(c);
            chars.next();
        }
        if token.is_empty() {
            continue;
        }
        let lower = token.to_lowercase();
        let term = if quoted && !lower.contains(':') {
            Term::Text(fold(&token))
        } else if let Some(t) = lower.strip_prefix("tag:") {
            Term::Tag(t.trim_start_matches('#').to_string())
        } else if let Some(t) = lower.strip_prefix("path:") {
            Term::Path(t.to_string())
        } else if let Some(t) = lower.strip_prefix("file:") {
            Term::File(t.to_string())
        } else if lower.len() > 1 && lower.starts_with('#') {
            Term::Tag(lower[1..].to_string())
        } else {
            Term::Text(fold(&token))
        };
        if matches!(&term, Term::Tag(t) | Term::Path(t) | Term::File(t) | Term::Text(t) if t.is_empty())
        {
            continue;
        }
        if negate {
            exclude.push(term);
        } else {
            include.push(term);
        }
    }
    Query { include, exclude }
}

/// One matching line, ready to show with highlights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    /// Zero-based line number in the note.
    pub line: u32,
    /// The line, trimmed to the area around the first match on long lines.
    pub text: String,
    /// Highlight ranges in `text`, in UTF-16 code units (what JavaScript uses).
    pub ranges: Vec<(u32, u32)>,
    /// UTF-16 column of the first match in the full line, and its length,
    /// so the editor can select it.
    pub ch: u32,
    pub len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: VaultPath,
    /// Note name without `.md`.
    pub name: String,
    /// Number of places in the note that matched.
    pub matches: u32,
    pub snippets: Vec<Snippet>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Notes that matched, including any cut by the limit.
    pub total: u32,
    pub took_ms: u32,
}

/// Lines shown per note.
const LINES_PER_NOTE: usize = 5;
/// Characters of context kept around the first hit in a long line.
const CONTEXT: usize = 70;

#[derive(Clone)]
pub(super) struct CachedText {
    mtime_ms: i64,
    size: u64,
    pub(super) text: Arc<str>,
    pub(super) lower: Arc<str>,
    /// `lower` with Bangla spelling variations folded (see [`fold`]); the
    /// same text as `lower` for notes without Bangla.
    pub(super) folded: Arc<str>,
    /// A password-locked note (its text is left out).
    pub(super) locked: bool,
}

#[derive(Default)]
pub(super) struct TextCache {
    pub(super) entries: HashMap<VaultPath, CachedText>,
}

impl TextCache {
    fn forget(&mut self, p: &VaultPath) {
        self.entries.remove(p);
    }
}

impl Engine {
    /// Drops the cached text of a path so the next search re-reads it.
    pub(super) fn forget_text(&mut self, p: &VaultPath) {
        self.texts.forget(p);
    }

    /// Notes locked with a password, as of now.
    pub fn locked_notes(&mut self) -> Vec<VaultPath> {
        self.refresh_texts();
        let mut out: Vec<VaultPath> = self.texts.entries.iter().filter(|(_, c)| c.locked).map(|(p, _)| p.clone()).collect();
        out.sort();
        out
    }

    /// Loads all note text now, so the first search doesn't pause.
    pub fn warm_search(&mut self) {
        self.refresh_texts();
    }

    /// Brings the in-memory text of every note up to date (cheap after the
    /// first call: only notes whose stat changed are re-read).
    pub(super) fn refresh_texts(&mut self) {
        let stale: Vec<(VaultPath, i64, u64)> = self
            .vault
            .markdown_files()
            .filter_map(|p| {
                let st = self.vault.stat(p)?;
                match self.texts.entries.get(p) {
                    Some(c) if c.mtime_ms == st.mtime_ms && c.size == st.size => None,
                    _ => Some((p.clone(), st.mtime_ms, st.size)),
                }
            })
            .collect();
        let vault = &self.vault;
        let loaded: Vec<(VaultPath, CachedText)> = stale
            .into_par_iter()
            .filter_map(|(p, mtime_ms, size)| {
                let text = vault.read_text(&p).ok()?;
                // Locked notes aren't searched (nor used for tasks, mentions or AI).
                let locked = crate::lock::is_locked(&text);
                let text = if locked { String::new() } else { text };
                let lower: Arc<str> = text.to_lowercase().into();
                let folded = if has_bangla(&text) { fold(&text).into() } else { lower.clone() };
                Some((
                    p,
                    CachedText {
                        mtime_ms,
                        size,
                        text: text.into(),
                        lower,
                        folded,
                        locked,
                    },
                ))
            })
            .collect();
        for (p, c) in loaded {
            self.texts.entries.insert(p, c);
        }
        let live: BTreeSet<&VaultPath> = self.vault.markdown_files().collect();
        self.texts.entries.retain(|p, _| live.contains(p));
    }

    pub fn search(&mut self, query: &str, limit: usize) -> SearchResults {
        let started = Instant::now();
        let q = parse_query(query);
        if q.include.is_empty() && q.exclude.is_empty() {
            return SearchResults::default();
        }
        self.refresh_texts();

        let words: Vec<&str> = q
            .include
            .iter()
            .filter_map(|t| match t {
                Term::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();

        let metas = &self.metas;
        let note_tags = |p: &VaultPath| -> Vec<String> {
            metas
                .get(p)
                .map(|m| {
                    m.all_tags()
                        .into_iter()
                        .map(|t| t.trim_start_matches('#').to_lowercase())
                        .collect()
                })
                .unwrap_or_default()
        };
        let matches = |term: &Term, p: &VaultPath, c: &CachedText| -> bool {
            match term {
                Term::Text(s) => {
                    c.folded.contains(s.as_str()) || fold(p.stem()).contains(s.as_str())
                }
                Term::Tag(t) => note_tags(p)
                    .iter()
                    .any(|n| n == t || n.starts_with(&format!("{t}/"))),
                Term::Path(s) => p.as_str().to_lowercase().contains(s.as_str()),
                Term::File(s) => p.stem().to_lowercase().contains(s.as_str()),
            }
        };

        let mut hits: Vec<(bool, SearchHit)> = self
            .texts
            .entries
            .par_iter()
            .filter(|(p, c)| {
                q.include.iter().all(|t| matches(t, p, c))
                    && !q.exclude.iter().any(|t| matches(t, p, c))
            })
            .map(|(p, c)| {
                let name = p.stem().to_string();
                let name_lower = fold(&name);
                let title_match = words.iter().any(|w| name_lower.contains(w));
                let mut snippets = Vec::new();
                let mut count = 0u32;
                if !words.is_empty() {
                    for (n, line) in c.text.lines().enumerate() {
                        let ranges = find_all(line, &words);
                        if ranges.is_empty() {
                            continue;
                        }
                        count += ranges.len() as u32;
                        if snippets.len() < LINES_PER_NOTE {
                            snippets.push(snippet(n as u32, line, &ranges));
                        }
                    }
                }
                (
                    title_match,
                    SearchHit {
                        path: p.clone(),
                        name,
                        matches: count,
                        snippets,
                    },
                )
            })
            .collect();

        // Name matches first, then the most matches, then by path.
        hits.sort_by(|(ta, a), (tb, b)| {
            tb.cmp(ta)
                .then(b.matches.cmp(&a.matches))
                .then_with(|| a.path.cmp(&b.path))
        });
        let total = hits.len() as u32;
        hits.truncate(limit);
        SearchResults {
            hits: hits.into_iter().map(|(_, h)| h).collect(),
            total,
            took_ms: started.elapsed().as_millis() as u32,
        }
    }
}

const NUKTA: char = '\u{09BC}';

fn has_bangla(s: &str) -> bool {
    s.chars().any(|c| ('\u{0980}'..='\u{09FF}').contains(&c))
}

/// One Bangla character as search sees it (None: ignored).
fn fold_one(c: char) -> Option<char> {
    Some(match c {
        'ী' => 'ি',
        'ঈ' => 'ই',
        'ূ' => 'ু',
        'ঊ' => 'উ',
        'ণ' => 'ন',
        'শ' | 'ষ' => 'স',
        'য' => 'জ',
        '\u{09DC}' | '\u{09DD}' => 'র',
        'ঙ' => 'ং',
        'ৎ' => 'ত',
        '\u{0981}' | '\u{200C}' | '\u{200D}' | NUKTA => return None,
        '\u{09E6}'..='\u{09EF}' => char::from(b'0' + (c as u32 - 0x09E6) as u8),
        _ => c,
    })
}

/// Lower-cases `s` and folds Bangla spelling variations, recording for each
/// byte of the result the byte offset in `s` of the character it came from
/// (plus a final entry for `s.len()`).
fn fold_with_map(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len() + 1);
    let mut push = |c: char, at: usize, out: &mut String| {
        let before = out.len();
        out.push(c);
        map.extend(std::iter::repeat_n(at, out.len() - before));
    };
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        let next = chars.get(i + 1).map(|x| x.1);
        // Two-character spellings of one letter or vowel sign.
        let pair = match (c, next) {
            ('য', Some(NUKTA)) => Some('\u{09DF}'),
            ('ড', Some(NUKTA)) | ('ঢ', Some(NUKTA)) => Some('র'),
            ('ে', Some('া')) => Some('ো'),
            ('ে', Some('\u{09D7}')) => Some('ৌ'),
            _ => None,
        };
        if let Some(p) = pair {
            push(p, at, &mut out);
            i += 2;
            continue;
        }
        if ('\u{0980}'..='\u{09FF}').contains(&c) || c == '\u{200C}' || c == '\u{200D}' {
            if let Some(f) = fold_one(c) {
                push(f, at, &mut out);
            }
        } else {
            for lc in c.to_lowercase() {
                push(lc, at, &mut out);
            }
        }
        i += 1;
    }
    map.push(s.len());
    (out, map)
}

/// `s` as search compares it: lower-case, Bangla spellings folded.
pub fn fold(s: &str) -> String {
    fold_with_map(s).0
}

/// Byte ranges in `line` where any of `words` (already lower-case) occur,
/// merged and in order.
fn find_all(line: &str, words: &[&str]) -> Vec<(usize, usize)> {
    let (lower, map) = fold_with_map(line);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for w in words {
        let mut from = 0;
        while let Some(i) = lower[from..].find(w) {
            let start = from + i;
            let end = start + w.len();
            let (os, oe) = (map[start], *map.get(end).unwrap_or(&line.len()));
            // Extend to the end of the character the match ends in.
            let oe = if oe <= os {
                line[os..].chars().next().map_or(os, |c| os + c.len_utf8())
            } else {
                oe
            };
            ranges.push((os, oe));
            from = end.max(start + 1);
            while from < lower.len() && !lower.is_char_boundary(from) {
                from += 1;
            }
        }
    }
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
            _ => merged.push(r),
        }
    }
    merged
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// Builds a snippet for one line: trims long lines to the area around the
/// first match and converts byte ranges to UTF-16 ranges within the snippet.
fn snippet(line_no: u32, line: &str, ranges: &[(usize, usize)]) -> Snippet {
    let (first_s, first_e) = ranges[0];
    let ch = utf16_len(&line[..first_s]);
    let len = utf16_len(&line[first_s..first_e]);

    // Window: some context before the first match, more after it.
    let mut start = first_s.saturating_sub(CONTEXT);
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (first_s + CONTEXT * 3).min(line.len());
    while !line.is_char_boundary(end) {
        end += 1;
    }
    // Leading indentation adds nothing.
    let lead = line[start..end].len() - line[start..end].trim_start().len();
    let start = start + lead;

    let prefix = if start > 0 && lead == 0 { "…" } else { "" };
    let suffix = if end < line.len() { "…" } else { "" };
    let body = &line[start..end];
    let mut text = String::with_capacity(body.len() + 6);
    text.push_str(prefix);
    text.push_str(body);
    text.push_str(suffix);

    let offset = utf16_len(prefix);
    let out_ranges = ranges
        .iter()
        .filter(|(s, e)| *e > start && *s < end)
        .map(|&(s, e)| {
            let s = s.max(start);
            let e = e.min(end);
            (
                offset + utf16_len(&line[start..s]),
                offset + utf16_len(&line[start..e]),
            )
        })
        .collect();
    Snippet {
        line: line_no,
        text,
        ranges: out_ranges,
        ch,
        len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use std::fs;

    fn engine(files: &[(&str, &str)]) -> (tempfile::TempDir, Engine) {
        let d = tempfile::tempdir().unwrap();
        for (p, body) in files {
            let full = d.path().join(p);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        (d, e)
    }

    fn paths(r: &SearchResults) -> Vec<&str> {
        r.hits.iter().map(|n| n.path.as_str()).collect()
    }

    /// The highlighted pieces of a snippet.
    fn marked(s: &Snippet) -> Vec<String> {
        let units: Vec<u16> = s.text.encode_utf16().collect();
        s.ranges
            .iter()
            .map(|&(a, b)| String::from_utf16(&units[a as usize..b as usize]).unwrap())
            .collect()
    }

    #[test]
    fn parses_queries() {
        let q =
            parse_query(r#"rust "link resolver" -draft tag:#idea #later path:projects file:2026"#);
        assert_eq!(
            q.include,
            vec![
                Term::Text("rust".into()),
                Term::Text("link resolver".into()),
                Term::Tag("idea".into()),
                Term::Tag("later".into()),
                Term::Path("projects".into()),
                Term::File("2026".into()),
            ]
        );
        assert_eq!(q.exclude, vec![Term::Text("draft".into())]);
        assert_eq!(parse_query("   ").include, vec![]);
    }

    #[test]
    fn words_are_and_ed_and_case_insensitive() {
        let (_d, mut e) = engine(&[
            ("a.md", "Rust and the Cache.\nNothing here.\n"),
            ("b.md", "Only rust.\n"),
            ("c.md", "Only cache.\n"),
        ]);
        let r = e.search("rust CACHE", 10);
        assert_eq!(paths(&r), vec!["a.md"]);
        let s = &r.hits[0].snippets[0];
        assert_eq!((s.line, s.text.as_str()), (0, "Rust and the Cache."));
        assert_eq!(marked(s), vec!["Rust", "Cache"]);
        assert_eq!((s.ch, s.len), (0, 4));
        assert_eq!(r.hits[0].matches, 2);
    }

    #[test]
    fn phrases_exclusions_paths_and_files() {
        let (_d, mut e) = engine(&[
            ("projects/Resolver.md", "The link resolver works.\n"),
            ("projects/Draft.md", "link and resolver, draft\n"),
            (
                "journal/2026-10-14.md",
                "Worked on the link resolver today.\n",
            ),
        ]);
        assert_eq!(paths(&e.search(r#""link resolver""#, 10)).len(), 2);
        assert_eq!(
            paths(&e.search(r#""link resolver" path:projects"#, 10)),
            vec!["projects/Resolver.md"]
        );
        assert_eq!(paths(&e.search("resolver -draft", 10)).len(), 2);
        assert_eq!(
            paths(&e.search("file:2026", 10)),
            vec!["journal/2026-10-14.md"]
        );
        assert!(e.search("file:2026", 10).hits[0].snippets.is_empty());
    }

    #[test]
    fn tags_match_sub_tags_and_frontmatter() {
        let (_d, mut e) = engine(&[
            ("a.md", "#idea/later\n"),
            ("b.md", "---\ntags: [idea]\n---\nbody\n"),
            ("c.md", "#ideas are not #idea\n"),
            ("d.md", "nothing\n"),
        ]);
        let r = e.search("tag:#idea", 10);
        let mut got = paths(&r);
        got.sort();
        assert_eq!(got, vec!["a.md", "b.md", "c.md"]);
        assert_eq!(paths(&e.search("#idea/later", 10)), vec!["a.md"]);

        let counts = e.tags();
        assert_eq!(counts["#idea"], 3);
        assert_eq!(counts["#idea/later"], 1);
        assert_eq!(counts["#ideas"], 1);
    }

    #[test]
    fn title_matches_rank_first_and_results_stay_fresh() {
        let (d, mut e) = engine(&[
            ("Cache design.md", "notes\n"),
            ("Other.md", "cache cache\ncache\n"),
        ]);
        assert_eq!(
            paths(&e.search("cache", 10)),
            vec!["Cache design.md", "Other.md"]
        );
        assert_eq!(e.search("cache", 10).hits[1].matches, 3);

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            d.path().join("Other.md"),
            "now about something else entirely\n",
        )
        .unwrap();
        e.on_paths_changed(&[VaultPath::new("Other.md").unwrap()])
            .unwrap();
        assert_eq!(paths(&e.search("cache", 10)), vec!["Cache design.md"]);
    }

    #[test]
    fn unicode_case_and_long_lines() {
        let (_d, mut e) = engine(&[("u.md", "İstanbul and STRASSE and ÉCOLE\n")]);
        let r = e.search("école", 10);
        assert_eq!(marked(&r.hits[0].snippets[0]), vec!["ÉCOLE"]);

        // Emoji take two UTF-16 units; the column must count them that way.
        let (_d3, mut e3) = engine(&[("e.md", "🦀🦀 crab\n")]);
        let s = &e3.search("crab", 10).hits[0].snippets[0];
        assert_eq!((s.ch, s.len), (5, 4));
        assert_eq!(marked(s), vec!["crab"]);

        let long = format!("{}needle{}\n", "a".repeat(500), "b".repeat(500));
        let (_d2, mut e2) = engine(&[("l.md", &long)]);
        let s = &e2.search("needle", 10).hits[0].snippets[0];
        assert!(s.text.starts_with('…') && s.text.ends_with('…'));
        assert!(s.text.len() < 400);
        assert_eq!(marked(s), vec!["needle"]);
        assert_eq!(s.ch, 500);
    }

    #[test]
    fn bangla_spellings_match() {
        let (_d, mut e) = engine(&[
            ("a.md", "আমি বাংলাদেশে থাকি। সূর্য উঠেছে।\nসাল ২০২৬"),
            ("b.md", "ভবিষ্যৎ নিয়ে ভাবনা"),
            ("c.md", "nothing here"),
        ]);
        // ূ/ু, য/জ
        assert_eq!(paths(&e.search("সুর্জ", 10)), vec!["a.md"]);
        // ৎ/ত, ষ/শ/স, য় written as য + nukta
        assert_eq!(paths(&e.search("ভবিশ্যত", 10)), vec!["b.md"]);
        assert_eq!(paths(&e.search("নি\u{09AF}\u{09BC}ে", 10)), vec!["b.md"]);
        // ং/ঙ and digits both ways
        assert_eq!(paths(&e.search("বাঙলাদেশ", 10)), vec!["a.md"]);
        assert_eq!(paths(&e.search("2026", 10)), vec!["a.md"]);
        let r = e.search("সূর্য", 10);
        assert_eq!(marked(&r.hits[0].snippets[0]), vec!["সূর্য"]);
        let r = e.search("২০২৬", 10);
        assert_eq!(marked(&r.hits[0].snippets[0]), vec!["২০২৬"]);
        // Ordinary text is unaffected.
        assert_eq!(paths(&e.search("NOTHING", 10)), vec!["c.md"]);
        assert_eq!(fold("Ünïcode"), "ünïcode");
    }
}
