//! Rename with link rewriting.
//!
//! 1. Find every note that links into the renamed path, plus every note
//!    being moved (its own relative links may break).
//! 2. Snapshot each of their references with the file it resolves to *now*,
//!    before anything moves, so no guessing is needed afterwards.
//! 3. Rename on disk and update the index.
//! 4. For each snapshot reference whose target or source moved: if the link
//!    still reaches the same file and the file's name didn't change, leave it
//!    alone; otherwise write new link text (keeping `#heading`, `^block`,
//!    alias and embed `!`). Edits are spliced bottom-up per file.
//!
//! A rename touches many files, so it keeps a journal in
//! `.marrow/journal/rename.json`: written before the move, then filled in
//! with every note's new text before any note is rewritten. If Marrow stops
//! partway, [`Engine::recover`] finishes the job the next time the vault
//! opens. A note someone changed in the meantime is left alone.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Engine, EngineEvent};
use crate::cache::hash_bytes;
use crate::error::{IoContext, Result};
use crate::links::link_text;
use crate::parse::{parse, split_subpath, LinkKind, LinkRef, NoteMeta};
use crate::path::VaultPath;

/// What a rename changed besides the file itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameReport {
    /// Where the renamed file or folder now lives.
    pub new_path: VaultPath,
    pub links_updated: usize,
    pub files_updated: Vec<VaultPath>,
    /// Links left as they were because the note changed while renaming.
    #[serde(default)]
    pub links_skipped: usize,
}

enum RefSite {
    Body(LinkRef),
    /// A `[[link]]` string inside YAML frontmatter.
    Front {
        link: String,
    },
}

struct Snapshot {
    source: VaultPath,
    meta: Arc<NoteMeta>,
    refs: Vec<(RefSite, VaultPath)>,
}

