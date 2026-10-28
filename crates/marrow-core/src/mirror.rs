//! Two-way sync between the vault and another place: a folder (usually one
//! inside Dropbox, OneDrive or Google Drive, whose app carries it to other
//! computers) or a cloud service Marrow talks to directly ([`Remote`]).
//!
//! Each run compares both folders with what they looked like after the last
//! run (kept in `.marrow/mirror/state.json`):
//!
//! * changed on one side → copied to the other;
//! * deleted on one side and unchanged on the other → deleted there too (on
//!   this computer it goes to Marrow's trash, `.marrow/trash`);
//! * deleted on one side but changed on the other → the changed file wins;
//! * a note changed on both sides → merged, using the last synced text as the
//!   base, so neither side's writing is lost;
//! * any other file changed on both sides → the newer one is kept and the
//!   older one is saved beside it as a "conflicted copy".
//!
//! It refuses to run when the other folder is missing or suddenly empty (a
//! cloud app that isn't running or is signed out), and pauses before
//! deleting many files at once, so a mistake can't empty the vault.
//!
//! Hidden files and folders (`.marrow`, `.obsidian`, `.git`, …) aren't synced.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::cache::hash_bytes;
use crate::merge::{merge2, merge3};
use crate::vault::CONFIG_DIR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    /// Modified time, milliseconds since 1970.
    m: u64,
    /// Size in bytes.
    s: u64,
}

/// A file's version on the other side: it changes whenever the file does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Version {
    /// Modified time (ms) and size, for folders.
    Stamp { m: u64, s: u64 },
    /// A server's tag for the content (an ETag or revision).
    Tag(String),
}

impl From<Stamp> for Version {
    fn from(s: Stamp) -> Self {
        Version::Stamp { m: s.m, s: s.s }
    }
}

/// A file on the other side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub version: Version,
    /// When it last changed (ms since 1970), to pick the newer of two versions.
    pub modified_ms: u64,
}

/// The other side of a sync: a folder, or a cloud service. Paths are
/// relative, with `/`. Hidden files and folders are left out of `list`.
pub trait Remote: Send {
    /// Identifies this place; a different one starts a fresh sync.
    fn id(&self) -> String;
    /// Ok when it can be reached; otherwise why not.
    fn check(&self) -> Result<(), SyncStop>;
    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>>;
    fn read(&self, key: &str) -> io::Result<Vec<u8>>;
    /// Writes a whole file (making folders as needed) and returns its new version.
    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version>;
    fn delete(&self, key: &str) -> io::Result<()>;
}

/// Another folder on this computer.
pub struct FolderRemote {
    root: PathBuf,
}

impl FolderRemote {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FolderRemote { root: root.into() }
    }
}

impl Remote for FolderRemote {
    fn id(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }
    fn check(&self) -> Result<(), SyncStop> {
        if self.root.is_dir() {
            Ok(())
        } else {
            Err(SyncStop::Missing)
        }
    }
    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        let mut out: BTreeMap<String, RemoteFile> = scan(&self.root)?
            .into_iter()
            .map(|(k, s)| (k, RemoteFile { version: s.into(), modified_ms: s.m }))
            .collect();
        // Files iCloud hasn't downloaded yet are still there (never "deleted").
        for (k, s) in icloud_placeholders(&self.root) {
            out.entry(k).or_insert(RemoteFile { version: Version::Tag(format!("icloud-pending-{}", s.m)), modified_ms: s.m });
        }
        Ok(out)
    }
    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let p = self.root.join(key);
        match fs::read(&p) {
            Err(e) if e.kind() == io::ErrorKind::NotFound && placeholder_of(&p).is_file() => {
                Err(io::Error::other("iCloud hasn't downloaded it to this device yet; it syncs once it has"))
            }
            r => r,
        }
    }
    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let p = self.root.join(key);
        write_atomic(&p, bytes)?;
        Ok(stamp(&p)?.into())
    }
    fn delete(&self, key: &str) -> io::Result<()> {
        // The cloud app keeps deleted files in its own trash.
        let p = self.root.join(key);
        match fs::remove_file(&p) {
            Err(e) if e.kind() == io::ErrorKind::NotFound && placeholder_of(&p).is_file() => fs::remove_file(placeholder_of(&p))?,
            r => r?,
        }
        remove_empty_parents(&p, &self.root);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Seen {
    hash: String,
    l: Stamp,
    r: Version,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    remote: String,
    files: BTreeMap<String, Seen>,
}

/// What a run did. Paths are relative, with `/`.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Copied from the other folder into the vault.
    pub to_local: Vec<String>,
    /// Copied from the vault to the other folder.
    pub to_remote: Vec<String>,
    /// Deleted in the vault (moved to Marrow's trash) because they were deleted there.
    pub deleted_local: Vec<String>,
    /// Deleted there because they were deleted in the vault.
    pub deleted_remote: Vec<String>,
    /// Notes changed on both sides and merged.
    pub merged: Vec<String>,
    /// Conflicted copies made for files that couldn't be merged.
    pub conflict_copies: Vec<String>,
    /// Files that couldn't be synced this time, with the reason.
    pub errors: Vec<String>,
}

