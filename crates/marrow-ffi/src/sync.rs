//! Keeping a vault in sync with a WebDAV server (Nextcloud, ownCloud,
//! Synology, pCloud, Koofr…) or another folder, optionally end-to-end
//! encrypted with a passphrase. It's the desktop app's sync engine, so a
//! phone and a computer can share one vault.
//!
//! Sync runs on a background thread while a vault is open: soon after each
//! save, every 15 seconds, and when the app asks (coming to the front).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use marrow_core::mirror::{FolderRemote, Mirror, Remote, SyncStop};
use marrow_core::sealed::{self, Sealed};
use marrow_core::vault_locks as vl;
use serde::{Deserialize, Serialize};

use crate::{fail, vp, Marrow, MarrowListener, Result, SecretStore};

const INTERVAL: Duration = Duration::from_secs(15);
/// Saves arriving close together are synced together.
const SETTLE: Duration = Duration::from_millis(1500);

/// Where a vault syncs to (kept in the app's settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum Target {
    Webdav { url: String, user: String },
    Folder { path: String },
}

impl Target {
    fn secret_id(&self) -> Option<String> {
        match self {
            Target::Webdav { url, user } => Some(format!("sync:webdav:{user}@{url}")),
            Target::Folder { .. } => None,
        }
    }

    fn label(&self) -> String {
        match self {
            Target::Webdav { url, .. } => reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| url.clone()),
            Target::Folder { path } => Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone()),
        }
    }

    fn connect(&self, secrets: &dyn SecretStore) -> std::result::Result<Box<dyn Remote>, String> {
        match self {
            Target::Webdav { url, user } => {
                let password = self.secret_id().and_then(|id| secrets.get(id)).unwrap_or_default();
                Ok(Box::new(marrow_webdav::WebDav::new(url, user, &password)?))
            }
            Target::Folder { path } => Ok(Box::new(FolderRemote::new(path))),
        }
    }
}

