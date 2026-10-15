//! Three-way merge of note text.
//!
//! Used when another app changes a note while it has unsaved edits in the
//! editor: `base` is the text last saved, `mine` is the editor, `theirs` is
//! what's on disk now.
//!
//! The merge works line by line, like Git's, with one difference: it never
//! writes conflict markers and never drops text. Where both sides changed the
//! same lines differently, it keeps both versions, mine first. The caller is
//! told how many places that happened so it can say so.

use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeOutcome {
    pub text: String,
    /// Places where both sides changed the same lines and both were kept.
    pub conflicts: usize,
}

/// A change one side made: base lines `start..end` became `lines`.
#[derive(Debug, Clone)]
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

fn split_lines(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

fn hunks<'a>(base: &[&str], other: &[&'a str]) -> Vec<Hunk<'a>> {
    let mut out: Vec<Hunk<'a>> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, base, other) {
        let (old, new) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index..old_index + old_len, new_index..new_index),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index..old_index, new_index..new_index + new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        };
        let lines = other[new].to_vec();
        // Fold ops that touch into one hunk (a delete followed by an insert).
        if let Some(last) = out.last_mut() {
            if last.end == old.start {
                last.end = old.end;
                last.lines.extend(lines);
                continue;
            }
        }
        out.push(Hunk {
            start: old.start,
            end: old.end,
            lines,
        });
    }
    out
}

/// Two changes collide if their base ranges overlap, or if they both start
/// at the same place (two insertions at one point, or an insertion right
/// where the other side starts editing).
fn collide(a: &Hunk, b: &Hunk) -> bool {
    (a.start < b.end && b.start < a.end) || a.start == b.start
}

pub fn merge3(base: &str, mine: &str, theirs: &str) -> MergeOutcome {
    if mine == theirs {
        return MergeOutcome {
            text: mine.to_string(),
            conflicts: 0,
        };
    }
    if base == mine {
        return MergeOutcome {
            text: theirs.to_string(),
            conflicts: 0,
        };
    }
    if base == theirs {
        return MergeOutcome {
            text: mine.to_string(),
            conflicts: 0,
        };
    }

    let b = split_lines(base);
    let m = split_lines(mine);
    let t = split_lines(theirs);
    let hm = hunks(&b, &m);
    let ht = hunks(&b, &t);

    let mut out = String::with_capacity(mine.len().max(theirs.len()));
    let mut conflicts = 0;
    let mut pos = 0; // next base line not yet emitted
    let (mut i, mut j) = (0, 0);

    while i < hm.len() || j < ht.len() {
        // Start a group with whichever change comes first in the base.
        let take_mine = match (hm.get(i), ht.get(j)) {
            (Some(a), Some(c)) => a.start <= c.start,
            (Some(_), None) => true,
            _ => false,
        };
        let (mut gs, mut ge) = if take_mine {
            (hm[i].start, hm[i].end)
        } else {
            (ht[j].start, ht[j].end)
        };
        let (mi0, ti0) = (i, j);
        if take_mine {
            i += 1;
        } else {
            j += 1;
        }
        // Pull in every change from either side that collides with the group.
        loop {
            let probe = Hunk {
                start: gs,
                end: ge,
                lines: vec![],
            };
            if let Some(a) = hm.get(i).filter(|a| collide(a, &probe)) {
                gs = gs.min(a.start);
                ge = ge.max(a.end);
                i += 1;
                continue;
            }
            if let Some(c) = ht.get(j).filter(|c| collide(c, &probe)) {
                gs = gs.min(c.start);
                ge = ge.max(c.end);
                j += 1;
                continue;
            }
            break;
        }

        // Unchanged base lines before the group.
        for line in &b[pos..gs] {
            out.push_str(line);
        }
        pos = ge;

        let side = |hs: &[Hunk]| -> Vec<String> {
            // This side's text for base lines gs..ge.
            let mut v = Vec::new();
            let mut p = gs;
            for h in hs {
                for line in &b[p..h.start] {
                    v.push(line.to_string());
                }
                v.extend(h.lines.iter().map(|s| s.to_string()));
                p = h.end;
            }
            for line in &b[p..ge] {
                v.push(line.to_string());
            }
            v
        };
        let mine_part = (i > mi0).then(|| side(&hm[mi0..i]));
        let theirs_part = (j > ti0).then(|| side(&ht[ti0..j]));
        let chosen: Vec<String> = match (mine_part, theirs_part) {
            (Some(a), None) => a,
            (None, Some(c)) => c,
            (Some(a), Some(c)) if a == c => a,
            (Some(a), Some(c)) => {
                conflicts += 1;
                let mut both = a;
                // Keep a line break between the two versions.
                if both.last().is_some_and(|l| !l.ends_with('\n')) {
                    if let Some(l) = both.last_mut() {
                        l.push('\n');
                    }
                }
                both.extend(c);
                both
            }
            (None, None) => Vec::new(),
        };
        for line in chosen {
            out.push_str(&line);
        }
    }
    for line in &b[pos..] {
        out.push_str(line);
    }
    MergeOutcome {
        text: out,
        conflicts,
    }
}

