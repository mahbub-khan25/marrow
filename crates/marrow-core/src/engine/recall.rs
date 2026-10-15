//! Finding the parts of the vault that answer a question: notes are cut into
//! chunks (a heading's section, or a few paragraphs), and chunks are ranked
//! by BM25 over their words. Works with no AI at all; the app adds meaning
//! search (embeddings) on top when a model for it is set up.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Engine;
use crate::path::VaultPath;

/// A piece of a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub path: VaultPath,
    /// Note name without `.md`.
    pub name: String,
    /// Zero-based line where the chunk starts.
    pub line: u32,
    /// The heading it sits under, if any.
    pub heading: String,
    pub text: String,
    /// Content hash (of name, heading and text), to reuse embeddings.
    pub hash: u64,
}

/// Chunks aim for about this many characters.
const TARGET: usize = 1200;

impl Engine {
    /// Every note cut into chunks, sorted by path and line.
    pub fn chunks(&mut self) -> Vec<Chunk> {
        self.refresh_texts();
        let mut out: Vec<Chunk> = self
            .texts
            .entries
            .iter()
            .flat_map(|(p, c)| chunk_note(p, &c.text))
            .collect();
        out.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()).then(a.line.cmp(&b.line)));
        out
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Cuts one note: sections by heading, long sections by paragraphs.
pub fn chunk_note(path: &VaultPath, text: &str) -> Vec<Chunk> {
    let name = path.stem().to_string();
    let lines: Vec<&str> = text.lines().collect();
    // Skip the frontmatter.
    let mut start = 0;
    if lines.first().map(|l| l.trim()) == Some("---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---" || l.trim() == "...") {
            start = end + 2;
        }
    }
    let mut out = Vec::new();
    let mut heading = String::new();
    let mut buf: Vec<(usize, &str)> = Vec::new();
    let mut in_fence = false;
    let flush = |buf: &mut Vec<(usize, &str)>, heading: &str, out: &mut Vec<Chunk>| {
        // Group lines into pieces near TARGET, breaking at blank lines.
        let mut piece: Vec<(usize, &str)> = Vec::new();
        let mut size = 0;
        let mut emit = |piece: &mut Vec<(usize, &str)>| {
            let body = piece.iter().map(|(_, l)| *l).collect::<Vec<_>>().join("\n");
            let body = body.trim();
            if !body.is_empty() && body.chars().any(|c| c.is_alphanumeric()) {
                let first = piece.iter().find(|(_, l)| !l.trim().is_empty()).map(|(n, _)| *n).unwrap_or(0);
                out.push(Chunk {
                    path: path.clone(),
                    name: name.clone(),
                    line: first as u32,
                    heading: heading.to_string(),
                    text: body.to_string(),
                    hash: fnv(&format!("{name}\u{1}{heading}\u{1}{body}")),
                });
            }
            piece.clear();
        };
        for &(n, l) in buf.iter() {
            if l.trim().is_empty() && size >= TARGET {
                emit(&mut piece);
                size = 0;
                continue;
            }
            piece.push((n, l));
            size += l.len() + 1;
            // A single huge paragraph still gets cut.
            if size >= TARGET * 3 {
                emit(&mut piece);
                size = 0;
            }
        }
        emit(&mut piece);
        buf.clear();
    };
    for (n, l) in lines.iter().enumerate().skip(start) {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if !in_fence && t.starts_with('#') {
            let level = t.chars().take_while(|c| *c == '#').count();
            if level <= 6 && t[level..].starts_with(' ') {
                flush(&mut buf, &heading, &mut out);
                heading = t[level..].trim().to_string();
                continue;
            }
        }
        buf.push((n, l));
    }
    flush(&mut buf, &heading, &mut out);
    out
}

