//! The engine: one owner for the vault, the cache, the metadata index and
//! the link graph.
//!
//! Everything that changes state goes through `&mut Engine`, so the caller
//! (the Tauri shell) only has to make sure one thread at a time holds it.
//! [`crate::service::VaultService`] does exactly that and feeds it watcher
//! events.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::{hash_bytes, Cache, CacheOp, FileRecord};
use crate::error::{Error, Result};
use crate::links::{backlinks, link_text, Backlink, FilenameIndex, LinkFormat, LinkGraph};
use crate::parse::{parse, NoteMeta};
use crate::path::VaultPath;
use crate::vault::{Entry, EntryKind, Vault, VaultEvent};

mod graph;
mod rename;
mod mentions;
pub mod recall;
mod search;
mod tasks;

pub use graph::{GraphData, GraphEdge, GraphNode, GraphNodeKind, GraphOptions};
pub use rename::RenameReport;
pub use mentions::Mention;
pub use tasks::{remind_of, set_remind, task_text_of, TaskItem, REMIND};
pub use search::{fold as fold_for_search, Query, SearchHit, SearchResults, Snippet};

/// What the UI needs to hear about after any change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EngineEvent {
    /// A file or folder appeared, changed, vanished or moved.
    Vault { event: VaultEvent },
    /// A note's metadata was recomputed.
    MetadataChanged { path: VaultPath },
    /// These notes' outgoing links were re-resolved; backlinks of anything
    /// they point to may have changed.
    LinksChanged { paths: Vec<VaultPath> },
}

/// Counts from the last indexing pass, to show the cache doing its job.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStats {
    /// mtime and size matched the cache: not even read.
    pub unchanged: usize,
    /// Read and hashed, but the hash was already known.
    pub reused: usize,
    /// Actually parsed.
    pub parsed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteInfo {
    /// Body and frontmatter tags, with `#`, in first-seen order.
    pub tags: Vec<String>,
    pub modified_ms: i64,
    /// Notes that link here.
    pub backlinks: usize,
}

/// One note as a database row: its properties, and what it links to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRow {
    pub path: String,
    /// The file name without `.md`.
    pub title: String,
    /// Frontmatter as JSON (an empty object if there is none).
    pub props: serde_json::Value,
    pub tags: Vec<String>,
    pub modified_ms: i64,
    pub created_ms: i64,
    /// For each property holding `[[links]]`, the notes they point to.
    pub prop_links: BTreeMap<String, Vec<String>>,
    /// Notes that link here.
    pub backlinks: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// The notes in the folder.
    pub rows: Vec<NoteRow>,
    /// Notes they point to through properties, or that link to them, for
    /// relations and rollups.
    pub related: Vec<NoteRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VaultStats {
    pub notes: usize,
    pub links: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub path: VaultPath,
    pub name: String,
    pub kind: EntryKind,
    pub children: Vec<TreeNode>,
}

pub struct Engine {
    vault: Vault,
    cache: Cache,
    names: FilenameIndex,
    records: HashMap<VaultPath, FileRecord>,
    metas: HashMap<VaultPath, Arc<NoteMeta>>,
    graph: LinkGraph,
    pub link_format: LinkFormat,
    stats: IndexStats,
    /// Note text for search, loaded on first search, dropped on change.
    texts: search::TextCache,
    /// What opening the vault had to repair, in words, for the window to show.
    recovered: Vec<String>,
}

struct Indexed {
    path: VaultPath,
    record: FileRecord,
    meta: Arc<NoteMeta>,
    parsed: bool,
    from_cache_stat: bool,
}