impl SyncReport {
    pub fn changed(&self) -> bool {
        !(self.to_local.is_empty()
            && self.to_remote.is_empty()
            && self.deleted_local.is_empty()
            && self.deleted_remote.is_empty()
            && self.merged.is_empty()
            && self.conflict_copies.is_empty())
    }
}

/// Why a run didn't happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyncStop {
    /// The other folder doesn't exist (drive not mounted, app not running).
    Missing,
    /// A cloud service can't be reached (offline, server down).
    Offline { message: String },
    /// The cloud service turned down the sign-in; sign in again.
    SignIn { message: String },
    /// One side is empty while the other isn't, after they were synced before.
    LooksEmpty { side: Side },
    /// The run would delete many files; pass `force` to go ahead.
    ManyDeletes { side: Side, count: usize },
    Io { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Local,
    Remote,
}

impl std::fmt::Display for SyncStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let where_ = |s: &Side| if *s == Side::Local { "this vault" } else { "the sync folder" };
        match self {
            SyncStop::Missing => write!(f, "The sync folder can't be found. Is the cloud app running?"),
            SyncStop::Offline { message } => write!(f, "Can't reach the cloud right now ({message}). Marrow tries again soon."),
            SyncStop::SignIn { message } => write!(f, "The cloud didn't accept the sign-in ({message}). Sign in again in Settings → Vault → Sync."),
            SyncStop::LooksEmpty { side } => write!(f, "Sync paused: {} is suddenly empty.", where_(side)),
            SyncStop::ManyDeletes { side, count } => {
                write!(f, "Sync paused: it would delete {count} files from {}.", where_(side))
            }
            SyncStop::Io { message } => write!(f, "Sync failed: {message}"),
        }
    }
}

fn io_stop(e: io::Error) -> SyncStop {
    SyncStop::Io { message: e.to_string() }
}

/// Deletions above this need `force` when they're also over half the files.
const MANY_DELETES: usize = 20;
/// Base text is kept for notes up to this size, for merging.
const MAX_BASE: u64 = 4 * 1024 * 1024;

pub struct Mirror {
    local: PathBuf,
    remote: Box<dyn Remote>,
}

impl Mirror {
    /// Syncs `local` with another folder.
    pub fn new(local: impl Into<PathBuf>, remote: impl Into<PathBuf>) -> Self {
        Self::with_remote(local, Box::new(FolderRemote::new(remote)))
    }

    /// Syncs `local` with any [`Remote`], such as a cloud service.
    pub fn with_remote(local: impl Into<PathBuf>, remote: Box<dyn Remote>) -> Self {
        Mirror { local: local.into(), remote }
    }

    fn meta_dir(&self) -> PathBuf {
        self.local.join(CONFIG_DIR).join("mirror")
    }