impl Engine {
    /// Renames a file or folder and rewrites links to it across the vault.
    pub fn rename_with_links(
        &mut self,
        from: &VaultPath,
        to: &VaultPath,
    ) -> Result<(Vec<EngineEvent>, RenameReport)> {
        let moved = |p: &VaultPath| -> VaultPath {
            if p.starts_with(from) {
                VaultPath::new(&format!(
                    "{}{}",
                    to.as_str(),
                    &p.as_str()[from.as_str().len()..]
                ))
                .unwrap_or_else(|_| p.clone())
            } else {
                p.clone()
            }
        };

        // 1. Notes to look at.
        let mut sources: BTreeSet<VaultPath> = BTreeSet::new();
        for (src, targets) in &self.graph.resolved {
            if src.starts_with(from) || targets.keys().any(|t| t.starts_with(from)) {
                sources.insert(src.clone());
            }
        }

        // 2. Snapshot their references with today's resolution.
        let mut snaps = Vec::new();
        for src in sources {
            let text = self.vault.read_text(&src)?;
            // If the file changed on disk since we indexed it, parse what's
            // there now so the offsets we splice at are right.
            let fresh = self.records.get(&src).map(|r| r.hash.as_str())
                == Some(hash_bytes(text.as_bytes()).as_str());
            let meta = match (fresh, self.metas.get(&src)) {
                (true, Some(m)) => m.clone(),
                _ => Arc::new(parse(&text)),
            };
            let mut refs = Vec::new();
            for r in meta.links.iter().chain(meta.embeds.iter()) {
                if let Some(t) = self.names.resolve_first(&r.link, &src) {
                    if t.starts_with(from) || src.starts_with(from) {
                        refs.push((RefSite::Body(r.clone()), t));
                    }
                }
            }
            if let Some(fm) = &meta.frontmatter {
                for l in &fm.links {
                    if let Some(t) = self.names.resolve_first(&l.link, &src) {
                        if t.starts_with(from) || src.starts_with(from) {
                            refs.push((
                                RefSite::Front {
                                    link: l.link.clone(),
                                },
                                t,
                            ));
                        }
                    }
                }
            }
            if !refs.is_empty() {
                snaps.push(Snapshot {
                    source: src,
                    meta,
                    refs,
                });
            }
        }

        // 3. Rename and re-index, with the journal in place first.
        let journal = Journal { from: from.to_string(), to: to.to_string(), files: None };
        self.write_journal(&journal)?;
        let mut events = match self.rename(from, to) {
            Ok(ev) => ev,
            Err(e) => {
                self.clear_journal();
                return Err(e);
            }
        };

        // 4. Rewrite.
        let mut report = RenameReport {
            new_path: to.clone(),
            ..Default::default()
        };
        let mut rewrites: Vec<(VaultPath, String, String, usize)> = Vec::new();
        for snap in snaps {
            let source = moved(&snap.source);
            let text = self.vault.read_text(&source)?;
            let mut edits: Vec<(Range<usize>, String)> = Vec::new();
            for (site, old_target) in &snap.refs {
                let new_target = moved(old_target);
                let link = match site {
                    RefSite::Body(r) => r.link.as_str(),
                    RefSite::Front { link } => link.as_str(),
                };
                let (path_part, sub) = split_subpath(link);
                let still_reaches =
                    self.names.resolve_first(path_part, &source).as_ref() == Some(&new_target);
                let renamed = old_target.name() != new_target.name();
                if still_reaches && !renamed {
                    continue;
                }
                let new_text = link_text(&self.names, &new_target, &source, self.link_format);
                match site {
                    RefSite::Body(r) => {
                        // Another app may have edited the note since it was
                        // read: only replace the link if it's still exactly there.
                        if text.get(r.pos.range()) != Some(r.original.as_str()) {
                            report.links_skipped += 1;
                            continue;
                        }
                        let replacement = rewrite_body_link(r, &new_text, sub, &new_target);
                        if replacement != r.original {
                            edits.push((r.pos.range(), replacement));
                        }
                    }
                    RefSite::Front { link } => {
                        if let Some(fm) = &snap.meta.frontmatter {
                            edits.extend(frontmatter_edits(
                                &text,
                                fm.pos.range(),
                                link,
                                &format!("{new_text}{sub}"),
                            ));
                        }
                    }
                }
            }
            if edits.is_empty() {
                continue;
            }
            // Bottom-up so earlier offsets stay valid; drop overlaps.
            edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
            edits.dedup_by(|a, b| a.0.start == b.0.start);
            let mut out = text.clone();
            let mut floor = usize::MAX;
            let mut applied = 0;
            for (range, replacement) in edits {
                if range.end > floor || range.end > out.len() || !out.is_char_boundary(range.start)
                {
                    continue;
                }
                out.replace_range(range.clone(), &replacement);
                floor = range.start;
                applied += 1;
            }
            if out != text {
                rewrites.push((source, text, out, applied));
            }
        }

        // 5. Record every new text, then write them.
        if !rewrites.is_empty() {
            let files = rewrites
                .iter()
                .map(|(p, before, after, _)| JournalFile {
                    path: p.to_string(),
                    before: hash_bytes(before.as_bytes()),
                    after: after.clone(),
                })
                .collect();
            self.write_journal(&Journal { files: Some(files), ..journal })?;
        }
        for (source, _, out, applied) in rewrites {
            events.extend(self.write_note(&source, &out)?);
            report.links_updated += applied;
            report.files_updated.push(source);
        }
        self.clear_journal();
        Ok((events, report))
    }

    /// Finishes a rename that was interrupted (Marrow closed or crashed
    /// partway). Run when the vault opens. Returns what it did, in words.
    pub fn recover(&mut self) -> Vec<String> {
        let path = self.journal_path();
        let Ok(bytes) = std::fs::read(&path) else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        let Ok(j) = serde_json::from_slice::<Journal>(&bytes) else {
            // Unreadable: keep it aside for diagnosis, don't act on it.
            let _ = std::fs::rename(&path, path.with_extension("unreadable.json"));
            return vec!["An unfinished rename couldn't be read, so it was set aside.".into()];
        };
        let (Ok(from), Ok(to)) = (VaultPath::new(&j.from), VaultPath::new(&j.to)) else {
            self.clear_journal();
            return notes;
        };
        match j.files {
            None => {
                let at_from = from.to_os(self.vault.root()).exists();
                let at_to = to.to_os(self.vault.root()).exists();
                if at_to && !at_from {
                    // Moved, but no links were rewritten yet: move it back and
                    // do the whole rename again, links and all.
                    let redo = self
                        .rename(&to, &from)
                        .and_then(|_| self.rename_with_links(&from, &to));
                    match redo {
                        Ok((_, r)) => notes.push(format!(
                            "Finished renaming “{}” to “{}” ({} links updated).",
                            from.name(),
                            to.name(),
                            r.links_updated
                        )),
                        Err(e) => notes.push(format!("Couldn't finish renaming “{}”: {e}", from.name())),
                    }
                }
            }
            Some(files) => {
                let (mut done, mut skipped) = (0, Vec::new());
                for f in files {
                    let Ok(p) = VaultPath::new(&f.path) else { continue };
                    let Ok(now) = self.vault.read_text(&p) else {
                        skipped.push(f.path);
                        continue;
                    };
                    if now == f.after {
                        continue;
                    }
                    if hash_bytes(now.as_bytes()) == f.before && self.write_note(&p, &f.after).is_ok() {
                        done += 1;
                    } else {
                        // Changed since: leave the person's text alone.
                        skipped.push(f.path);
                    }
                }
                if done > 0 {
                    notes.push(format!("Finished updating links after renaming “{}” ({done} notes).", to.name()));
                }
                if !skipped.is_empty() {
                    notes.push(format!(
                        "After renaming “{}”, links in these notes may need a look: {}",
                        to.name(),
                        skipped.join(", ")
                    ));
                }
            }
        }
        self.clear_journal();
        notes
    }