impl Engine {
    /// Opens a vault, indexes it (reusing the cache) and resolves all links.
    pub fn open(root: &Path, cache: Cache) -> Result<Self> {
        let (vault, _) = Vault::open(root)?;
        let mut e = Engine {
            vault,
            cache,
            names: FilenameIndex::default(),
            records: HashMap::new(),
            metas: HashMap::new(),
            graph: LinkGraph::default(),
            link_format: LinkFormat::default(),
            stats: IndexStats::default(),
            texts: Default::default(),
            recovered: Vec::new(),
        };
        for p in e.vault.files().cloned().collect::<Vec<_>>() {
            e.names.add(&p);
        }
        let notes: Vec<VaultPath> = e.vault.markdown_files().cloned().collect();
        e.index_many(&notes)?;

        // Forget cache entries for files that are gone.
        let gone: Vec<CacheOp> = e
            .cache
            .all_files()?
            .into_iter()
            .filter(|(p, _)| {
                VaultPath::new(p)
                    .map(|vp| !e.records.contains_key(&vp))
                    .unwrap_or(true)
            })
            .map(|(p, _)| CacheOp::DeleteFile(p))
            .collect();
        e.cache.apply(gone)?;
        e.cache.collect_garbage()?;

        for p in notes {
            e.resolve_note(&p);
        }

        // Tidy up after a crash: half-made temporary files, unfinished renames.
        let tidy = root.to_path_buf();
        let _ = std::thread::Builder::new()
            .name("marrow-tidy".into())
            .spawn(move || crate::atomic::clean_up(&tidy));
        e.recovered = e.recover();
        Ok(e)
    }

    /// What opening the vault repaired (an interrupted rename, say). Empty
    /// almost always. Each call hands the list over once.
    pub fn take_recovered(&mut self) -> Vec<String> {
        std::mem::take(&mut self.recovered)
    }

    /// Drops cached headings, links and properties of file versions that no
    /// longer exist (after locking a note, so its readable metadata goes).
    pub fn forget_old_metadata(&mut self) -> Result<usize> {
        self.cache.collect_garbage()
    }

    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    pub fn stats(&self) -> IndexStats {
        self.stats
    }

    // ------------------------------------------------------------ indexing

    /// Brings metadata for `paths` up to date, in parallel. Returns the
    /// paths whose metadata actually changed.
    fn index_many(&mut self, paths: &[VaultPath]) -> Result<Vec<VaultPath>> {
        let vault = &self.vault;
        let cache = &self.cache;
        let known = &self.records;
        let results: Vec<Result<Option<Indexed>>> = paths
            .par_iter()
            .map(|p| index_one(vault, cache, known.get(p), p))
            .collect();

        let mut stats = IndexStats::default();
        let mut ops = Vec::new();
        let mut changed = Vec::new();
        for r in results {
            let Some(ix) = r? else { continue };
            if ix.from_cache_stat {
                stats.unchanged += 1;
            } else if ix.parsed {
                stats.parsed += 1;
            } else {
                stats.reused += 1;
            }
            if ix.parsed {
                ops.push(CacheOp::PutMeta(ix.record.hash.clone(), (*ix.meta).clone()));
            }
            let prev_hash = self.records.get(&ix.path).map(|r| r.hash.clone());
            if prev_hash.as_deref() != Some(ix.record.hash.as_str())
                || !self.metas.contains_key(&ix.path)
            {
                changed.push(ix.path.clone());
            }
            ops.push(CacheOp::PutFile(
                ix.path.as_str().to_string(),
                ix.record.clone(),
            ));
            self.records.insert(ix.path.clone(), ix.record);
            self.metas.insert(ix.path, ix.meta);
        }
        self.cache.apply(ops)?;
        self.stats = stats;
        Ok(changed)
    }

    fn resolve_note(&mut self, p: &VaultPath) {
        if let Some(m) = self.metas.get(p).cloned() {
            self.graph.update_note(&self.names, p, &m);
        }
    }

    fn forget(&mut self, p: &VaultPath) -> Vec<CacheOp> {
        self.names.remove(p);
        self.records.remove(p);
        self.metas.remove(p);
        self.graph.remove_note(p);
        vec![CacheOp::DeleteFile(p.as_str().to_string())]
    }

    // ------------------------------------------------------ applying events

