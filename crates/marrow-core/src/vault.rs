//! The in-memory vault: every file and folder under the root, kept in sync
//! with disk by *reconciling* paths rather than trusting watcher events.
//!
//! A [`Vault`] is owned by exactly one thread (the engine). All disk reads and
//! writes go through `&mut self`, so a save and a watcher event for the same
//! file can never interleave.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::error::{Error, IoContext, Result};
use crate::path::VaultPath;

/// Name of the per-vault settings folder. Hidden folders are never indexed.
pub const CONFIG_DIR: &str = ".marrow";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// Modified time, milliseconds since the Unix epoch.
    pub mtime_ms: i64,
    /// Creation time where the OS reports it, else the modified time.
    pub ctime_ms: i64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    File(FileStat),
    Folder,
}

impl Entry {
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Folder,
}

/// What changed in the vault after a scan, reconcile or write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum VaultEvent {
    Created {
        path: VaultPath,
        kind: EntryKind,
    },
    Modified {
        path: VaultPath,
    },
    Deleted {
        path: VaultPath,
        kind: EntryKind,
    },
    Renamed {
        from: VaultPath,
        to: VaultPath,
        kind: EntryKind,
    },
}

pub struct Vault {
    root: PathBuf,
    entries: BTreeMap<VaultPath, Entry>,
    case_insensitive: bool,
}

impl Vault {
    /// Opens a vault and scans it. Returns the vault and one `Created` event
    /// per entry found.
    pub fn open(root: impl Into<PathBuf>) -> Result<(Self, Vec<VaultEvent>)> {
        let root: PathBuf = root.into();
        // dunce avoids Windows `\\?\` verbatim paths, which would leak into the UI
        // and differ from the paths the file watcher reports.
        let root = dunce::canonicalize(&root).at(&root)?;
        if !root.is_dir() {
            return Err(Error::NotFound(root.display().to_string()));
        }
        let mut v = Vault {
            root,
            entries: BTreeMap::new(),
            case_insensitive: cfg!(any(windows, target_os = "macos")),
        };
        let events = v.scan()?;
        Ok((v, events))
    }

    /// Overrides case-sensitivity detection (used by tests).
    pub fn set_case_insensitive(&mut self, yes: bool) {
        self.case_insensitive = yes;
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, path: &VaultPath) -> Option<&Entry> {
        if path.is_root() {
            return Some(&Entry::Folder);
        }
        self.entries.get(path)
    }

    pub fn stat(&self, path: &VaultPath) -> Option<FileStat> {
        match self.entries.get(path) {
            Some(Entry::File(s)) => Some(*s),
            _ => None,
        }
    }

    /// All entries in path order.
    pub fn entries(&self) -> impl Iterator<Item = (&VaultPath, &Entry)> {
        self.entries.iter()
    }

    pub fn files(&self) -> impl Iterator<Item = &VaultPath> {
        self.entries
            .iter()
            .filter(|(_, e)| e.is_file())
            .map(|(p, _)| p)
    }

    pub fn markdown_files(&self) -> impl Iterator<Item = &VaultPath> {
        self.files().filter(|p| p.is_markdown())
    }

    /// Direct children of a folder, folders first then files, each by name.
    pub fn children(&self, folder: &VaultPath) -> Vec<(&VaultPath, &Entry)> {
        let mut out: Vec<_> = self
            .entries
            .iter()
            .filter(|(p, _)| &p.parent() == folder && !p.is_root())
            .collect();
        out.sort_by(|a, b| {
            b.1.is_file()
                .cmp(&a.1.is_file())
                .reverse()
                .then_with(|| a.0.name().to_lowercase().cmp(&b.0.name().to_lowercase()))
        });
        out
    }

    // ---------------------------------------------------------------- scan