    fn journal_path(&self) -> std::path::PathBuf {
        self.vault.root().join(".marrow").join("journal").join("rename.json")
    }

    fn write_journal(&self, j: &Journal) -> Result<()> {
        let p = self.journal_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).at(d)?;
        }
        let bytes = serde_json::to_vec(j).expect("journal serialises");
        crate::atomic::write(&p, &bytes).at(&p)
    }

    fn clear_journal(&self) {
        let _ = std::fs::remove_file(self.journal_path());
    }
}

/// A rename in progress. `files` is filled in once every new text is known.
#[derive(Serialize, Deserialize)]
struct Journal {
    from: String,
    to: String,
    files: Option<Vec<JournalFile>>,
}

#[derive(Serialize, Deserialize)]
struct JournalFile {
    path: String,
    /// Hash of the text the rewrite was made from.
    before: String,
    after: String,
}

/// Rebuilds one body link with new target text.
fn rewrite_body_link(r: &LinkRef, new_text: &str, sub: &str, target: &VaultPath) -> String {
    match r.kind {
        LinkKind::Wiki => {
            let embed = r.original.starts_with('!');
            let table_pipe = r.original.contains("\\|");
            let mut s = String::new();
            if embed {
                s.push('!');
            }
            s.push_str("[[");
            s.push_str(new_text);
            s.push_str(sub);
            if let Some(alias) = &r.display {
                s.push_str(if table_pipe { "\\|" } else { "|" });
                s.push_str(alias);
            }
            s.push_str("]]");
            s
        }
        LinkKind::Markdown => {
            // Keep `[label](` or `![alt](` as written; replace only the target.
            let Some(open) = r.original.rfind("](") else {
                return r.original.clone();
            };
            let mut dest = new_text.to_string();
            if target.is_markdown() && !dest.to_lowercase().ends_with(".md") {
                dest.push_str(".md");
            }
            let dest = dest.replace('%', "%25").replace(' ', "%20");
            let sub = sub.replace(' ', "%20");
            format!("{}{dest}{sub})", &r.original[..open + 2])
        }
    }
}