    /// Updates the index for a batch of vault changes and returns everything
    /// the UI should hear about.
    pub fn apply(&mut self, events: Vec<VaultEvent>) -> Result<Vec<EngineEvent>> {
        let mut out: Vec<EngineEvent> = Vec::new();
        let mut to_index: BTreeSet<VaultPath> = BTreeSet::new();
        let mut names: BTreeSet<String> = BTreeSet::new();
        let mut ops = Vec::new();

        for ev in &events {
            // Search text is reloaded lazily for anything that changed.
            match ev {
                VaultEvent::Created { path, .. }
                | VaultEvent::Modified { path }
                | VaultEvent::Deleted { path, .. } => self.forget_text(path),
                VaultEvent::Renamed { from, to, .. } => {
                    self.forget_text(from);
                    self.forget_text(to);
                }
            }
            match ev {
                VaultEvent::Created {
                    path,
                    kind: EntryKind::File,
                } => {
                    self.names.add(path);
                    names.insert(path.name().to_string());
                    if path.is_markdown() {
                        to_index.insert(path.clone());
                    }
                }
                VaultEvent::Modified { path } => {
                    if path.is_markdown() {
                        to_index.insert(path.clone());
                    }
                }
                VaultEvent::Deleted {
                    path,
                    kind: EntryKind::File,
                } => {
                    names.insert(path.name().to_string());
                    to_index.remove(path);
                    ops.extend(self.forget(path));
                }
                VaultEvent::Renamed {
                    from,
                    to,
                    kind: EntryKind::File,
                } => {
                    self.names.remove(from);
                    self.names.add(to);
                    names.insert(from.name().to_string());
                    names.insert(to.name().to_string());
                    if let Some(r) = self.records.remove(from) {
                        ops.push(CacheOp::DeleteFile(from.as_str().to_string()));
                        if to.is_markdown() {
                            ops.push(CacheOp::PutFile(to.as_str().to_string(), r.clone()));
                            self.records.insert(to.clone(), r);
                        }
                    }
                    if let Some(m) = self.metas.remove(from) {
                        if to.is_markdown() {
                            self.metas.insert(to.clone(), m);
                        }
                    }
                    self.graph.rename_note(from, to);
                    if to.is_markdown() {
                        // Its own relative links may now point elsewhere.
                        to_index.insert(to.clone());
                    } else {
                        self.graph.remove_note(to);
                    }
                }
                _ => {} // folder events need no index work
            }
            out.push(EngineEvent::Vault { event: ev.clone() });
        }
        self.cache.apply(ops)?;

        let list: Vec<VaultPath> = to_index.iter().cloned().collect();
        let changed = self.index_many(&list)?;
        for p in &changed {
            out.push(EngineEvent::MetadataChanged { path: p.clone() });
        }

        // Re-resolve: the notes we just indexed, plus any note whose links
        // mention a name that appeared, vanished or moved.
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut relink: BTreeSet<VaultPath> = to_index.into_iter().collect();
        if !name_refs.is_empty() {
            relink.extend(self.graph.affected_by_names(&name_refs));
        }
        relink.retain(|p| self.metas.contains_key(p));
        for p in &relink {
            self.resolve_note(p);
        }
        if !relink.is_empty() {
            out.push(EngineEvent::LinksChanged {
                paths: relink.into_iter().collect(),
            });
        }
        Ok(out)
    }

    /// Feed from the file watcher: reconcile each path against disk, then
    /// update the index.
    pub fn on_paths_changed(&mut self, paths: &[VaultPath]) -> Result<Vec<EngineEvent>> {
        let mut events = Vec::new();
        for p in paths {
            events.extend(self.vault.reconcile(p)?);
        }
        self.apply(events)
    }

    /// Full re-check, e.g. after the watcher overflowed.
    pub fn rescan(&mut self) -> Result<Vec<EngineEvent>> {
        let events = self.vault.rescan()?;
        self.apply(events)
    }

    // ------------------------------------------------------------ file ops

    pub fn read_note(&self, path: &VaultPath) -> Result<String> {
        match self.vault.get(path) {
            Some(Entry::File(_)) => self.vault.read_text(path),
            _ => Err(Error::NotFound(path.to_string())),
        }
    }

    pub fn write_note(&mut self, path: &VaultPath, text: &str) -> Result<Vec<EngineEvent>> {
        let ev = self.vault.write(path, text)?;
        self.apply(ev)
    }

    /// Writes `text` only if the file still holds the version with hash
    /// `expected` (the one the new text was made from). `None` means someone
    /// changed it meanwhile, and it was left alone, as an ordinary save would.
    pub fn write_note_if_unchanged(&mut self, path: &VaultPath, expected: &str, text: &str) -> Result<Option<Vec<EngineEvent>>> {
        let now = self.vault.read_text(path)?;
        if crate::cache::hash_bytes(now.as_bytes()) != expected {
            return Ok(None);
        }
        self.write_note(path, text).map(Some)
    }

