//! Link resolution, the link graph and backlinks.
//!
//! Resolution follows the order described in the design doc:
//!
//! 1. Empty target → the source note itself (`[[#Heading]]`).
//! 2. Look the target's file name up in a case-insensitive filename index,
//!    first as written (if it has an extension), then with `.md` added.
//! 3. A bare name with exactly one match wins.
//! 4. `./` and `../` targets resolve against the source note's folder.
//! 5. A target that equals a candidate's full vault path wins.
//! 6. A target that starts with `/` and didn't match stops here.
//! 7. Otherwise candidates whose path ends with the target, those inside the
//!    source note's folder first, each group shortest path first.
//!
//! One deliberate difference: step 7 matches whole path segments, so
//! `[[x/b]]` does not match `ax/b.md`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::parse::{split_subpath, LinkRef, NoteMeta};
use crate::path::VaultPath;

/// Lower-cased file name (with extension) → files with that name.
#[derive(Default)]
pub struct FilenameIndex {
    by_name: HashMap<String, Vec<VaultPath>>,
}

impl FilenameIndex {
    pub fn add(&mut self, path: &VaultPath) {
        let v = self.by_name.entry(path.name().to_lowercase()).or_default();
        if !v.contains(path) {
            v.push(path.clone());
        }
    }

    pub fn remove(&mut self, path: &VaultPath) {
        let key = path.name().to_lowercase();
        if let Some(v) = self.by_name.get_mut(&key) {
            v.retain(|p| p != path);
            if v.is_empty() {
                self.by_name.remove(&key);
            }
        }
    }