    fn scan(&mut self) -> Result<Vec<VaultEvent>> {
        let mut events = Vec::new();
        let walker = WalkDir::new(&self.root)
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
        for item in walker {
            let item = match item {
                Ok(i) => i,
                Err(_) => continue, // unreadable entry: skip, like a missing file
            };
            let ft = item.file_type();
            if ft.is_symlink() {
                continue;
            }
            let path = VaultPath::from_os(&self.root, item.path())?;
            if ft.is_dir() {
                self.entries.insert(path.clone(), Entry::Folder);
                events.push(VaultEvent::Created {
                    path,
                    kind: EntryKind::Folder,
                });
            } else if ft.is_file() {
                let md = item.metadata().map_err(|e| Error::Io {
                    path: item.path().to_path_buf(),
                    source: e.into(),
                })?;
                self.entries
                    .insert(path.clone(), Entry::File(file_stat(&md)));
                events.push(VaultEvent::Created {
                    path,
                    kind: EntryKind::File,
                });
            }
        }
        Ok(events)
    }

    // ----------------------------------------------------------- reconcile

    /// Brings one path in line with what is on disk and reports what changed.
    ///
    /// Never trusts a watcher's event kind: it stats the path and compares
    /// modified time and size with the stored record.
    pub fn reconcile(&mut self, path: &VaultPath) -> Result<Vec<VaultEvent>> {
        let mut events = Vec::new();
        if path.is_root() || path.is_hidden() {
            return Ok(events);
        }
        // Parents first, so the tree never holds an orphan.
        let parent = path.parent();
        if !parent.is_root() && !matches!(self.entries.get(&parent), Some(Entry::Folder)) {
            events.extend(self.reconcile(&parent)?);
        }

        let full = path.to_os(&self.root);
        let meta = match fs::symlink_metadata(&full) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => {
                return Err(Error::Io {
                    path: full,
                    source: e,
                })
            }
        };
        let meta = match meta {
            Some(_) if self.case_insensitive && !self.exact_name_exists(path)? => None,
            other => other,
        };