    pub fn create_note(&mut self, path: &VaultPath, text: &str) -> Result<Vec<EngineEvent>> {
        let ev = self.vault.create(path, text)?;
        self.apply(ev)
    }

    pub fn create_folder(&mut self, path: &VaultPath) -> Result<Vec<EngineEvent>> {
        let ev = self.vault.create_folder(path)?;
        self.apply(ev)
    }

    /// Renames a file or folder. Rewriting links in other notes is Phase 6.
    pub fn rename(&mut self, from: &VaultPath, to: &VaultPath) -> Result<Vec<EngineEvent>> {
        let ev = self.vault.rename(from, to)?;
        self.apply(ev)
    }

    pub fn trash(&mut self, path: &VaultPath) -> Result<Vec<EngineEvent>> {
        let ev = self.vault.trash(path)?;
        self.apply(ev)
    }

    // -------------------------------------------------------------- queries

    pub fn meta(&self, path: &VaultPath) -> Option<Arc<NoteMeta>> {
        self.metas.get(path).cloned()
    }

    /// Parses unsaved editor text without touching the index.
    pub fn preview_meta(&self, text: &str) -> NoteMeta {
        parse(text)
    }

    pub fn resolve_link(&self, link: &str, source: &VaultPath) -> Option<VaultPath> {
        self.names.resolve_first(link, source)
    }

    pub fn backlinks(&self, target: &VaultPath) -> Vec<Backlink> {
        backlinks(
            &self.graph,
            &self.names,
            |p| self.metas.get(p).cloned(),
            target,
        )
    }

    fn note_row(&self, path: &VaultPath) -> Option<NoteRow> {
        let meta = self.metas.get(path)?;
        let stat = self.vault.stat(path)?;
        let props = meta
            .frontmatter
            .as_ref()
            .map(|f| f.data.clone())
            .filter(|d| d.is_object())
            .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
        let mut prop_links: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if let Some(fm) = &meta.frontmatter {
            for l in &fm.links {
                // "project.parent" belongs to "project".
                let key = l.key.split('.').next().unwrap_or(&l.key).to_string();
                if let Some(t) = self.resolve_link(&l.link, path) {
                    let v = prop_links.entry(key).or_default();
                    if !v.contains(&t.to_string()) {
                        v.push(t.to_string());
                    }
                }
            }
        }
        let mut tags: Vec<String> = meta.all_tags().into_iter().map(|t| format!("#{}", t.trim_start_matches('#'))).collect();
        let mut seen = BTreeSet::new();
        tags.retain(|t| seen.insert(t.to_lowercase()));
        let backlinks = self
            .graph
            .linking_notes(path)
            .into_iter()
            .filter(|(p, _)| p != path)
            .map(|(p, _)| p.to_string())
            .collect();
        Some(NoteRow {
            path: path.to_string(),
            title: path.stem().to_string(),
            props,
            tags,
            modified_ms: stat.mtime_ms,
            created_ms: stat.ctime_ms,
            prop_links,
            backlinks,
        })
    }

    /// The notes in `folder` (and its subfolders when `recursive`) as rows,
    /// plus the notes they're related to.
    pub fn query(&self, folder: &VaultPath, recursive: bool) -> QueryResult {
        let prefix = if folder.is_root() { String::new() } else { format!("{}/", folder.as_str()) };
        let mut rows: Vec<NoteRow> = self
            .vault
            .markdown_files()
            .filter(|p| {
                let s = p.as_str();
                s.starts_with(&prefix) && (recursive || !s[prefix.len()..].contains('/'))
            })
            .filter_map(|p| self.note_row(p))
            .collect();
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        let inside: BTreeSet<String> = rows.iter().map(|r| r.path.clone()).collect();
        let mut want: BTreeSet<String> = BTreeSet::new();
        for r in &rows {
            want.extend(r.prop_links.values().flatten().cloned());
            want.extend(r.backlinks.iter().cloned());
        }
        let related = want
            .into_iter()
            .filter(|p| !inside.contains(p))
            .filter_map(|p| VaultPath::new(&p).ok())
            .filter_map(|p| self.note_row(&p))
            .collect();
        QueryResult { rows, related }
    }