/// Edits for `[[link...` occurrences inside the frontmatter block.
fn frontmatter_edits(
    text: &str,
    fm: Range<usize>,
    link: &str,
    replacement: &str,
) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let Some(block) = text.get(fm.clone()) else {
        return out;
    };
    let needle = format!("[[{link}");
    let mut from = 0;
    while let Some(i) = block[from..].find(&needle) {
        let start = fm.start + from + i + 2;
        let end = start + link.len();
        let next = text[end..].chars().next();
        if matches!(next, Some(']') | Some('|') | Some('\\')) {
            out.push((start..end, replacement.to_string()));
        }
        from += i + needle.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::cache::Cache;
    use crate::engine::Engine;
    use crate::path::VaultPath;
    use std::fs;

    fn p(s: &str) -> VaultPath {
        VaultPath::new(s).unwrap()
    }

    fn setup(files: &[(&str, &str)]) -> (tempfile::TempDir, Engine) {
        let d = tempfile::tempdir().unwrap();
        for (path, body) in files {
            let full = d.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, body).unwrap();
        }
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        (d, e)
    }

    fn read(d: &tempfile::TempDir, path: &str) -> String {
        fs::read_to_string(d.path().join(path)).unwrap()
    }

    fn journal(d: &tempfile::TempDir) -> std::path::PathBuf {
        d.path().join(".marrow/journal/rename.json")
    }

    fn put_journal(d: &tempfile::TempDir, j: serde_json::Value) {
        fs::create_dir_all(journal(d).parent().unwrap()).unwrap();
        fs::write(journal(d), j.to_string()).unwrap();
    }

    fn reopen(d: &tempfile::TempDir) -> Engine {
        Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap()
    }

    #[test]
    fn a_finished_rename_leaves_no_journal() {
        let (d, mut e) = setup(&[("Old.md", "x"), ("A.md", "[[Old]]")]);
        e.rename_with_links(&p("Old.md"), &p("New.md")).unwrap();
        assert!(!journal(&d).exists());
        assert_eq!(read(&d, "A.md"), "[[New]]");
    }

    #[test]
    fn a_failed_rename_leaves_no_journal() {
        let (d, mut e) = setup(&[("Old.md", "x"), ("Taken.md", "y"), ("A.md", "[[Old]]")]);
        assert!(e.rename_with_links(&p("Old.md"), &p("Taken.md")).is_err());
        assert!(!journal(&d).exists());
        assert_eq!(read(&d, "A.md"), "[[Old]]");
    }

    /// Marrow stopped right after moving the file, before any link was fixed.
    #[test]
    fn recovers_a_rename_that_stopped_after_the_move() {
        let (d, e) = setup(&[("Old.md", "x"), ("A.md", "See [[Old]] and [[Old#Top|that]].")]);
        drop(e);
        fs::rename(d.path().join("Old.md"), d.path().join("New.md")).unwrap();
        put_journal(&d, serde_json::json!({"from": "Old.md", "to": "New.md", "files": null}));
        let mut e = reopen(&d);
        assert_eq!(read(&d, "A.md"), "See [[New]] and [[New#Top|that]].");
        assert!(d.path().join("New.md").exists() && !d.path().join("Old.md").exists());
        assert!(!journal(&d).exists());
        let said = e.take_recovered();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(e.take_recovered().is_empty());
    }

    /// Stopped before the move happened at all: nothing to do.
    #[test]
    fn a_rename_that_never_moved_is_dropped() {
        let (d, e) = setup(&[("Old.md", "x"), ("A.md", "[[Old]]")]);
        drop(e);
        put_journal(&d, serde_json::json!({"from": "Old.md", "to": "New.md", "files": null}));
        let mut e = reopen(&d);
        assert_eq!(read(&d, "A.md"), "[[Old]]");
        assert!(d.path().join("Old.md").exists());
        assert!(!journal(&d).exists());
        assert!(e.take_recovered().is_empty());
    }

    /// Stopped halfway through rewriting links: A was done, B wasn't, and
    /// C was edited by the person after the crash.
    #[test]
    fn finishes_rewrites_and_leaves_edited_notes_alone() {
        let (d, e) = setup(&[("New.md", "x"), ("A.md", "[[New]]"), ("B.md", "[[Old]] b"), ("C.md", "my new words [[Old]]")]);
        drop(e);
        let h = |t: &str| crate::cache::hash_bytes(t.as_bytes());
        put_journal(
            &d,
            serde_json::json!({"from": "Old.md", "to": "New.md", "files": [
                {"path": "A.md", "before": h("[[Old]]"), "after": "[[New]]"},
                {"path": "B.md", "before": h("[[Old]] b"), "after": "[[New]] b"},
                {"path": "C.md", "before": h("[[Old]] c"), "after": "[[New]] c"},
            ]}),
        );
        let mut e = reopen(&d);
        assert_eq!(read(&d, "A.md"), "[[New]]");
        assert_eq!(read(&d, "B.md"), "[[New]] b");
        assert_eq!(read(&d, "C.md"), "my new words [[Old]]");
        assert!(!journal(&d).exists());
        let said = e.take_recovered().join("\n");
        assert!(said.contains("1 notes") && said.contains("C.md"), "{said}");
        // The index follows the files.
        assert_eq!(e.backlinks(&p("New.md")).len(), 2);
    }

    #[test]
    fn an_unreadable_journal_is_set_aside() {
        let (d, e) = setup(&[("A.md", "x")]);
        drop(e);
        fs::create_dir_all(journal(&d).parent().unwrap()).unwrap();
        fs::write(journal(&d), "{ not json").unwrap();
        let mut e = reopen(&d);
        assert!(!journal(&d).exists());
        assert!(d.path().join(".marrow/journal/rename.unreadable.json").exists());
        assert_eq!(e.take_recovered().len(), 1);
    }

    #[test]
    fn renaming_a_note_rewrites_every_kind_of_link() {
        let (d, mut e) = setup(&[
            ("Old.md", "# Old\n"),
            (
                "A.md",
                "---\nrelated: \"[[Old]]\"\n---\nSee [[Old]], [[Old#Intro|the intro]], ![[Old]] and [md](Old.md).\n| t |\n|---|\n| [[Old\\|alias]] |\n",
            ),
            ("B.md", "Unrelated [[Other]].\n"),
        ]);
        let (_, report) = e
            .rename_with_links(&p("Old.md"), &p("New Name.md"))
            .unwrap();
        assert_eq!(
            read(&d, "A.md"),
            "---\nrelated: \"[[New Name]]\"\n---\nSee [[New Name]], [[New Name#Intro|the intro]], ![[New Name]] and [md](New%20Name.md).\n| t |\n|---|\n| [[New Name\\|alias]] |\n"
        );
        assert_eq!(report.files_updated, vec![p("A.md")]);
        assert_eq!(report.links_updated, 6);
        assert_eq!(read(&d, "B.md"), "Unrelated [[Other]].\n");
        assert_eq!(e.backlinks(&p("New Name.md"))[0].source, p("A.md"));
        assert!(e.unresolved(&p("A.md")).is_empty());
    }

    #[test]
    fn moving_a_folder_keeps_links_that_still_resolve() {
        let (d, mut e) = setup(&[
            ("people/Alice.md", "Hi [[Home]]\n"),
            ("Home.md", "[[Alice]] and [[people/Alice|full path]]\n"),
        ]);
        let (_, report) = e.rename_with_links(&p("people"), &p("friends")).unwrap();
        // The bare link still works; the path-qualified one is rewritten.
        assert_eq!(read(&d, "Home.md"), "[[Alice]] and [[Alice|full path]]\n");
        assert_eq!(report.links_updated, 1);
        assert_eq!(read(&d, "friends/Alice.md"), "Hi [[Home]]\n");
    }

    #[test]
    fn new_name_that_collides_gets_a_path() {
        let (d, mut e) = setup(&[
            ("Old.md", ""),
            ("B.md", "root B\n"),
            ("Src.md", "[[Old]]\n"),
        ]);
        e.rename_with_links(&p("Old.md"), &p("sub/B.md")).unwrap();
        // A bare [[B]] would reach the root B.md, so the vault path is used.
        assert_eq!(read(&d, "Src.md"), "[[sub/B]]\n");
        assert_eq!(e.resolve_link("sub/B", &p("Src.md")), Some(p("sub/B.md")));
    }

    #[test]
    fn moved_note_fixes_its_own_relative_links() {
        let (d, mut e) = setup(&[
            ("a/Sib.md", ""),
            ("a/b/Src.md", "[[../Sib]]\n"),
            ("a/b/Other.md", ""),
        ]);
        e.rename_with_links(&p("a/b/Src.md"), &p("Src.md")).unwrap();
        assert_eq!(read(&d, "Src.md"), "[[Sib]]\n");
    }

    #[test]
    fn links_inside_code_are_left_alone() {
        let (d, mut e) = setup(&[
            ("Old.md", ""),
            ("A.md", "[[Old]] `[[Old]]`\n```\n[[Old]]\n```\n"),
        ]);
        e.rename_with_links(&p("Old.md"), &p("New.md")).unwrap();
        assert_eq!(read(&d, "A.md"), "[[New]] `[[Old]]`\n```\n[[Old]]\n```\n");
    }

    #[test]
    fn self_links_follow_the_note() {
        let (d, mut e) = setup(&[("Old.md", "See [[Old#Top]]\n")]);
        e.rename_with_links(&p("Old.md"), &p("New.md")).unwrap();
        assert_eq!(read(&d, "New.md"), "See [[New#Top]]\n");
    }

    #[test]
    fn case_only_rename_updates_link_case() {
        let (d, mut e) = setup(&[("idea.md", ""), ("A.md", "[[idea]]\n")]);
        e.rename_with_links(&p("idea.md"), &p("Idea.md")).unwrap();
        assert_eq!(read(&d, "A.md"), "[[Idea]]\n");
    }
}