fn passphrase_id(root: &Path) -> String {
    format!("sync:passphrase:{}", root.to_string_lossy())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncKind {
    Webdav,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncProblem {
    /// Offline, or the server can't be reached; it tries again by itself.
    Offline,
    /// The server turned down the password.
    SignIn,
    /// It would delete many files, or one side looks emptied: the person decides.
    NeedsConfirming,
    /// The folder isn't there.
    Missing,
    Other,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncStatus {
    pub kind: SyncKind,
    /// The server's name, or the folder's.
    pub label: String,
    /// The address or folder.
    pub location: String,
    pub encrypted: bool,
    pub syncing: bool,
    /// When the last sync finished (ms since 1970).
    pub last_sync_ms: Option<i64>,
    pub problem: Option<SyncProblem>,
    /// What went wrong, in words.
    pub message: Option<String>,
    /// Files brought in or sent by the last run.
    pub last_changes: u32,
}

/// What's already at a place before syncing with it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncPlace {
    pub files: u32,
    /// It holds a vault encrypted by Marrow: its passphrase is needed.
    pub encrypted: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Conflict {
    pub copy: String,
    pub original: String,
    pub source: String,
}

enum Msg {
    Poke { force: bool },
    Stop,
}

pub(crate) struct Worker {
    tx: Sender<Msg>,
    status: Arc<Mutex<SyncStatus>>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn problem(stop: &SyncStop) -> SyncProblem {
    match stop {
        SyncStop::Offline { .. } => SyncProblem::Offline,
        SyncStop::SignIn { .. } => SyncProblem::SignIn,
        SyncStop::LooksEmpty { .. } | SyncStop::ManyDeletes { .. } => SyncProblem::NeedsConfirming,
        SyncStop::Missing => SyncProblem::Missing,
        SyncStop::Io { .. } => SyncProblem::Other,
    }
}

fn check_sealed(remote: Box<dyn Remote>, passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < 8 {
        return Err(fail("Use a sync passphrase of at least 8 characters."));
    }
    Sealed::new(remote, passphrase).check().map_err(|e| fail(e.to_string()))
}

fn tell(listener: &Arc<Mutex<Option<Arc<dyn MarrowListener>>>>, s: &SyncStatus) {
    if let Some(l) = listener.lock().ok().and_then(|l| l.clone()) {
        l.sync_changed(s.clone());
    }
}

impl Marrow {
    /// Starts syncing the open vault if it's set up to (after opening it).
    pub(crate) fn resume_sync(&self) {
        let Ok(root) = self.root() else { return };
        let s = self.app_settings();
        let key = root.to_string_lossy().into_owned();
        if let Some(t) = s.sync.get(&key).cloned() {
            self.start_sync(&root, t, s.encrypted.contains(&key));
        }
    }

    fn start_sync(&self, root: &Path, target: Target, encrypted: bool) {
        self.stop_sync();
        let (tx, rx) = channel::<Msg>();
        let status = Arc::new(Mutex::new(SyncStatus {
            kind: match target {
                Target::Webdav { .. } => SyncKind::Webdav,
                Target::Folder { .. } => SyncKind::Folder,
            },
            label: target.label(),
            location: match &target {
                Target::Webdav { url, .. } => url.clone(),
                Target::Folder { path } => path.clone(),
            },
            encrypted,
            syncing: false,
            last_sync_ms: None,
            problem: None,
            message: None,
            last_changes: 0,
        }));
        let worker_status = status.clone();
        let listener = self.listener.clone();
        let secrets = self.secrets.clone();
        let root: PathBuf = root.to_path_buf();
        std::thread::Builder::new()
            .name("marrow-sync".into())
            .spawn(move || {
                let remote = target.connect(secrets.as_ref()).and_then(|r| {
                    if !encrypted {
                        return Ok(r);
                    }
                    let pass = secrets.get(passphrase_id(&root)).ok_or("The sync passphrase is missing on this device. Set up sync again.")?;
                    Ok(Box::new(Sealed::new(r, &pass)) as Box<dyn Remote>)
                });
                let m = match remote {
                    Ok(r) => Mirror::with_remote(&root, r),
                    Err(e) => {
                        if let Ok(mut s) = worker_status.lock() {
                            s.problem = Some(SyncProblem::Other);
                            s.message = Some(e);
                            tell(&listener, &s);
                        }
                        return;
                    }
                };
                let mut force = false;
                loop {
                    if let Ok(mut s) = worker_status.lock() {
                        s.syncing = true;
                        tell(&listener, &s);
                    }
                    let result = m.sync(force);
                    force = false;
                    if let Ok(mut s) = worker_status.lock() {
                        s.syncing = false;
                        match result {
                            Ok(r) => {
                                s.last_sync_ms = Some(now_ms());
                                s.problem = None;
                                s.message = r.errors.first().cloned();
                                s.last_changes = (r.to_local.len() + r.to_remote.len() + r.deleted_local.len() + r.deleted_remote.len() + r.merged.len()) as u32;
                            }
                            Err(stop) => {
                                s.problem = Some(problem(&stop));
                                s.message = Some(stop.to_string());
                            }
                        }
                        tell(&listener, &s);
                    }
                    match rx.recv_timeout(INTERVAL) {
                        Ok(Msg::Poke { force: f }) => {
                            force = f;
                            std::thread::sleep(SETTLE);
                            while let Ok(m) = rx.try_recv() {
                                match m {
                                    Msg::Poke { force: f } => force |= f,
                                    Msg::Stop => return,
                                }
                            }
                        }
                        Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
            })
            .ok();
        if let Ok(mut slot) = self.sync.lock() {
            *slot = Some(Worker { tx, status });
        }
    }

    pub(crate) fn stop_sync(&self) {
        if let Ok(mut slot) = self.sync.lock() {
            if let Some(w) = slot.take() {
                let _ = w.tx.send(Msg::Stop);
            }
        }
    }

    /// Asks for a sync soon (after a save).
    pub(crate) fn poke_sync(&self) {
        if let Ok(slot) = self.sync.lock() {
            if let Some(w) = slot.as_ref() {
                let _ = w.tx.send(Msg::Poke { force: false });
            }
        }
    }

    fn set_target(&self, target: Option<Target>, passphrase: Option<String>) -> Result<()> {
        let root = self.root()?;
        let key = root.to_string_lossy().into_owned();
        let mut s = self.app_settings();
        let old = s.sync.remove(&key);
        if let Some(id) = old.as_ref().and_then(Target::secret_id) {
            if target.as_ref().and_then(Target::secret_id).as_deref() != Some(id.as_str()) {
                self.secrets.set(id, None);
            }
        }
        // A new place (or a change of encryption) starts from a clean slate.
        if old != target || s.encrypted.contains(&key) != passphrase.is_some() {
            Mirror::new(&root, "").reset();
        }
        match &passphrase {
            Some(p) => {
                self.secrets.set(passphrase_id(&root), Some(p.clone()));
                s.encrypted.insert(key.clone());
            }
            None => {
                self.secrets.set(passphrase_id(&root), None);
                s.encrypted.remove(&key);
            }
        }
        match target {
            Some(t) => {
                s.sync.insert(key.clone(), t.clone());
                self.save_app_settings(&s)?;
                self.start_sync(&root, t, passphrase.is_some());
            }
            None => {
                self.save_app_settings(&s)?;
                self.stop_sync();
            }
        }
        Ok(())
    }
}

#[uniffi::export]
impl Marrow {
    /// Signs in to a WebDAV server (making the folder if needed) and says
    /// what's there. Slow: call it off the main thread.
    pub fn check_webdav(&self, url: String, user: String, password: String) -> Result<SyncPlace> {
        let dav = marrow_webdav::WebDav::new(&url, &user, &password).map_err(fail)?;
        dav.check().map_err(|e| fail(e.to_string()))?;
        Ok(SyncPlace { files: dav.list()?.len() as u32, encrypted: sealed::is_encrypted(&dav) })
    }

    /// Syncs the open vault with a WebDAV server. `passphrase` turns on
    /// end-to-end encryption (the server only ever sees sealed files).
    pub fn sync_with_webdav(&self, url: String, user: String, password: String, passphrase: Option<String>) -> Result<()> {
        let passphrase = passphrase.filter(|p| !p.is_empty());
        let dav = marrow_webdav::WebDav::new(&url, &user, &password).map_err(fail)?;
        dav.check().map_err(|e| fail(e.to_string()))?;
        match &passphrase {
            Some(p) => check_sealed(Box::new(dav), p)?,
            None if sealed::is_encrypted(&dav) => return Err(fail("That folder holds an encrypted vault. Turn on encryption and enter its sync passphrase.")),
            None => {}
        }
        let target = Target::Webdav { url: url.trim().to_string(), user: user.trim().to_string() };
        if let Some(id) = target.secret_id() {
            self.secrets.set(id, Some(password));
        }
        self.set_target(Some(target), passphrase)
    }

    /// Syncs the open vault with another folder on this device (an iCloud
    /// Drive or Files folder, say).
    pub fn sync_with_folder(&self, path: String, passphrase: Option<String>) -> Result<()> {
        let passphrase = passphrase.filter(|p| !p.is_empty());
        let root = self.root()?;
        std::fs::create_dir_all(&path)?;
        let r = dunce_canon(Path::new(&path))?;
        let l = dunce_canon(&root)?;
        if r.starts_with(&l) || l.starts_with(&r) {
            return Err(fail("Pick a folder outside this vault (and not one that contains it)."));
        }
        let folder = Box::new(FolderRemote::new(&r));
        match &passphrase {
            Some(p) => check_sealed(folder, p)?,
            None if sealed::is_encrypted(folder.as_ref()) => return Err(fail("That folder holds an encrypted vault. Turn on encryption and enter its sync passphrase.")),
            None => {}
        }
        self.set_target(Some(Target::Folder { path: r.to_string_lossy().into_owned() }), passphrase)
    }

    /// Stops syncing the open vault (nothing is deleted anywhere).
    pub fn turn_off_sync(&self) -> Result<()> {
        self.set_target(None, None)
    }

    /// The open vault's sync, if it syncs.
    pub fn sync_status(&self) -> Option<SyncStatus> {
        let slot = self.sync.lock().ok()?;
        let s = slot.as_ref()?.status.lock().ok()?.clone();
        Some(s)
    }

    /// Syncs now. `force` goes ahead with a large delete that was paused.
    pub fn sync_now(&self, force: bool) {
        if let Ok(slot) = self.sync.lock() {
            if let Some(w) = slot.as_ref() {
                let _ = w.tx.send(Msg::Poke { force });
            }
        }
    }

    /// Conflict copies sync left in the vault (from any sync app).
    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        self.with(|e| {
            let paths: Vec<String> = e.vault().markdown_files().map(|p| p.to_string()).collect();
            Ok(marrow_core::conflicts::find(&paths).into_iter().map(|c| Conflict { copy: c.copy, original: c.original, source: c.source }).collect())
        })
    }

    /// Merges a conflict copy into its original and moves the copy to the
    /// trash. Returns how many places both versions were kept.
    pub fn resolve_conflict(&self, copy: String, original: String) -> Result<u32> {
        let (cp, op) = (vp(&copy)?, vp(&original)?);
        let (conflicts, events) = self.with(|e| {
            let raw = e.read_note(&op)?;
            let a = vl::reveal(&self.locks, raw.clone())?;
            let b = vl::reveal(&self.locks, e.read_note(&cp)?)?;
            let merged = marrow_core::merge2(&a, &b);
            let mut events = vec![];
            if merged.text != a {
                let text = vl::conceal(&self.locks, e.vault().root(), op.as_str(), Some(&raw), &merged.text)?;
                events.extend(e.write_note(&op, &text)?);
            }
            events.extend(e.trash(&cp)?);
            Ok((merged.conflicts as u32, events))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(conflicts)
    }
}

fn dunce_canon(p: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(p)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_sync_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("A.md", "hello")]);
        let other = dir.path().join("other");
        assert!(m.sync_with_folder(dir.path().join("vault/inside").to_string_lossy().into(), None).is_err());
        m.sync_with_folder(other.to_string_lossy().into(), Some("correct horse".into())).unwrap();
        let s = m.sync_status().unwrap();
        assert!(s.encrypted);
        assert_eq!(s.kind, SyncKind::Folder);
        // Wait for the first run.
        for _ in 0..100 {
            if m.sync_status().unwrap().last_sync_ms.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let st = m.sync_status().unwrap();
        assert!(st.last_sync_ms.is_some(), "{:?}", st.message);
        assert!(sealed::is_encrypted(&FolderRemote::new(&other)));
        // Nothing readable reached the other folder.
        for f in walk(&other) {
            assert!(!std::fs::read(&f).unwrap().windows(5).any(|w| w == b"hello"), "{}", f.display());
        }
        m.turn_off_sync().unwrap();
        assert!(m.sync_status().is_none());
    }

    fn walk(p: &Path) -> Vec<PathBuf> {
        let mut out = vec![];
        for e in std::fs::read_dir(p).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() {
                out.extend(walk(&e.path()));
            } else {
                out.push(e.path());
            }
        }
        out
    }
}