    /// Resolved outgoing links of a note, with counts.
    pub fn outgoing(&self, source: &VaultPath) -> BTreeMap<VaultPath, u32> {
        self.graph.resolved.get(source).cloned().unwrap_or_default()
    }

    pub fn unresolved(&self, source: &VaultPath) -> BTreeMap<String, u32> {
        self.graph
            .unresolved
            .get(source)
            .cloned()
            .unwrap_or_default()
    }

    pub fn graph(&self) -> &LinkGraph {
        &self.graph
    }

    /// Every tag in the vault with how many notes use it.
    pub fn tags(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for m in self.metas.values() {
            // A note tagged #a/b also counts towards #a.
            let mut unique: BTreeSet<String> = BTreeSet::new();
            for t in m.all_tags() {
                let t = t.to_lowercase();
                let mut end = t.len();
                loop {
                    unique.insert(t[..end].to_string());
                    match t[..end].rfind('/') {
                        Some(i) if i > 1 => end = i,
                        _ => break,
                    }
                }
            }
            for t in unique {
                *out.entry(t).or_default() += 1;
            }
        }
        out
    }

    /// Text to put inside `[[...]]` for a link from `source` to `target`.
    pub fn link_text(&self, target: &VaultPath, source: &VaultPath) -> String {
        link_text(&self.names, target, source, self.link_format)
    }

    /// Notes and files whose name contains `query` (case-insensitive),
    /// best matches first. A starting point for the quick switcher.
    pub fn find_files(&self, query: &str, limit: usize) -> Vec<VaultPath> {
        let q = search::fold(query);
        let mut hits: Vec<(u8, usize, &VaultPath)> = self
            .vault
            .files()
            .filter_map(|p| {
                let name = search::fold(p.stem());
                let rank = if name == q {
                    0
                } else if name.starts_with(&q) {
                    1
                } else if name.contains(&q) {
                    2
                } else if search::fold(p.as_str()).contains(&q) {
                    3
                } else {
                    return None;
                };
                Some((rank, p.as_str().len(), p))
            })
            .collect();
        hits.sort();
        hits.into_iter()
            .take(limit)
            .map(|(_, _, p)| p.clone())
            .collect()
    }

    /// The whole file tree, folders first, for the sidebar.
    /// Facts about one note for its header and the inspector.
    pub fn note_info(&self, path: &VaultPath) -> Option<NoteInfo> {
        let meta = self.metas.get(path)?;
        let stat = self.vault.stat(path)?;
        let mut tags: Vec<String> = meta
            .all_tags()
            .into_iter()
            .map(|t| format!("#{}", t.trim_start_matches('#')))
            .collect();
        let mut seen = BTreeSet::new();
        tags.retain(|t| seen.insert(t.to_lowercase()));
        let backlinks = self
            .graph
            .linking_notes(path)
            .iter()
            .filter(|(p, _)| p != path)
            .count();
        Some(NoteInfo {
            tags,
            modified_ms: stat.mtime_ms,
            backlinks,
        })
    }

    /// Note count and the number of distinct note-to-note links.
    pub fn vault_stats(&self) -> VaultStats {
        let links = self
            .graph
            .resolved
            .iter()
            .map(|(src, targets)| {
                targets
                    .keys()
                    .filter(|t| *t != src && t.is_markdown())
                    .count()
            })
            .sum();
        VaultStats {
            notes: self.metas.len(),
            links,
        }
    }