/// Words for ranking: lower-case letters and digits; Chinese, Japanese and
/// Korean text (no spaces) as overlapping pairs of characters.
pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut cjk: Vec<char> = Vec::new();
    let is_cjk = |c: char| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF);
    let flush_cjk = |cjk: &mut Vec<char>, out: &mut Vec<String>| {
        if cjk.len() == 1 {
            out.push(cjk[0].to_string());
        }
        for w in cjk.windows(2) {
            out.push(w.iter().collect());
        }
        cjk.clear();
    };
    for c in s.chars() {
        if is_cjk(c) {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            cjk.push(c);
        } else if c.is_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            word.extend(c.to_lowercase());
        } else {
            flush_cjk(&mut cjk, &mut out);
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
        }
    }
    flush_cjk(&mut cjk, &mut out);
    if !word.is_empty() {
        out.push(word);
    }
    out.retain(|w| !STOP.contains(&w.as_str()));
    out
}

/// Common English words that say nothing about a topic.
const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how", "i", "in", "is", "it", "me", "my",
    "of", "on", "or", "our", "that", "the", "this", "to", "was", "what", "when", "where", "which", "who", "why", "with", "you", "your",
    "about", "did", "have", "has", "we", "there", "their", "they", "any", "all", "tell", "show", "find", "notes", "note",
];

/// BM25 ranking of `chunks` for `query`; (index, score), best first.
pub fn bm25(chunks: &[Chunk], query: &str, k: usize) -> Vec<(usize, f32)> {
    let q: Vec<String> = {
        let mut t = tokens(query);
        t.sort();
        t.dedup();
        t
    };
    if q.is_empty() || chunks.is_empty() {
        return Vec::new();
    }
    let docs: Vec<Vec<String>> = chunks.iter().map(|c| tokens(&format!("{} {} {}", c.name, c.heading, c.text))).collect();
    let n = docs.len() as f32;
    let avg = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n.max(1.0);
    let mut df: HashMap<&str, f32> = HashMap::new();
    for d in &docs {
        let mut seen: Vec<&str> = d.iter().map(|s| s.as_str()).filter(|w| q.iter().any(|x| x == w)).collect();
        seen.sort();
        seen.dedup();
        for w in seen {
            *df.entry(w).or_default() += 1.0;
        }
    }
    let (k1, b) = (1.4f32, 0.75f32);
    let mut scored: Vec<(usize, f32)> = docs
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let len = d.len() as f32;
            let mut s = 0.0;
            for w in &q {
                let tf = d.iter().filter(|x| *x == w).count() as f32;
                if tf == 0.0 {
                    continue;
                }
                let dfw = df.get(w.as_str()).copied().unwrap_or(0.0);
                let idf = ((n - dfw + 0.5) / (dfw + 0.5) + 1.0).ln();
                s += idf * tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * len / avg.max(1.0)));
            }
            (s > 0.0).then_some((i, s))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_by_heading_and_size() {
        let p = VaultPath::new("A/Note.md").unwrap();
        let text = format!("---\ntags: [x]\n---\nIntro line.\n\n## First\nAlpha beta.\n\n## Second\n{}\n\n{}\n```\n# not a heading\n```", "word ".repeat(300), "more ".repeat(300));
        let c = chunk_note(&p, &text);
        assert_eq!(c[0].text, "Intro line.");
        assert_eq!(c[0].line, 3);
        assert_eq!(c[1].heading, "First");
        assert!(c.iter().filter(|x| x.heading == "Second").count() >= 2);
        assert!(c.iter().any(|x| x.text.contains("# not a heading")));
        assert!(!c.iter().any(|x| x.text.contains("tags:")));
    }

    #[test]
    fn ranks_by_words() {
        let p = VaultPath::new("n.md").unwrap();
        let chunks = vec![
            Chunk { path: p.clone(), name: "Gardening".into(), line: 0, heading: "".into(), text: "Tomatoes need sun and water.".into(), hash: 1 },
            Chunk { path: p.clone(), name: "Printing".into(), line: 0, heading: "".into(), text: "Halftone screens make dots.".into(), hash: 2 },
            Chunk { path: p, name: "学习".into(), line: 0, heading: "".into(), text: "间隔重复可以帮助记忆".into(), hash: 3 },
        ];
        let r = bm25(&chunks, "How do halftone dots work?", 3);
        assert_eq!(r[0].0, 1);
        let r = bm25(&chunks, "重复记忆", 3);
        assert_eq!(r[0].0, 2);
        assert!(bm25(&chunks, "the and of", 3).is_empty());
    }
}