        match meta {
            None => events.extend(self.remove_subtree(path)),
            Some(m) if m.file_type().is_symlink() => events.extend(self.remove_subtree(path)),
            Some(m) if m.is_file() => {
                let stat = file_stat(&m);
                match self.entries.get(path).copied() {
                    Some(Entry::File(old)) => {
                        if old.mtime_ms != stat.mtime_ms || old.size != stat.size {
                            self.entries.insert(path.clone(), Entry::File(stat));
                            events.push(VaultEvent::Modified { path: path.clone() });
                        }
                    }
                    Some(Entry::Folder) => {
                        events.extend(self.remove_subtree(path));
                        self.entries.insert(path.clone(), Entry::File(stat));
                        events.push(VaultEvent::Created {
                            path: path.clone(),
                            kind: EntryKind::File,
                        });
                    }
                    None => {
                        self.entries.insert(path.clone(), Entry::File(stat));
                        events.push(VaultEvent::Created {
                            path: path.clone(),
                            kind: EntryKind::File,
                        });
                    }
                }
            }
            Some(m) if m.is_dir() => match self.entries.get(path).copied() {
                Some(Entry::Folder) => {}
                existing => {
                    if existing.is_some() {
                        events.extend(self.remove_subtree(path));
                    }
                    self.entries.insert(path.clone(), Entry::Folder);
                    events.push(VaultEvent::Created {
                        path: path.clone(),
                        kind: EntryKind::Folder,
                    });
                    // A folder moved in from outside arrives with its contents.
                    events.extend(self.scan_subtree(path)?);
                }
            },
            Some(_) => {} // sockets, devices: ignore
        }
        Ok(events)
    }

    /// Re-checks every known path and looks for new ones. Used after the
    /// watcher reports an overflow or on demand.
    pub fn rescan(&mut self) -> Result<Vec<VaultEvent>> {
        let mut events = Vec::new();
        let known: Vec<VaultPath> = self.entries.keys().cloned().collect();
        for p in known {
            if self.entries.contains_key(&p) {
                events.extend(self.reconcile(&p)?);
            }
        }
        events.extend(self.scan_subtree(&VaultPath::root())?);
        Ok(events)
    }

    fn scan_subtree(&mut self, folder: &VaultPath) -> Result<Vec<VaultEvent>> {
        let mut events = Vec::new();
        let base = folder.to_os(&self.root);
        let walker = WalkDir::new(&base)
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
        for item in walker.flatten() {
            let ft = item.file_type();
            if ft.is_symlink() {
                continue;
            }
            let p = VaultPath::from_os(&self.root, item.path())?;
            if self.entries.contains_key(&p) {
                continue;
            }
            if ft.is_dir() {
                self.entries.insert(p.clone(), Entry::Folder);
                events.push(VaultEvent::Created {
                    path: p,
                    kind: EntryKind::Folder,
                });
            } else if ft.is_file() {
                if let Ok(md) = item.metadata() {
                    self.entries.insert(p.clone(), Entry::File(file_stat(&md)));
                    events.push(VaultEvent::Created {
                        path: p,
                        kind: EntryKind::File,
                    });
                }
            }
        }
        Ok(events)
    }

    /// On case-insensitive filesystems `stat("note.md")` succeeds for
    /// `Note.md`. Confirm the exact (NFC) name is in the parent listing.
    fn exact_name_exists(&self, path: &VaultPath) -> Result<bool> {
        let dir = path.parent().to_os(&self.root);
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
            Err(e) => {
                return Err(Error::Io {
                    path: dir,
                    source: e,
                })
            }
        };
        let want = path.name();
        for ent in rd.flatten() {
            let name: String = ent.file_name().to_string_lossy().nfc().collect();
            if name == want {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Removes a path and everything under it; deepest paths reported first.
    fn remove_subtree(&mut self, path: &VaultPath) -> Vec<VaultEvent> {
        let doomed: Vec<(VaultPath, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, e)| (p.clone(), *e))
            .collect();
        let mut events = Vec::new();
        for (p, e) in doomed.into_iter().rev() {
            self.entries.remove(&p);
            let kind = if e.is_file() {
                EntryKind::File
            } else {
                EntryKind::Folder
            };
            events.push(VaultEvent::Deleted { path: p, kind });
        }
        events
    }

    // ------------------------------------------------------------- file ops

    pub fn read_bytes(&self, path: &VaultPath) -> Result<Vec<u8>> {
        let full = path.to_os(&self.root);
        fs::read(&full).at(full)
    }

    pub fn read_text(&self, path: &VaultPath) -> Result<String> {
        let bytes = self.read_bytes(path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Overwrites (or creates) a file, then reconciles it.
    pub fn write(&mut self, path: &VaultPath, contents: &str) -> Result<Vec<VaultEvent>> {
        if path.is_root() || path.is_hidden() {
            return Err(Error::InvalidPath(path.to_string()));
        }
        let full = path.to_os(&self.root);
        if let Some(dir) = full.parent() {
            fs::create_dir_all(dir).at(dir)?;
        }
        fs::write(&full, contents).at(&full)?;
        self.reconcile(path)
    }

    /// Creates a new file; fails if anything already exists at `path`.
    pub fn create(&mut self, path: &VaultPath, contents: &str) -> Result<Vec<VaultEvent>> {
        if self.get(path).is_some() || path.to_os(&self.root).exists() {
            return Err(Error::AlreadyExists(path.to_string()));
        }
        self.write(path, contents)
    }

    pub fn create_folder(&mut self, path: &VaultPath) -> Result<Vec<VaultEvent>> {
        if path.is_root() || path.is_hidden() {
            return Err(Error::InvalidPath(path.to_string()));
        }
        let full = path.to_os(&self.root);
        fs::create_dir_all(&full).at(&full)?;
        self.reconcile(path)
    }

    /// Renames a file or folder. Reports one `Renamed` per moved entry.
    ///
    /// A case-only rename (`note.md` → `Note.md`) is allowed even on
    /// case-insensitive filesystems.
    pub fn rename(&mut self, from: &VaultPath, to: &VaultPath) -> Result<Vec<VaultEvent>> {
        if from.is_root() || to.is_root() || to.is_hidden() || to.starts_with(from) && from != to {
            return Err(Error::InvalidPath(to.to_string()));
        }
        if self.get(from).is_none() {
            return Err(Error::NotFound(from.to_string()));
        }
        let case_only = from.as_str().to_lowercase() == to.as_str().to_lowercase();
        if from == to {
            return Ok(Vec::new());
        }
        if !case_only && (self.get(to).is_some() || to.to_os(&self.root).exists()) {
            return Err(Error::AlreadyExists(to.to_string()));
        }
        let src = from.to_os(&self.root);
        let dst = to.to_os(&self.root);
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir).at(dir)?;
        }
        fs::rename(&src, &dst).at(&src)?;

        let mut events = Vec::new();
        // Make sure new parent folders are known first.
        let parent = to.parent();
        if !parent.is_root() && self.get(&parent).is_none() {
            events.extend(self.reconcile(&parent)?);
        }
        let moved: Vec<(VaultPath, Entry)> = self
            .entries
            .iter()
            .filter(|(p, _)| p.starts_with(from))
            .map(|(p, e)| (p.clone(), *e))
            .collect();
        for (old, _) in &moved {
            self.entries.remove(old);
        }
        for (old, entry) in moved {
            let suffix = &old.as_str()[from.as_str().len()..];
            let new = VaultPath::new(&format!("{}{}", to.as_str(), suffix))?;
            self.entries.insert(new.clone(), entry);
            let kind = if entry.is_file() {
                EntryKind::File
            } else {
                EntryKind::Folder
            };
            events.push(VaultEvent::Renamed {
                from: old,
                to: new,
                kind,
            });
        }
        Ok(events)
    }

    /// Moves a file or folder into the vault's local trash
    /// (`.marrow/trash/`), which is hidden and never indexed.
    pub fn trash(&mut self, path: &VaultPath) -> Result<Vec<VaultEvent>> {
        if path.is_root() || self.get(path).is_none() {
            return Err(Error::NotFound(path.to_string()));
        }
        let trash_dir = self.root.join(CONFIG_DIR).join("trash");
        let mut dst = trash_dir.clone();
        for seg in path.segments() {
            dst.push(seg);
        }
        // Never overwrite an earlier trashed copy.
        let mut n = 1;
        while dst.exists() {
            let mut name = path.stem().to_string();
            name.push_str(&format!(" {n}"));
            let ext = path.extension();
            if !ext.is_empty() && matches!(self.get(path), Some(Entry::File(_))) {
                name.push('.');
                name.push_str(&ext);
            }
            dst.set_file_name(name);
            n += 1;
        }
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir).at(dir)?;
        }
        let src = path.to_os(&self.root);
        fs::rename(&src, &dst).at(&src)?;
        Ok(self.remove_subtree(path))
    }
}