    fn load_state(&self) -> State {
        let remote = self.remote.id();
        fs::read(self.meta_dir().join("state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<State>(&b).ok())
            // A different folder starts from scratch.
            .filter(|s| s.remote == remote)
            .unwrap_or(State {
                remote,
                files: BTreeMap::new(),
            })
    }

    fn save_state(&self, state: &State) -> io::Result<()> {
        let dir = self.meta_dir();
        fs::create_dir_all(&dir)?;
        write_atomic(&dir.join("state.json"), &serde_json::to_vec(state).map_err(io::Error::other)?)
    }

    /// Forgets what was synced (used when sync is turned off).
    pub fn reset(&self) {
        let _ = fs::remove_dir_all(self.meta_dir());
    }

    /// Runs one sync. `force` goes ahead with a large delete that was paused.
    pub fn sync(&self, force: bool) -> Result<SyncReport, SyncStop> {
        self.remote.check()?;
        if !self.local.is_dir() {
            return Err(SyncStop::Io {
                message: "the vault folder is missing".into(),
            });
        }
        let mut state = self.load_state();
        let lmap = scan(&self.local).map_err(io_stop)?;
        let rmap = self.remote.list().map_err(io_stop)?;

        // A small vault can legitimately end up empty; a larger one going empty at once is suspicious.
        if state.files.len() > 5 && !force {
            if rmap.is_empty() && !lmap.is_empty() {
                return Err(SyncStop::LooksEmpty { side: Side::Remote });
            }
            if lmap.is_empty() && !rmap.is_empty() {
                return Err(SyncStop::LooksEmpty { side: Side::Local });
            }
        }

        let keys: BTreeSet<String> = lmap
            .keys()
            .chain(rmap.keys())
            .chain(state.files.keys())
            .cloned()
            .collect();

        // Decide first, so a large delete can be stopped before anything happens.
        let mut plan: Vec<(String, Action)> = Vec::new();
        for k in keys {
            let l = lmap.get(&k).copied();
            let r = rmap.get(&k);
            let seen = state.files.get(&k).cloned();
            let action = self.decide(&k, l, r, seen.as_ref());
            plan.push((k, action));
        }
        if !force {
            let tracked = state.files.len().max(1);
            for side in [Side::Local, Side::Remote] {
                let count = plan
                    .iter()
                    .filter(|(_, a)| matches!(a, Action::Delete(s) if *s == side))
                    .count();
                if count > MANY_DELETES && count * 2 > tracked {
                    return Err(SyncStop::ManyDeletes { side, count });
                }
            }
        }

        let mut report = SyncReport::default();
        for (k, action) in plan {
            let l = lmap.get(&k).copied();
            let r = rmap.get(&k);
            match self.apply(&k, action, l, r, &rmap, &mut state, &mut report) {
                Ok(()) => {}
                Err(e) => report.errors.push(format!("{k}: {e}")),
            }
        }
        self.save_state(&state).map_err(io_stop)?;
        self.prune_bases(&state);
        Ok(report)
    }

    fn decide(&self, key: &str, l: Option<Stamp>, r: Option<&RemoteFile>, seen: Option<&Seen>) -> Action {
        let mut lc = changed(l, seen.map(|s| s.l));
        let mut rc = changed(r.map(|f| &f.version), seen.map(|s| &s.r));
        // A new time with the same content isn't a change.
        if let Some(s) = seen {
            if lc && l.is_some() && hash_file(&self.local.join(key)).as_deref() == Some(s.hash.as_str()) {
                lc = false;
            }
            if rc && r.is_some() && self.remote.read(key).ok().map(|b| hash_bytes(&b)).as_deref() == Some(s.hash.as_str()) {
                rc = false;
            }
        }
        match (lc, rc, l.is_some(), r.is_some()) {
            (false, false, true, true) => Action::Refresh,
            (false, false, _, _) => Action::Forget,
            // Changed on one side only.
            (true, false, true, _) => Action::Copy(Side::Remote),
            (true, false, false, true) => Action::Delete(Side::Remote),
            (false, true, _, true) => Action::Copy(Side::Local),
            (false, true, true, false) => Action::Delete(Side::Local),
            (true, false, false, false) | (false, true, false, false) => Action::Forget,
            // Changed on both.
            (true, true, false, false) => Action::Forget,
            (true, true, true, false) => Action::Copy(Side::Remote),
            (true, true, false, true) => Action::Copy(Side::Local),
            (true, true, true, true) => Action::Reconcile,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &self,
        key: &str,
        action: Action,
        l: Option<Stamp>,
        r: Option<&RemoteFile>,
        rmap: &BTreeMap<String, RemoteFile>,
        state: &mut State,
        report: &mut SyncReport,
    ) -> io::Result<()> {
        let lp = self.local.join(key);
        match action {
            Action::Forget => {
                state.files.remove(key);
            }
            Action::Refresh => {
                // Same content; just remember the new versions.
                if let (Some(s), Some(l), Some(r)) = (state.files.get_mut(key), l, r) {
                    s.l = l;
                    s.r = r.version.clone();
                }
            }
            Action::Copy(Side::Remote) => {
                let bytes = fs::read(&lp)?;
                let rv = self.remote.write(key, &bytes)?;
                let hash = hash_bytes(&bytes);
                self.keep_base(key, &hash, &bytes);
                let ls = match l {
                    Some(l) => l,
                    None => stamp(&lp)?,
                };
                state.files.insert(key.to_string(), Seen { hash, l: ls, r: rv });
                report.to_remote.push(key.to_string());
            }
            Action::Copy(Side::Local) => {
                let bytes = self.remote.read(key)?;
                write_atomic(&lp, &bytes)?;
                let hash = hash_bytes(&bytes);
                self.keep_base(key, &hash, &bytes);
                let rv = match r {
                    Some(r) => r.version.clone(),
                    None => self.remote.list()?.remove(key).map(|f| f.version).ok_or_else(|| io::Error::other("vanished"))?,
                };
                state.files.insert(key.to_string(), Seen { hash, l: stamp(&lp)?, r: rv });
                report.to_local.push(key.to_string());
            }
            Action::Delete(Side::Local) => {
                self.to_trash(key)?;
                remove_empty_parents(&lp, &self.local);
                state.files.remove(key);
                report.deleted_local.push(key.to_string());
            }
            Action::Delete(Side::Remote) => {
                self.remote.delete(key)?;
                state.files.remove(key);
                report.deleted_remote.push(key.to_string());
            }
            Action::Reconcile => {
                let lb = fs::read(&lp)?;
                let rb = self.remote.read(key)?;
                let (lh, rh) = (hash_bytes(&lb), hash_bytes(&rb));
                if lh == rh {
                    self.keep_base(key, &lh, &lb);
                    let ls = match l {
                        Some(l) => l,
                        None => stamp(&lp)?,
                    };
                    let rv = r.map(|r| r.version.clone()).ok_or_else(|| io::Error::other("vanished"))?;
                    state.files.insert(key.to_string(), Seen { hash: lh, l: ls, r: rv });
                    return Ok(());
                }
                let texts = (is_note(key))
                    .then(|| (String::from_utf8(lb.clone()).ok(), String::from_utf8(rb.clone()).ok()))
                    // Locked notes can't be merged line by line: keep both versions.
                    .filter(|(l, r)| ![l, r].iter().any(|t| t.as_deref().is_some_and(crate::lock::is_locked)));
                if let Some((Some(lt), Some(rt))) = texts {
                    // Merge, with the last synced text as the base when we have it.
                    let base = state.files.get(key).and_then(|s| self.read_base(&s.hash));
                    let merged = match base {
                        Some(b) => merge3(&b, &lt, &rt).text,
                        None => merge2(&lt, &rt).text,
                    };
                    let bytes = merged.into_bytes();
                    write_atomic(&lp, &bytes)?;
                    let rv = self.remote.write(key, &bytes)?;
                    let hash = hash_bytes(&bytes);
                    self.keep_base(key, &hash, &bytes);
                    state.files.insert(key.to_string(), Seen { hash, l: stamp(&lp)?, r: rv });
                    report.merged.push(key.to_string());
                } else {
                    // Keep the newer file; save the older one beside it on both sides.
                    let local_newer = l.map(|s| s.m).unwrap_or(0) >= r.map(|f| f.modified_ms).unwrap_or(0);
                    let (winner, loser) = if local_newer { (&lb, &rb) } else { (&rb, &lb) };
                    let copy = conflict_name(key, |c| self.local.join(c).exists() || rmap.contains_key(c) || state.files.contains_key(c));
                    for (k, bytes) in [(copy.clone(), loser), (key.to_string(), winner)] {
                        let p = self.local.join(&k);
                        write_atomic(&p, bytes)?;
                        let rv = self.remote.write(&k, bytes)?;
                        state.files.insert(k, Seen { hash: hash_bytes(bytes), l: stamp(&p)?, r: rv });
                    }
                    report.conflict_copies.push(copy);
                }
            }
        }
        Ok(())
    }

    /// Moves a vault file into Marrow's trash, the same place the app's
    /// Delete puts things.
    fn to_trash(&self, key: &str) -> io::Result<()> {
        let src = self.local.join(key);
        let trash = self.local.join(CONFIG_DIR).join("trash");
        let mut dst = trash.join(key);
        let mut n = 1;
        while dst.exists() {
            dst = trash.join(numbered(key, n));
            n += 1;
        }
        if let Some(dir) = dst.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::rename(&src, &dst).or_else(|_| {
            fs::copy(&src, &dst)?;
            fs::remove_file(&src)
        })
    }

    fn base_path(&self, hash: &str) -> PathBuf {
        self.meta_dir().join("base").join(hash)
    }

    fn keep_base(&self, key: &str, hash: &str, bytes: &[u8]) {
        if !is_note(key) || bytes.len() as u64 > MAX_BASE {
            return;
        }
        let p = self.base_path(hash);
        if p.exists() {
            return;
        }
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        let _ = crate::atomic::write(&p, bytes);
    }

    fn read_base(&self, hash: &str) -> Option<String> {
        fs::read_to_string(self.base_path(hash)).ok()
    }

    /// Drops base texts nothing refers to any more.
    fn prune_bases(&self, state: &State) {
        let live: BTreeSet<&str> = state.files.values().map(|s| s.hash.as_str()).collect();
        if let Ok(rd) = fs::read_dir(self.meta_dir().join("base")) {
            for e in rd.flatten() {
                if !live.contains(e.file_name().to_string_lossy().as_ref()) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Nothing to do; drop it from the state.
    Forget,
    /// Same on both sides; update the stored times.
    Refresh,
    /// Copy to this side.
    Copy(Side),
    /// Delete on this side.
    Delete(Side),
    /// Changed on both sides.
    Reconcile,
}

fn changed<T: PartialEq>(now: Option<T>, before: Option<T>) -> bool {
    match (now, before) {
        (None, None) => false,
        (Some(a), Some(b)) => a != b,
        _ => true,
    }
}

fn is_note(key: &str) -> bool {
    let k = key.to_lowercase();
    k.ends_with(".md") || k.ends_with(".txt")
}

/// `a/Note.md` → `a/Note 2.md`
fn numbered(key: &str, n: usize) -> String {
    let (stem, ext) = split_ext(key);
    format!("{stem} {n}{ext}")
}

fn split_ext(key: &str) -> (&str, &str) {
    let name_start = key.rfind('/').map(|i| i + 1).unwrap_or(0);
    match key[name_start..].rfind('.') {
        Some(i) if i > 0 => key.split_at(name_start + i),
        _ => (key, ""),
    }
}

/// `pic.png` → `pic (Marrow conflicted copy 2026-10-15).png`
fn conflict_name(key: &str, exists: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_ext(key);
    let day = today();
    let mut name = format!("{stem} (Marrow conflicted copy {day}){ext}");
    let mut n = 2;
    while exists(&name) {
        name = format!("{stem} (Marrow conflicted copy {day} {n}){ext}");
        n += 1;
    }
    name
}

fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    // Civil date from days since 1970 (UTC).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Names sync leaves alone: hidden files, Office lock files, system and partial files.
pub fn skip_name(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with("~$")
        || name.eq_ignore_ascii_case("desktop.ini")
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.ends_with(".tmp")
        || name.ends_with(".partial")
        || name.ends_with(".crdownload")
}

/// Every file under `root` (skipping hidden ones), keyed by relative path.
fn scan(root: &Path) -> io::Result<BTreeMap<String, Stamp>> {
    let mut out = BTreeMap::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !skip_name(&e.file_name().to_string_lossy()));
    for entry in walker {
        let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_err(io::Error::other)?;
        let key = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let md = entry.metadata().map_err(|e| io::Error::other(e.to_string()))?;
        out.insert(key, stamp_of(&md));
    }
    Ok(out)
}

/// Where iCloud keeps a stand-in for a file it hasn't downloaded:
/// `folder/.name.icloud` for `folder/name`.
fn placeholder_of(p: &Path) -> PathBuf {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    p.with_file_name(format!(".{name}.icloud"))
}

/// Files iCloud knows about but hasn't downloaded yet, by the path they'll have.
fn icloud_placeholders(root: &Path) -> Vec<(String, Stamp)> {
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_type().is_file() || !skip_name(&e.file_name().to_string_lossy()));
    let mut out = vec![];
    for entry in walker.flatten() {
        let name = entry.file_name().to_string_lossy();
        let Some(real) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".icloud")) else { continue };
        if !entry.file_type().is_file() || real.is_empty() || skip_name(real) {
            continue;
        }
        let Ok(rel) = entry.path().with_file_name(real).strip_prefix(root).map(Path::to_path_buf) else { continue };
        let key = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        if let Ok(md) = entry.metadata() {
            out.push((key, stamp_of(&md)));
        }
    }
    out
}

fn stamp_of(md: &fs::Metadata) -> Stamp {
    let m = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Stamp { m, s: md.len() }
}

fn stamp(p: &Path) -> io::Result<Stamp> {
    Ok(stamp_of(&fs::metadata(p)?))
}

fn hash_file(p: &Path) -> Option<String> {
    fs::read(p).ok().map(|b| hash_bytes(&b))
}

/// Writes through a temporary file so a half-written file is never seen.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent folder"))?;
    fs::create_dir_all(dir)?;
    match crate::atomic::write(path, bytes) {
        Ok(()) => Ok(()),
        // Some sync folders refuse to replace a file in one step.
        Err(_) => fs::write(path, bytes),
    }
}

fn remove_empty_parents(file: &Path, root: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// How many files a folder holds (hidden ones aside), for "this folder
/// already has N files" before the first sync.
pub fn count_files(root: &Path) -> usize {
    scan(root).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }
    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }
    /// Makes sure a later write gets a different time stamp.
    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Mirror) {
        let t = tempfile::tempdir().unwrap();
        let l = t.path().join("vault");
        let r = t.path().join("cloud");
        fs::create_dir_all(&l).unwrap();
        fs::create_dir_all(&r).unwrap();
        let m = Mirror::new(&l, &r);
        (t, l, r, m)
    }

    #[test]
    fn files_icloud_has_not_downloaded_are_never_taken_as_deleted() {
        let (_t, l, r, m) = setup();
        write(&l.join("Note.md"), "mine\n");
        write(&l.join("sub/Two.md"), "two\n");
        m.sync(false).unwrap();
        // iCloud swaps the files for stand-ins until they're downloaded again.
        fs::remove_file(r.join("Note.md")).unwrap();
        write(&r.join(".Note.md.icloud"), "stub");
        fs::remove_file(r.join("sub/Two.md")).unwrap();
        write(&r.join("sub/.Two.md.icloud"), "stub");
        tick();
        let rep = m.sync(false).unwrap();
        assert!(rep.deleted_local.is_empty(), "{rep:?}");
        assert_eq!(read(&l.join("Note.md")), "mine\n");
        assert_eq!(read(&l.join("sub/Two.md")), "two\n");
        assert!(rep.errors.iter().all(|e| e.contains("iCloud")), "{:?}", rep.errors);
        // Once downloaded, all is well again.
        fs::remove_file(r.join(".Note.md.icloud")).unwrap();
        write(&r.join("Note.md"), "mine\n");
        fs::remove_file(r.join("sub/.Two.md.icloud")).unwrap();
        write(&r.join("sub/Two.md"), "two\n");
        let rep = m.sync(false).unwrap();
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(read(&l.join("Note.md")), "mine\n");
    }

    #[test]
    fn first_sync_combines_both_folders() {
        let (_t, l, r, m) = setup();
        write(&l.join("A.md"), "a\n");
        write(&l.join(".marrow/cache"), "x");
        write(&r.join("sub/B.md"), "b\n");
        write(&l.join("Same.md"), "one\ntwo\n");
        write(&r.join("Same.md"), "one\nthree\n");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.to_remote, vec!["A.md"]);
        assert_eq!(rep.to_local, vec!["sub/B.md"]);
        assert_eq!(rep.merged, vec!["Same.md"]);
        assert_eq!(read(&l.join("Same.md")), "one\ntwo\nthree\n");
        assert_eq!(read(&r.join("Same.md")), "one\ntwo\nthree\n");
        assert!(!r.join(".marrow").exists());
        // Nothing changed: nothing to do.
        assert!(!m.sync(false).unwrap().changed());
    }

    #[test]
    fn edits_and_deletes_flow_both_ways() {
        let (_t, l, r, m) = setup();
        write(&l.join("A.md"), "a\n");
        write(&l.join("B.md"), "b\n");
        write(&l.join("dir/C.md"), "c\n");
        m.sync(false).unwrap();
        tick();
        write(&r.join("A.md"), "a edited on the other computer\n");
        fs::remove_file(l.join("B.md")).unwrap();
        fs::remove_file(r.join("dir/C.md")).unwrap();
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.to_local, vec!["A.md"]);
        assert_eq!(rep.deleted_remote, vec!["B.md"]);
        assert_eq!(rep.deleted_local, vec!["dir/C.md"]);
        assert_eq!(read(&l.join("A.md")), "a edited on the other computer\n");
        assert!(!r.join("B.md").exists());
        // Deleted here = moved to Marrow's trash, and the empty folder is gone.
        assert!(!l.join("dir").exists());
        assert_eq!(read(&l.join(".marrow/trash/dir/C.md")), "c\n");
    }

    #[test]
    fn edits_on_both_sides_merge_with_the_base() {
        let (_t, l, r, m) = setup();
        write(&l.join("N.md"), "title\n\none\n\ntwo\n");
        m.sync(false).unwrap();
        tick();
        write(&l.join("N.md"), "title\n\none, edited here\n\ntwo\n");
        write(&r.join("N.md"), "title\n\none\n\ntwo, edited there\n");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.merged, vec!["N.md"]);
        let want = "title\n\none, edited here\n\ntwo, edited there\n";
        assert_eq!(read(&l.join("N.md")), want);
        assert_eq!(read(&r.join("N.md")), want);
    }

    #[test]
    fn other_files_keep_both_versions() {
        let (_t, l, r, m) = setup();
        write(&l.join("pic.png"), "v1");
        m.sync(false).unwrap();
        tick();
        write(&l.join("pic.png"), "local v2");
        tick();
        write(&r.join("pic.png"), "remote v2");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.conflict_copies.len(), 1);
        let copy = &rep.conflict_copies[0];
        assert!(copy.starts_with("pic (Marrow conflicted copy "), "{copy}");
        assert_eq!(read(&l.join("pic.png")), "remote v2");
        assert_eq!(read(&l.join(copy)), "local v2");
        assert_eq!(read(&r.join(copy)), "local v2");
        assert!(!m.sync(false).unwrap().changed());
    }

    #[test]
    fn locked_notes_keep_both_versions() {
        use crate::lock::{derive, seal, Cost};
        let k = derive("pw", [1u8; 16], Cost { m: 64, t: 1, p: 1 }).unwrap();
        let (_t, l, r, m) = setup();
        write(&l.join("Secret.md"), &seal("one", &k, None));
        m.sync(false).unwrap();
        tick();
        write(&l.join("Secret.md"), &seal("local", &k, None));
        tick();
        let remote = seal("remote", &k, None);
        write(&r.join("Secret.md"), &remote);
        let rep = m.sync(false).unwrap();
        assert!(rep.merged.is_empty());
        assert_eq!(rep.conflict_copies.len(), 1);
        assert_eq!(read(&l.join("Secret.md")), remote);
        assert!(crate::lock::is_locked(&read(&l.join(&rep.conflict_copies[0]))));
    }

    #[test]
    fn locked_notes_are_not_indexed() {
        use crate::lock::{derive, seal, Cost};
        let k = derive("pw", [1u8; 16], Cost { m: 64, t: 1, p: 1 }).unwrap();
        let meta = crate::parse::parse(&seal("# Heading\n[[Link]] #tag\n- [ ] task", &k, None));
        assert!(meta.links.is_empty() && meta.tags.is_empty() && meta.headings.is_empty());
    }

    #[test]
    fn delete_vs_edit_keeps_the_edit() {
        let (_t, l, r, m) = setup();
        write(&l.join("A.md"), "a\n");
        m.sync(false).unwrap();
        tick();
        fs::remove_file(l.join("A.md")).unwrap();
        write(&r.join("A.md"), "a, still wanted\n");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.to_local, vec!["A.md"]);
        assert_eq!(read(&l.join("A.md")), "a, still wanted\n");
    }

    #[test]
    fn stops_when_the_cloud_folder_is_gone_or_empty() {
        let (_t, l, r, m) = setup();
        for i in 0..30 {
            write(&l.join(format!("n{i}.md")), "x\n");
        }
        m.sync(false).unwrap();
        // The cloud app signed out and emptied its folder.
        for i in 0..30 {
            fs::remove_file(r.join(format!("n{i}.md"))).unwrap();
        }
        assert_eq!(m.sync(false).unwrap_err(), SyncStop::LooksEmpty { side: Side::Remote });
        // Most (not all) files gone: paused, not deleted.
        write(&r.join("n0.md"), "x\n");
        assert_eq!(
            m.sync(false).unwrap_err(),
            SyncStop::ManyDeletes {
                side: Side::Local,
                count: 29
            }
        );
        assert!(l.join("n5.md").exists());
        // The folder itself is missing.
        fs::remove_dir_all(&r).unwrap();
        assert_eq!(m.sync(false).unwrap_err(), SyncStop::Missing);
        assert!(l.join("n5.md").exists());
    }

    #[test]
    fn names() {
        assert_eq!(numbered("a/Note.md", 2), "a/Note 2.md");
        assert_eq!(numbered("a.b/Note", 2), "a.b/Note 2");
        let c = conflict_name("x/pic.png", |_| false);
        assert!(c.starts_with("x/pic (Marrow conflicted copy 20") && c.ends_with(").png"), "{c}");
        assert_eq!(today().len(), 10);
    }

    /// A cloud service kept in memory: versions are tags, like ETags.
    #[derive(Default)]
    struct Cloud {
        files: std::sync::Mutex<BTreeMap<String, (Vec<u8>, u64)>>,
        next: std::sync::atomic::AtomicU64,
        offline: std::sync::atomic::AtomicBool,
    }

    impl Cloud {
        fn put(&self, key: &str, text: &str) {
            let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.files.lock().unwrap().insert(key.into(), (text.as_bytes().to_vec(), n));
        }
        fn get(&self, key: &str) -> Option<String> {
            self.files.lock().unwrap().get(key).map(|(b, _)| String::from_utf8(b.clone()).unwrap())
        }
    }

    impl Remote for std::sync::Arc<Cloud> {
        fn id(&self) -> String {
            "memory".into()
        }
        fn check(&self) -> Result<(), SyncStop> {
            if self.offline.load(std::sync::atomic::Ordering::SeqCst) {
                Err(SyncStop::Offline { message: "no network".into() })
            } else {
                Ok(())
            }
        }
        fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .map(|(k, (_, n))| (k.clone(), RemoteFile { version: Version::Tag(format!("v{n}")), modified_ms: *n }))
                .collect())
        }
        fn read(&self, key: &str) -> io::Result<Vec<u8>> {
            self.files.lock().unwrap().get(key).map(|(b, _)| b.clone()).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
        fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
            let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.files.lock().unwrap().insert(key.into(), (bytes.to_vec(), n));
            Ok(Version::Tag(format!("v{n}")))
        }
        fn delete(&self, key: &str) -> io::Result<()> {
            self.files.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[test]
    fn syncs_with_a_cloud_service() {
        let t = tempfile::tempdir().unwrap();
        let l = t.path().join("vault");
        fs::create_dir_all(&l).unwrap();
        let cloud = std::sync::Arc::new(Cloud::default());
        let m = Mirror::with_remote(&l, Box::new(cloud.clone()));
        write(&l.join("A.md"), "a\n");
        write(&l.join("N.md"), "title\n\none\n\ntwo\n");
        cloud.put("from phone/B.md", "b\n");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.to_remote, vec!["A.md", "N.md"]);
        assert_eq!(rep.to_local, vec!["from phone/B.md"]);
        assert_eq!(read(&l.join("from phone/B.md")), "b\n");
        assert!(!m.sync(false).unwrap().changed());

        // Edited in both places: merged, and both sides get the result.
        tick();
        write(&l.join("N.md"), "title\n\none, on the computer\n\ntwo\n");
        cloud.put("N.md", "title\n\none\n\ntwo, on the phone\n");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.merged, vec!["N.md"]);
        let want = "title\n\none, on the computer\n\ntwo, on the phone\n";
        assert_eq!(read(&l.join("N.md")), want);
        assert_eq!(cloud.get("N.md").unwrap(), want);

        // Deleted on the phone: moved to Marrow's trash here.
        cloud.files.lock().unwrap().remove("A.md");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.deleted_local, vec!["A.md"]);
        assert_eq!(read(&l.join(".marrow/trash/A.md")), "a\n");

        // Offline: nothing happens and nothing is lost.
        cloud.offline.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(matches!(m.sync(false), Err(SyncStop::Offline { .. })));
        cloud.offline.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(!m.sync(false).unwrap().changed());
    }

    #[test]
    fn binary_files_changed_on_both_sides_keep_both() {
        let t = tempfile::tempdir().unwrap();
        let l = t.path().join("vault");
        fs::create_dir_all(&l).unwrap();
        let cloud = std::sync::Arc::new(Cloud::default());
        let m = Mirror::with_remote(&l, Box::new(cloud.clone()));
        write(&l.join("pic.png"), "one");
        m.sync(false).unwrap();
        tick();
        write(&l.join("pic.png"), "local");
        cloud.put("pic.png", "remote");
        let rep = m.sync(false).unwrap();
        assert_eq!(rep.conflict_copies.len(), 1);
        let copy = &rep.conflict_copies[0];
        assert_eq!(read(&l.join(copy)), cloud.get(copy).unwrap());
        assert_eq!(read(&l.join("pic.png")), cloud.get("pic.png").unwrap());
        let mut both = vec![read(&l.join("pic.png")), read(&l.join(copy))];
        both.sort();
        assert_eq!(both, vec!["local", "remote"]);
    }

    #[test]
    fn state_saved_by_older_versions_still_loads() {
        let (_t, l, r, m) = setup();
        write(&l.join("A.md"), "a\n");
        m.sync(false).unwrap();
        // Before cloud services, the other side's version was always {m, s}.
        let raw = fs::read_to_string(l.join(".marrow/mirror/state.json")).unwrap();
        assert!(raw.contains("\"r\":{\"m\":"), "{raw}");
        assert!(!m.sync(false).unwrap().changed());
        drop(r);
    }
}