    pub fn get(&self, lower_name: &str) -> &[VaultPath] {
        self.by_name
            .get(lower_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn contains(&self, path: &VaultPath) -> bool {
        self.get(&path.name().to_lowercase()).contains(path)
    }

    /// Resolves a link target (subpath already removed) written in `source`.
    /// Returns every match, best first.
    pub fn resolve(&self, target: &str, source: &VaultPath) -> Vec<VaultPath> {
        if target.is_empty() {
            return if self.contains(source) {
                vec![source.clone()]
            } else {
                vec![]
            };
        }
        let mut wanted = target.to_lowercase().replace('\\', "/");
        let mut name = last_segment(&wanted).to_string();
        let mut cands: &[VaultPath] = &[];
        if name.contains('.') {
            cands = self.get(&name);
        }
        if cands.is_empty() {
            wanted = format!("{}.md", target.to_lowercase().replace('\\', "/"));
            name = last_segment(&wanted).to_string();
            cands = self.get(&name);
        }
        if cands.is_empty() {
            return vec![];
        }
        if name == wanted && cands.len() == 1 {
            return cands.to_vec();
        }

        let source_dir = source.parent().as_str().to_lowercase();
        let lower = |p: &VaultPath| p.as_str().to_lowercase();

        if wanted.starts_with("./") || wanted.starts_with("../") {
            let mut rel = wanted.as_str();
            if let Some(r) = rel.strip_prefix("./") {
                if r.starts_with("../") {
                    rel = r;
                }
            }
            let mut dir = source_dir.clone();
            let full = if let Some(r) = rel.strip_prefix("./") {
                join(&dir, r)
            } else {
                while let Some(r) = rel.strip_prefix("../") {
                    rel = r;
                    dir = parent_str(&dir).to_string();
                }
                join(&dir, rel)
            };
            if let Some(hit) = cands.iter().find(|c| lower(c) == full) {
                return vec![hit.clone()];
            }
        }

        let absolute = wanted.trim_start_matches('/');
        if let Some(hit) = cands.iter().find(|c| lower(c) == absolute) {
            return vec![hit.clone()];
        }
        if target.starts_with('/') {
            return vec![];
        }

        let (mut near, mut far): (Vec<&VaultPath>, Vec<&VaultPath>) = cands
            .iter()
            .filter(|c| ends_with_segments(&lower(c), absolute))
            .partition(|c| {
                source_dir.is_empty() || lower(c).starts_with(&format!("{source_dir}/"))
            });
        near.sort_by_key(|p| p.as_str().len());
        far.sort_by_key(|p| p.as_str().len());
        near.into_iter().chain(far).cloned().collect()
    }

    pub fn resolve_first(&self, link: &str, source: &VaultPath) -> Option<VaultPath> {
        self.resolve(split_subpath(link).0, source)
            .into_iter()
            .next()
    }
}

fn last_segment(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

fn parent_str(s: &str) -> &str {
    match s.rfind('/') {
        Some(i) => &s[..i],
        None => "",
    }
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

fn ends_with_segments(path: &str, suffix: &str) -> bool {
    path == suffix
        || (path.ends_with(suffix) && path.as_bytes()[path.len() - suffix.len() - 1] == b'/')
}

/// How new links are written (mirrors the "new link format" setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkFormat {
    /// Bare name when unique, else the vault path.
    #[default]
    Shortest,
    /// `../` steps from the source note's folder.
    Relative,
    /// Always the vault path.
    Absolute,
}

/// The text to put inside `[[...]]` to reach `target` from `source`.
pub fn link_text(
    index: &FilenameIndex,
    target: &VaultPath,
    source: &VaultPath,
    format: LinkFormat,
) -> String {
    let full = target.without_md().to_string();
    match format {
        LinkFormat::Absolute => full,
        LinkFormat::Relative => {
            let mut up = String::new();
            let mut dir = source.parent();
            while !dir.is_root() && !target.starts_with(&dir) {
                up.push_str("../");
                dir = dir.parent();
            }
            let rest = if dir.is_root() {
                full.as_str()
            } else {
                &full[dir.as_str().len() + 1..]
            };
            format!("{up}{rest}")
        }
        LinkFormat::Shortest => {
            let short = if target.is_markdown() {
                target.stem()
            } else {
                target.name()
            };
            let back = index.resolve(short, source);
            if back.len() == 1 && &back[0] == target {
                short.to_string()
            } else {
                full
            }
        }
    }
}

// ------------------------------------------------------------ link graph

/// Outgoing links per note, as resolved paths and unresolved link texts.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct LinkGraph {
    pub resolved: HashMap<VaultPath, BTreeMap<VaultPath, u32>>,
    pub unresolved: HashMap<VaultPath, BTreeMap<String, u32>>,
}

impl LinkGraph {
    /// Recomputes `source`'s outgoing links from its metadata.
    pub fn update_note(&mut self, index: &FilenameIndex, source: &VaultPath, meta: &NoteMeta) {
        let mut res: BTreeMap<VaultPath, u32> = BTreeMap::new();
        let mut unres: BTreeMap<String, u32> = BTreeMap::new();
        let fm_links = meta
            .frontmatter
            .iter()
            .flat_map(|f| f.links.iter().map(|l| l.link.as_str()));
        let all = meta.references().map(|r| r.link.as_str()).chain(fm_links);
        for link in all {
            let target = split_subpath(link).0;
            match index.resolve(target, source).into_iter().next() {
                Some(dest) => *res.entry(dest).or_default() += 1,
                None => {
                    let key = target.strip_suffix(".md").unwrap_or(target).to_string();
                    *unres.entry(key).or_default() += 1;
                }
            }
        }
        self.resolved.insert(source.clone(), res);
        self.unresolved.insert(source.clone(), unres);
    }

    pub fn remove_note(&mut self, source: &VaultPath) {
        self.resolved.remove(source);
        self.unresolved.remove(source);
    }

    /// Moves a note's own outgoing entry to its new path. Targets inside
    /// other notes are fixed by re-resolving those notes.
    pub fn rename_note(&mut self, from: &VaultPath, to: &VaultPath) {
        if let Some(v) = self.resolved.remove(from) {
            self.resolved.insert(to.clone(), v);
        }
        if let Some(v) = self.unresolved.remove(from) {
            self.unresolved.insert(to.clone(), v);
        }
    }

    /// Notes that link to `target` (resolved), with counts.
    pub fn linking_notes(&self, target: &VaultPath) -> Vec<(VaultPath, u32)> {
        let mut out: Vec<_> = self
            .resolved
            .iter()
            .filter_map(|(src, m)| m.get(target).map(|n| (src.clone(), *n)))
            .collect();
        out.sort();
        out
    }

    /// Notes whose links could change meaning when files with these names
    /// appear, vanish or move. Names are matched case-insensitively, with
    /// or without `.md`.
    pub fn affected_by_names(&self, names: &[&str]) -> Vec<VaultPath> {
        let mut keys: HashSet<String> = HashSet::new();
        for n in names {
            let n = n.to_lowercase();
            if let Some(stem) = n.strip_suffix(".md") {
                keys.insert(stem.to_string());
            }
            keys.insert(n);
        }
        let mut out: HashSet<VaultPath> = HashSet::new();
        for (src, m) in &self.resolved {
            if m.keys().any(|t| keys.contains(&t.name().to_lowercase())) {
                out.insert(src.clone());
            }
        }
        for (src, m) in &self.unresolved {
            if m.keys()
                .any(|t| keys.contains(last_segment(&t.to_lowercase())))
            {
                out.insert(src.clone());
            }
        }
        let mut v: Vec<_> = out.into_iter().collect();
        v.sort();
        v
    }

    /// All unresolved link texts across the vault with total counts.
    pub fn all_unresolved(&self) -> BTreeMap<String, u32> {
        let mut out = BTreeMap::new();
        for m in self.unresolved.values() {
            for (k, n) in m {
                *out.entry(k.clone()).or_default() += n;
            }
        }
        out
    }
}

/// One backlink: a note and the references in it that reach the target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backlink {
    pub source: VaultPath,
    pub refs: Vec<LinkRef>,
}

/// Computes backlinks on demand. Uses the graph to find candidate sources,
/// then re-resolves each reference so the exact positions are returned.
pub fn backlinks(
    graph: &LinkGraph,
    index: &FilenameIndex,
    meta_of: impl Fn(&VaultPath) -> Option<std::sync::Arc<NoteMeta>>,
    target: &VaultPath,
) -> Vec<Backlink> {
    let mut out = Vec::new();
    for (src, _) in graph.linking_notes(target) {
        let Some(meta) = meta_of(&src) else { continue };
        let refs: Vec<LinkRef> = meta
            .references()
            .filter(|r| index.resolve_first(&r.link, &src).as_ref() == Some(target))
            .cloned()
            .collect();
        if !refs.is_empty() {
            out.push(Backlink { source: src, refs });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> VaultPath {
        VaultPath::new(s).unwrap()
    }

    fn index(paths: &[&str]) -> FilenameIndex {
        let mut ix = FilenameIndex::default();
        for s in paths {
            ix.add(&p(s));
        }
        ix
    }

    fn r(ix: &FilenameIndex, link: &str, src: &str) -> Vec<String> {
        ix.resolve(link, &p(src))
            .into_iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    #[test]
    fn empty_target_is_self() {
        let ix = index(&["a.md"]);
        assert_eq!(r(&ix, "", "a.md"), vec!["a.md"]);
    }

    #[test]
    fn unique_bare_name_case_insensitive() {
        let ix = index(&["Projects/Idea.md", "img/Photo.PNG"]);
        assert_eq!(r(&ix, "idea", "x.md"), vec!["Projects/Idea.md"]);
        assert_eq!(r(&ix, "Idea.md", "x.md"), vec!["Projects/Idea.md"]);
        assert_eq!(r(&ix, "photo.png", "x.md"), vec!["img/Photo.PNG"]);
        assert!(r(&ix, "Missing", "x.md").is_empty());
    }

    #[test]
    fn dotted_note_names_fall_back_to_md() {
        let ix = index(&["v1.2 notes.md"]);
        assert_eq!(r(&ix, "v1.2 notes", "x.md"), vec!["v1.2 notes.md"]);
    }

    #[test]
    fn duplicates_prefer_same_folder_then_shortest() {
        let ix = index(&["a/Note.md", "b/c/Note.md", "c/d/Note.md", "b/Note.md"]);
        assert_eq!(
            r(&ix, "Note", "b/src.md"),
            vec!["b/Note.md", "b/c/Note.md", "a/Note.md", "c/d/Note.md"]
        );
        // A note at the vault root matches the bare name as a full path, so it
        // wins from anywhere (same as Obsidian).
        let ix = index(&["b/Note.md", "Note.md"]);
        assert_eq!(r(&ix, "Note", "b/src.md"), vec!["Note.md"]);
    }

    #[test]
    fn full_path_and_partial_path() {
        let ix = index(&["a/Note.md", "b/Note.md", "x/b/Note.md"]);
        assert_eq!(r(&ix, "a/Note", "z.md"), vec!["a/Note.md"]);
        assert_eq!(r(&ix, "/b/Note", "z.md"), vec!["b/Note.md"]);
        assert_eq!(r(&ix, "/c/Note", "z.md"), Vec::<String>::new());
        // Segment-aware suffix: "b/Note" matches b/Note.md and x/b/Note.md, never "ab/Note.md".
        assert_eq!(r(&ix, "b/note", "z.md"), vec!["b/Note.md"]); // exact path wins
        let ix2 = index(&["ab/Note.md", "x/b/Note.md", "y/Note.md"]);
        assert_eq!(r(&ix2, "b/Note", "z.md"), vec!["x/b/Note.md"]);
    }

    #[test]
    fn relative_targets() {
        let ix = index(&["a/b/Note.md", "a/Note.md", "Note.md"]);
        assert_eq!(r(&ix, "./Note", "a/b/src.md"), vec!["a/b/Note.md"]);
        assert_eq!(r(&ix, "../Note", "a/b/src.md"), vec!["a/Note.md"]);
        assert_eq!(r(&ix, "../../Note", "a/b/src.md"), vec!["Note.md"]);
        assert_eq!(r(&ix, "./../Note", "a/b/src.md"), vec!["a/Note.md"]);
    }

    #[test]
    fn link_text_formats() {
        let ix = index(&["a/Note.md", "b/Note.md", "a/Unique.md", "img/p.png"]);
        let s = p("a/x/src.md");
        assert_eq!(
            link_text(&ix, &p("a/Unique.md"), &s, LinkFormat::Shortest),
            "Unique"
        );
        assert_eq!(
            link_text(&ix, &p("b/Note.md"), &s, LinkFormat::Shortest),
            "b/Note"
        );
        assert_eq!(
            link_text(&ix, &p("img/p.png"), &s, LinkFormat::Shortest),
            "p.png"
        );
        assert_eq!(
            link_text(&ix, &p("b/Note.md"), &s, LinkFormat::Relative),
            "../../b/Note"
        );
        assert_eq!(
            link_text(&ix, &p("a/Note.md"), &s, LinkFormat::Relative),
            "../Note"
        );
        assert_eq!(
            link_text(&ix, &p("a/Note.md"), &s, LinkFormat::Absolute),
            "a/Note"
        );
    }

    #[test]
    fn graph_resolved_unresolved_and_affected() {
        let mut ix = index(&["A.md", "B.md"]);
        let mut g = LinkGraph::default();
        let meta = crate::parse::parse("[[B]] [[B#h]] [[C]] ![[c.md]]\n");
        g.update_note(&ix, &p("A.md"), &meta);
        assert_eq!(g.resolved[&p("A.md")][&p("B.md")], 2);
        assert_eq!(g.unresolved[&p("A.md")]["C"], 1);
        assert_eq!(g.unresolved[&p("A.md")]["c"], 1);
        assert_eq!(g.affected_by_names(&["C.md"]), vec![p("A.md")]);
        assert_eq!(g.affected_by_names(&["B.md"]), vec![p("A.md")]);
        assert!(g.affected_by_names(&["Z.md"]).is_empty());

        ix.add(&p("C.md"));
        g.update_note(&ix, &p("A.md"), &meta);
        assert_eq!(g.resolved[&p("A.md")][&p("C.md")], 2);
        assert!(g.unresolved[&p("A.md")].is_empty());
    }

    #[test]
    fn backlinks_return_positions() {
        let ix = index(&["A.md", "B.md", "T.md"]);
        let mut g = LinkGraph::default();
        let ma = std::sync::Arc::new(crate::parse::parse("x [[T]] y [[T|alias]] [[B]]\n"));
        let mb = std::sync::Arc::new(crate::parse::parse("![[T]]\n"));
        g.update_note(&ix, &p("A.md"), &ma);
        g.update_note(&ix, &p("B.md"), &mb);
        let metas: HashMap<VaultPath, std::sync::Arc<NoteMeta>> =
            [(p("A.md"), ma), (p("B.md"), mb)].into_iter().collect();
        let bl = backlinks(&g, &ix, |s| metas.get(s).cloned(), &p("T.md"));
        assert_eq!(bl.len(), 2);
        assert_eq!(bl[0].source, p("A.md"));
        assert_eq!(bl[0].refs.len(), 2);
        assert_eq!(bl[0].refs[1].display.as_deref(), Some("alias"));
        assert_eq!(bl[1].refs[0].original, "![[T]]");
    }
}