fn file_stat(m: &fs::Metadata) -> FileStat {
    let ms = |t: std::io::Result<std::time::SystemTime>| -> Option<i64> {
        t.ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
    };
    let mtime_ms = ms(m.modified()).unwrap_or(0);
    FileStat {
        mtime_ms,
        ctime_ms: ms(m.created()).unwrap_or(mtime_ms),
        size: m.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn p(s: &str) -> VaultPath {
        VaultPath::new(s).unwrap()
    }

    fn setup() -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("notes/daily")).unwrap();
        fs::create_dir_all(dir.path().join(".marrow")).unwrap();
        fs::write(dir.path().join("notes/A.md"), "a").unwrap();
        fs::write(dir.path().join("notes/daily/2026-10-14.md"), "d").unwrap();
        fs::write(dir.path().join("img.png"), [0u8, 1, 2]).unwrap();
        fs::write(dir.path().join(".marrow/app.json"), "{}").unwrap();
        let (v, _) = Vault::open(dir.path()).unwrap();
        (dir, v)
    }

    #[test]
    fn scan_finds_files_and_skips_hidden() {
        let (_d, v) = setup();
        let files: Vec<_> = v.files().map(|p| p.as_str().to_string()).collect();
        assert_eq!(
            files,
            vec!["img.png", "notes/A.md", "notes/daily/2026-10-14.md"]
        );
        assert!(v.get(&p("notes/daily")).is_some());
        assert!(v.get(&p(".marrow")).is_none());
        assert_eq!(v.markdown_files().count(), 2);
    }

    #[test]
    fn children_lists_folders_first() {
        let (_d, v) = setup();
        let names: Vec<_> = v
            .children(&p("notes"))
            .iter()
            .map(|(p, _)| p.name().to_string())
            .collect();
        assert_eq!(names, vec!["daily", "A.md"]);
    }

    #[test]
    fn reconcile_detects_external_create_modify_delete() {
        let (d, mut v) = setup();
        fs::create_dir_all(d.path().join("new/deep")).unwrap();
        fs::write(d.path().join("new/deep/B.md"), "x").unwrap();
        let ev = v.reconcile(&p("new/deep/B.md")).unwrap();
        assert_eq!(
            ev,
            vec![
                VaultEvent::Created {
                    path: p("new"),
                    kind: EntryKind::Folder
                },
                VaultEvent::Created {
                    path: p("new/deep"),
                    kind: EntryKind::Folder
                },
                VaultEvent::Created {
                    path: p("new/deep/B.md"),
                    kind: EntryKind::File
                },
            ]
        );
        // Same content, untouched: nothing to report.
        assert!(v.reconcile(&p("new/deep/B.md")).unwrap().is_empty());

        sleep(Duration::from_millis(20));
        fs::write(d.path().join("new/deep/B.md"), "longer").unwrap();
        assert_eq!(
            v.reconcile(&p("new/deep/B.md")).unwrap(),
            vec![VaultEvent::Modified {
                path: p("new/deep/B.md")
            }]
        );

        fs::remove_dir_all(d.path().join("new")).unwrap();
        let ev = v.reconcile(&p("new")).unwrap();
        assert_eq!(ev.len(), 3);
        assert!(v.get(&p("new/deep/B.md")).is_none());
    }

    #[test]
    fn folder_moved_in_brings_contents() {
        let (d, mut v) = setup();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("pack")).unwrap();
        fs::write(outside.path().join("pack/C.md"), "c").unwrap();
        fs::rename(outside.path().join("pack"), d.path().join("pack")).unwrap();
        let ev = v.reconcile(&p("pack")).unwrap();
        assert!(ev.contains(&VaultEvent::Created {
            path: p("pack/C.md"),
            kind: EntryKind::File
        }));
    }

    #[test]
    fn case_only_rename_seen_on_case_insensitive_fs() {
        let (d, mut v) = setup();
        v.set_case_insensitive(true);
        // Simulate: the file on disk is now "a.md" but we still know "A.md".
        fs::rename(d.path().join("notes/A.md"), d.path().join("notes/a.md")).unwrap();
        let ev = v.reconcile(&p("notes/A.md")).unwrap();
        assert_eq!(
            ev,
            vec![VaultEvent::Deleted {
                path: p("notes/A.md"),
                kind: EntryKind::File
            }]
        );
        let ev = v.reconcile(&p("notes/a.md")).unwrap();
        assert_eq!(
            ev,
            vec![VaultEvent::Created {
                path: p("notes/a.md"),
                kind: EntryKind::File
            }]
        );
    }

    #[test]
    fn write_create_rename_trash() {
        let (d, mut v) = setup();
        let ev = v.create(&p("inbox/Idea.md"), "hello").unwrap();
        assert!(ev.contains(&VaultEvent::Created {
            path: p("inbox/Idea.md"),
            kind: EntryKind::File
        }));
        assert!(v.create(&p("inbox/Idea.md"), "again").is_err());
        assert_eq!(v.read_text(&p("inbox/Idea.md")).unwrap(), "hello");

        let ev = v.rename(&p("notes"), &p("archive/notes")).unwrap();
        assert!(ev.contains(&VaultEvent::Renamed {
            from: p("notes/A.md"),
            to: p("archive/notes/A.md"),
            kind: EntryKind::File
        }));
        assert!(v.get(&p("archive/notes/daily/2026-10-14.md")).is_some());
        assert!(v.get(&p("notes")).is_none());
        assert!(d.path().join("archive/notes/A.md").exists());

        v.trash(&p("inbox/Idea.md")).unwrap();
        assert!(v.get(&p("inbox/Idea.md")).is_none());
        assert!(d.path().join(".marrow/trash/inbox/Idea.md").exists());
        // Trashing a second file with the same path never overwrites the first.
        v.create(&p("inbox/Idea.md"), "two").unwrap();
        v.trash(&p("inbox/Idea.md")).unwrap();
        assert!(d.path().join(".marrow/trash/inbox/Idea 1.md").exists());
    }

    #[test]
    fn rename_into_itself_is_refused() {
        let (_d, mut v) = setup();
        assert!(v.rename(&p("notes"), &p("notes/sub")).is_err());
    }
}
