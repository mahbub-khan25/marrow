//! Unlinked mentions: places where a note's name (or an alias) appears as
//! plain text in another note, not inside a link, code or the frontmatter.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::Engine;
use crate::path::VaultPath;

/// One plain-text mention of a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mention {
    pub path: VaultPath,
    /// Note name without `.md`.
    pub name: String,
    /// Zero-based line number.
    pub line: u32,
    /// The whole line.
    pub text: String,
    /// Where the mention is in the line, in UTF-16 code units.
    pub ch: u32,
    pub len: u32,
    /// The text as written there.
    pub matched: String,
}

/// At most this many mentions are returned.
const LIMIT: usize = 300;

impl Engine {
    /// Plain-text mentions of `target` by any of `names` (case-insensitive,
    /// whole words) in other notes.
    pub fn unlinked_mentions(&mut self, target: &VaultPath, names: &[String]) -> Vec<Mention> {
        let names: Vec<Vec<char>> = names
            .iter()
            .map(|n| n.trim().chars().flat_map(char::to_lowercase).collect::<Vec<char>>())
            .filter(|n| n.len() >= 2)
            .collect();
        if names.is_empty() {
            return Vec::new();
        }
        self.refresh_texts();
        let mut out: Vec<Mention> = self
            .texts
            .entries
            .par_iter()
            .filter(|(p, _)| *p != target)
            .flat_map_iter(|(p, c)| {
                // Cheap check first: does any name occur at all?
                let hit = names.iter().any(|n| c.lower.contains(&n.iter().collect::<String>()));
                let found = if hit { scan(&c.text, &names) } else { Vec::new() };
                let name = p.stem().to_string();
                found.into_iter().map(move |(line, text, ch, len, matched)| Mention {
                    path: p.clone(),
                    name: name.clone(),
                    line,
                    text,
                    ch,
                    len,
                    matched,
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.line.cmp(&b.line)).then(a.ch.cmp(&b.ch)));
        out.truncate(LIMIT);
        out
    }
}

/// Finds whole-word, case-insensitive occurrences of `names` outside links,
/// code, URLs and frontmatter. Returns (line, line text, utf16 col, utf16 len, matched).
fn scan(text: &str, names: &[Vec<char>]) -> Vec<(u32, String, u32, u32, String)> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    let mut in_front = false;
    for (n, line) in text.lines().enumerate() {
        let t = line.trim();
        if n == 0 && t == "---" {
            in_front = true;
            continue;
        }
        if in_front {
            if t == "---" || t == "..." {
                in_front = false;
            }
            continue;
        }
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
        let masked = mask(&chars);
        // UTF-16 offsets of each char.
        let mut at16 = Vec::with_capacity(chars.len() + 1);
        let mut acc = 0u32;
        for c in &chars {
            at16.push(acc);
            acc += c.len_utf16() as u32;
        }
        at16.push(acc);
        let mut taken = vec![false; chars.len()];
        // Longer names first, so "Atomic notes" wins over "Atomic".
        let mut order: Vec<&Vec<char>> = names.iter().collect();
        order.sort_by_key(|n| std::cmp::Reverse(n.len()));
        for name in order {
            let k = name.len();
            if k > lower.len() {
                continue;
            }
            for i in 0..=lower.len() - k {
                if lower[i..i + k] != name[..] {
                    continue;
                }
                if (i..i + k).any(|j| masked[j] || taken[j]) {
                    continue;
                }
                let before = i.checked_sub(1).map(|j| chars[j]);
                let after = chars.get(i + k).copied();
                let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                if word(before) || word(after) {
                    continue;
                }
                for slot in taken.iter_mut().skip(i).take(k) {
                    *slot = true;
                }
                out.push((n as u32, line.to_string(), at16[i], at16[i + k] - at16[i], chars[i..i + k].iter().collect()));
            }
        }
    }
    out
}

/// Marks characters inside `[[links]]`, `[text](url)`, `code`, URLs and #tags.
fn mask(c: &[char]) -> Vec<bool> {
    let n = c.len();
    let mut m = vec![false; n];
    let find = |from: usize, pat: &[char]| -> Option<usize> {
        (from..n.saturating_sub(pat.len() - 1)).find(|&i| c[i..i + pat.len()] == *pat)
    };
    let mut i = 0;
    while i < n {
        let end = if c[i] == '[' && c.get(i + 1) == Some(&'[') {
            find(i + 2, &[']', ']']).map(|e| e + 2)
        } else if c[i] == '[' {
            // [text](url)
            find(i + 1, &[']', '(']).and_then(|e| find(e + 2, &[')']).map(|f| f + 1))
        } else if c[i] == '`' {
            find(i + 1, &['`']).map(|e| e + 1)
        } else if c[i] == '#' && (i == 0 || c[i - 1].is_whitespace()) && c.get(i + 1).is_some_and(|x| !x.is_whitespace() && *x != '#') {
            Some((i + 1..n).find(|&j| c[j].is_whitespace()).unwrap_or(n))
        } else if c[i].is_ascii_alphabetic() && (i == 0 || !c[i - 1].is_alphanumeric()) {
            // scheme://…
            let mut j = i;
            while j < n && (c[j].is_ascii_alphanumeric() || matches!(c[j], '+' | '.' | '-')) {
                j += 1;
            }
            if c.get(j) == Some(&':') && c.get(j + 1) == Some(&'/') && c.get(j + 2) == Some(&'/') {
                Some((j..n).find(|&k| c[k].is_whitespace()).unwrap_or(n))
            } else {
                None
            }
        } else {
            None
        };
        match end {
            Some(e) if e > i => {
                let from = if i > 0 && c[i - 1] == '!' { i - 1 } else { i };
                for v in m.iter_mut().take(e).skip(from) {
                    *v = true;
                }
                i = e;
            }
            _ => i += 1,
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::scan;

    fn names(v: &[&str]) -> Vec<Vec<char>> {
        v.iter().map(|n| n.to_lowercase().chars().collect()).collect()
    }

    #[test]
    fn finds_plain_mentions_only() {
        let text = "---\ntitle: Atomic notes\n---\nWrite atomic notes daily.\nSee [[Atomic notes]] and `atomic notes`.\nNot atomic notesy.\n```\natomic notes\n```\nATOMIC NOTES!";
        let hits = scan(text, &names(&["Atomic notes"]));
        let lines: Vec<u32> = hits.iter().map(|h| h.0).collect();
        assert_eq!(lines, vec![3, 9]);
        assert_eq!(hits[0].2, 6);
        assert_eq!(hits[0].3, 12);
        assert_eq!(hits[0].4, "atomic notes");
        assert_eq!(hits[1].4, "ATOMIC NOTES");
    }

    #[test]
    fn utf16_offsets_and_longest_name() {
        let text = "😀 Gutenberg's press changed Gutenberg";
        let hits = scan(text, &names(&["Gutenberg", "Gutenberg's press"]));
        assert_eq!(hits.len(), 2);
        let first = hits.iter().find(|h| h.4 == "Gutenberg's press").unwrap();
        assert_eq!(first.2, 3); // 😀 is two UTF-16 units, then a space
        assert!(hits.iter().any(|h| h.4 == "Gutenberg" && h.2 > 10));
    }
}