    /// Notes and canvases, most recently modified first.
    pub fn recent_files(&self, limit: usize) -> Vec<RecentFile> {
        let mut v: Vec<RecentFile> = self
            .vault
            .files()
            .filter(|p| p.is_markdown() || p.extension() == "canvas")
            .filter_map(|p| {
                self.vault.stat(p).map(|s| RecentFile {
                    path: p.to_string(),
                    modified_ms: s.mtime_ms,
                })
            })
            .collect();
        v.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then_with(|| a.path.cmp(&b.path)));
        v.truncate(limit);
        v
    }

    pub fn tree(&self) -> TreeNode {
        self.subtree(&VaultPath::root())
    }

    fn subtree(&self, folder: &VaultPath) -> TreeNode {
        let children = self
            .vault
            .children(folder)
            .into_iter()
            .map(|(p, e)| match e {
                Entry::Folder => self.subtree(p),
                Entry::File(_) => TreeNode {
                    path: p.clone(),
                    name: p.name().to_string(),
                    kind: EntryKind::File,
                    children: vec![],
                },
            })
            .collect();
        TreeNode {
            path: folder.clone(),
            name: folder.name().to_string(),
            kind: EntryKind::Folder,
            children,
        }
    }
}

/// Indexes one note. Skips the read when mtime and size match what we (or
/// the cache) already know; skips the parse when the hash is known.
fn index_one(
    vault: &Vault,
    cache: &Cache,
    known: Option<&FileRecord>,
    p: &VaultPath,
) -> Result<Option<Indexed>> {
    let Some(stat) = vault.stat(p) else {
        return Ok(None);
    };
    let cached = match known {
        Some(r) => Some(r.clone()),
        None => cache.file(p.as_str())?,
    };
    if let Some(rec) = &cached {
        if rec.mtime_ms == stat.mtime_ms && rec.size == stat.size {
            if let Some(meta) = cache.meta(&rec.hash)? {
                return Ok(Some(Indexed {
                    path: p.clone(),
                    record: rec.clone(),
                    meta: Arc::new(meta),
                    parsed: false,
                    from_cache_stat: true,
                }));
            }
        }
    }
    let bytes = match vault.read_bytes(p) {
        Ok(b) => b,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let hash = hash_bytes(&bytes);
    let record = FileRecord {
        mtime_ms: stat.mtime_ms,
        size: stat.size,
        hash: hash.clone(),
    };
    if let Some(meta) = cache.meta(&hash)? {
        return Ok(Some(Indexed {
            path: p.clone(),
            record,
            meta: Arc::new(meta),
            parsed: false,
            from_cache_stat: false,
        }));
    }
    let meta = parse(&String::from_utf8_lossy(&bytes));
    Ok(Some(Indexed {
        path: p.clone(),
        record,
        meta: Arc::new(meta),
        parsed: true,
        from_cache_stat: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn p(s: &str) -> VaultPath {
        VaultPath::new(s).unwrap()
    }

    fn vault_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("people")).unwrap();
        fs::write(
            d.path().join("Home.md"),
            "# Home\nSee [[Alice]] and [[Bob]] #start\n",
        )
        .unwrap();
        fs::write(
            d.path().join("people/Alice.md"),
            "Back to [[Home]]. Knows [[Carol]].\n",
        )
        .unwrap();
        fs::write(
            d.path().join("Copy of Home.md"),
            "# Home\nSee [[Alice]] and [[Bob]] #start\n",
        )
        .unwrap();
        d
    }

    #[test]
    fn opens_indexes_and_resolves() {
        let d = vault_dir();
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        // Two notes share identical bytes: one parse, one hash reuse... but
        // they are parsed in parallel, so both may parse on a cold cache.
        assert_eq!(e.stats().parsed + e.stats().reused, 3);
        assert_eq!(
            e.resolve_link("alice", &p("Home.md")),
            Some(p("people/Alice.md"))
        );
        assert_eq!(
            e.unresolved(&p("Home.md")).keys().collect::<Vec<_>>(),
            vec!["Bob"]
        );
        let bl = e.backlinks(&p("Home.md"));
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, p("people/Alice.md"));
        assert_eq!(e.tags()["#start"], 2);
    }

    #[test]
    fn cache_survives_restart_and_skips_unchanged() {
        let d = vault_dir();
        let db = d.path().join(".marrow/cache.redb");
        {
            let e = Engine::open(d.path(), Cache::open(&db).unwrap()).unwrap();
            assert_eq!(e.stats().unchanged, 0);
        }
        let e = Engine::open(d.path(), Cache::open(&db).unwrap()).unwrap();
        assert_eq!(
            e.stats(),
            IndexStats {
                unchanged: 3,
                reused: 0,
                parsed: 0
            }
        );
    }

    #[test]
    fn creating_a_target_resolves_dangling_links() {
        let d = vault_dir();
        let mut e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        let ev = e
            .create_note(&p("people/Bob.md"), "Hi [[Alice]]\n")
            .unwrap();
        let relinked = ev.iter().find_map(|x| match x {
            EngineEvent::LinksChanged { paths } => Some(paths.clone()),
            _ => None,
        });
        let relinked = relinked.unwrap();
        assert!(relinked.contains(&p("Home.md")));
        assert!(relinked.contains(&p("Copy of Home.md")));
        assert_eq!(e.outgoing(&p("Home.md"))[&p("people/Bob.md")], 1);
        assert!(e.unresolved(&p("Home.md")).is_empty());
        assert_eq!(e.backlinks(&p("people/Alice.md")).len(), 3);
    }

    #[test]
    fn deleting_a_target_makes_links_dangle() {
        let d = vault_dir();
        let mut e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        e.trash(&p("people/Alice.md")).unwrap();
        assert!(e.unresolved(&p("Home.md")).contains_key("Alice"));
        assert!(e.meta(&p("people/Alice.md")).is_none());
    }

    #[test]
    fn external_edit_via_paths_changed() {
        let d = vault_dir();
        let mut e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            d.path().join("people/Alice.md"),
            "Now links [[Bob]] only, longer text.\n",
        )
        .unwrap();
        let ev = e.on_paths_changed(&[p("people/Alice.md")]).unwrap();
        assert!(ev.contains(&EngineEvent::MetadataChanged {
            path: p("people/Alice.md")
        }));
        assert!(e.backlinks(&p("Home.md")).is_empty());
    }

    #[test]
    fn rename_moves_index_entries() {
        let d = vault_dir();
        let mut e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        e.rename(&p("people"), &p("friends")).unwrap();
        assert!(e.meta(&p("friends/Alice.md")).is_some());
        assert!(e.meta(&p("people/Alice.md")).is_none());
        assert_eq!(
            e.resolve_link("Alice", &p("Home.md")),
            Some(p("friends/Alice.md"))
        );
        assert_eq!(
            e.outgoing(&p("Home.md")).keys().next(),
            Some(&p("friends/Alice.md"))
        );
        assert_eq!(e.backlinks(&p("Home.md"))[0].source, p("friends/Alice.md"));
    }

    #[test]
    fn tree_and_find() {
        let d = vault_dir();
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();
        let t = e.tree();
        assert_eq!(t.children[0].name, "people");
        assert_eq!(
            e.find_files("home", 5),
            vec![p("Home.md"), p("Copy of Home.md")]
        );
        assert_eq!(e.link_text(&p("people/Alice.md"), &p("Home.md")), "Alice");
    }

    #[test]
    fn query_returns_rows_and_related_notes() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("Tasks/Old")).unwrap();
        fs::write(d.path().join("Website.md"), "---\nstatus: active\n---\n# Website\n").unwrap();
        fs::write(
            d.path().join("Tasks/Design.md"),
            "---\nstatus: done\nhours: 3\nproject: \"[[Website]]\"\n---\nBody\n",
        )
        .unwrap();
        fs::write(d.path().join("Tasks/Build.md"), "---\nstatus: todo\nhours: 5\nproject: \"[[Website]]\"\n---\n").unwrap();
        fs::write(d.path().join("Tasks/Old/Archive.md"), "old").unwrap();
        fs::write(d.path().join("Notes.md"), "About [[Design]]").unwrap();
        let e = Engine::open(d.path(), Cache::in_memory().unwrap()).unwrap();

        let q = e.query(&p("Tasks"), false);
        let paths: Vec<&str> = q.rows.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["Tasks/Build.md", "Tasks/Design.md"]);
        let design = &q.rows[1];
        assert_eq!(design.title, "Design");
        assert_eq!(design.props["hours"], serde_json::json!(3));
        assert_eq!(design.prop_links["project"], vec!["Website.md"]);
        assert_eq!(design.backlinks, vec!["Notes.md"]);
        let related: Vec<&str> = q.related.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(related, vec!["Notes.md", "Website.md"]);
        assert_eq!(e.query(&p("Tasks"), true).rows.len(), 3);
    }
}