/// Combines two versions of a note when there's no earlier version to compare
/// with (a sync app's conflict copy, or two folders synced for the first
/// time). Lines both have are kept once; lines only one has are kept; where
/// the two differ, both versions are kept, `a` first.
pub fn merge2(a: &str, b: &str) -> MergeOutcome {
    if a == b {
        return MergeOutcome { text: a.to_string(), conflicts: 0 };
    }
    let la = split_lines(a);
    let lb = split_lines(b);
    let mut out: Vec<String> = Vec::with_capacity(la.len().max(lb.len()));
    let mut conflicts = 0;
    let push = |out: &mut Vec<String>, lines: &[&str]| {
        // Keep a line break before what follows.
        if let Some(last) = out.last_mut() {
            if !last.ends_with('\n') {
                last.push('\n');
            }
        }
        out.extend(lines.iter().map(|s| s.to_string()));
    };
    // Compare without line endings, so "x" and "x\n" count as the same line.
    let ka: Vec<&str> = la.iter().map(|l| l.trim_end_matches(['\r', '\n'])).collect();
    let kb: Vec<&str> = lb.iter().map(|l| l.trim_end_matches(['\r', '\n'])).collect();
    for op in capture_diff_slices(Algorithm::Myers, &ka, &kb) {
        match op {
            DiffOp::Equal { old_index, new_index, len } => {
                for i in 0..len {
                    let (x, y) = (la[old_index + i], lb[new_index + i]);
                    push(&mut out, &[if x.ends_with('\n') { x } else { y }]);
                }
            }
            DiffOp::Delete { old_index, old_len, .. } => push(&mut out, &la[old_index..old_index + old_len]),
            DiffOp::Insert { new_index, new_len, .. } => push(&mut out, &lb[new_index..new_index + new_len]),
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                conflicts += 1;
                push(&mut out, &la[old_index..old_index + old_len]);
                push(&mut out, &lb[new_index..new_index + new_len]);
            }
        }
    }
    MergeOutcome { text: out.concat(), conflicts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(base: &str, mine: &str, theirs: &str) -> (String, usize) {
        let r = merge3(base, mine, theirs);
        (r.text, r.conflicts)
    }

    #[test]
    fn trivial_cases() {
        assert_eq!(m("a\n", "a\n", "b\n"), ("b\n".into(), 0));
        assert_eq!(m("a\n", "b\n", "a\n"), ("b\n".into(), 0));
        assert_eq!(m("a\n", "c\n", "c\n"), ("c\n".into(), 0));
    }

    #[test]
    fn edits_in_different_places_merge_cleanly() {
        let base = "one\ntwo\nthree\nfour\nfive\n";
        let mine = "ONE\ntwo\nthree\nfour\nfive\n";
        let theirs = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(
            m(base, mine, theirs),
            ("ONE\ntwo\nthree\nfour\nFIVE\n".into(), 0)
        );
    }

    #[test]
    fn edits_on_adjacent_lines_merge_cleanly() {
        let base = "a\nb\nc\nd\n";
        let mine = "a\nB\nc\nd\n";
        let theirs = "a\nb\nC\nd\n";
        assert_eq!(m(base, mine, theirs), ("a\nB\nC\nd\n".into(), 0));
    }

    #[test]
    fn insert_and_delete_elsewhere() {
        let base = "# T\n\nkeep\ndrop me\nend\n";
        let mine = "# T\n\nnew line\nkeep\ndrop me\nend\n";
        let theirs = "# T\n\nkeep\nend\n";
        assert_eq!(
            m(base, mine, theirs),
            ("# T\n\nnew line\nkeep\nend\n".into(), 0)
        );
    }

    #[test]
    fn same_line_changed_both_ways_keeps_both() {
        let base = "x\nshared line\ny\n";
        let mine = "x\nmy version\ny\n";
        let theirs = "x\ntheir version\ny\n";
        assert_eq!(
            m(base, mine, theirs),
            ("x\nmy version\ntheir version\ny\n".into(), 1)
        );
    }

    #[test]
    fn both_append_at_end_keeps_both() {
        // The common case: a phone app appends a line while you type at the end.
        let base = "List\n- one\n";
        let mine = "List\n- one\n- typed here\n";
        let theirs = "List\n- one\n- from phone\n";
        assert_eq!(
            m(base, mine, theirs),
            ("List\n- one\n- typed here\n- from phone\n".into(), 1)
        );
    }

    #[test]
    fn missing_final_newline_does_not_glue_lines() {
        let base = "a\n";
        let mine = "a\nmine";
        let theirs = "a\ntheirs";
        assert_eq!(m(base, mine, theirs), ("a\nmine\ntheirs".into(), 1));
    }

    #[test]
    fn identical_changes_on_both_sides_apply_once() {
        let base = "a\nb\nc\n";
        let mine = "a\nB\nc\nmine\n";
        let theirs = "a\nB\nc\n";
        assert_eq!(m(base, mine, theirs), ("a\nB\nc\nmine\n".into(), 0));
    }

    #[test]
    fn crlf_lines_survive() {
        let base = "a\r\nb\r\nc\r\n";
        let mine = "A\r\nb\r\nc\r\n";
        let theirs = "a\r\nb\r\nC\r\n";
        assert_eq!(m(base, mine, theirs), ("A\r\nb\r\nC\r\n".into(), 0));
    }

    #[test]
    fn never_loses_text() {
        // Whatever the inputs, every line either side added appears in the result.
        let base = "1\n2\n3\n4\n5\n6\n";
        let cases = [
            ("1\nX\n3\n4\n5\n6\n", "1\n2\nY\n4\n5\n6\n"),
            ("1\n2\n3\n4\n5\n6\nM\n", "T\n1\n2\n3\n4\n5\n6\n"),
            ("1\nM1\nM2\n6\n", "1\n2\nT1\n5\n6\n"),
            ("", "1\n2\n3\nT\n4\n5\n6\n"),
        ];
        for (mine, theirs) in cases {
            let out = merge3(base, mine, theirs).text;
            for added in mine
                .lines()
                .chain(theirs.lines())
                .filter(|l| !base.lines().any(|b| b == *l))
            {
                assert!(
                    out.contains(added),
                    "lost {added:?} merging {mine:?} + {theirs:?} → {out:?}"
                );
            }
        }
    }

    #[test]
    fn merge2_keeps_both_sides() {
        let a = "# Title\nshared\nonly a\nend\n";
        let b = "# Title\nshared\nonly b\nend\nmore b\n";
        let m = merge2(a, b);
        assert_eq!(m.text, "# Title\nshared\nonly a\nonly b\nend\nmore b\n");
        assert_eq!(m.conflicts, 1);
        assert_eq!(merge2("x\n", "x\n").text, "x\n");
        assert_eq!(merge2("x", "x\ny").text, "x\ny");
        assert_eq!(merge2("x\ny", "x\nz").text, "x\ny\nz");
    }
}
