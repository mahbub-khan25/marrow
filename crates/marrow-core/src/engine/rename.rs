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

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Engine, EngineEvent};
use crate::cache::hash_bytes;
use crate::error::Result;
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

        // 3. Rename and re-index.
        let mut events = self.rename(from, to)?;

        // 4. Rewrite.
        let mut report = RenameReport {
            new_path: to.clone(),
            ..Default::default()
        };
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
                events.extend(self.write_note(&source, &out)?);
                report.links_updated += applied;
                report.files_updated.push(source);
            }
        }
        Ok((events, report))
    }
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
