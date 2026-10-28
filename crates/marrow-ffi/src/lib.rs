//! Marrow's engine for the native phone and tablet apps.
//!
//! The iOS app (SwiftUI) and the Android app (Jetpack Compose) draw every
//! screen themselves; everything else — the vault, the index, links, search,
//! history, locks, sync and the AI — is the same Rust code the desktop app
//! uses, reached through one [`Marrow`] object. UniFFI writes the Swift and
//! Kotlin side of this interface (see `apps/README.md`).
//!
//! | module     | job                                                      |
//! |------------|----------------------------------------------------------|
//! | `files`    | opening a vault; listing, creating, renaming, moving     |
//! | `notes`    | reading and saving notes, links, tags, tasks, search     |
//! | `render`   | Markdown → blocks the apps draw natively; editor colours |
//! | `daily`    | daily notes, the calendar, capture, resurfacing          |
//! | `history`  | earlier versions and the time machine                    |
//! | `locks`    | password-locked notes and folders                        |
//! | `sync`     | sync with a WebDAV server or a folder, end-to-end sealed |
//! | `ai`       | AI providers, Ask your notes, note actions               |
//! | `graph`    | the link graph, laid out for drawing                     |
//! | `canvas`   | JSON Canvas boards                                       |

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use marrow_core::vault_locks::Locks;
use marrow_core::{Engine, EngineEvent, VaultEvent, VaultPath, VaultService};

mod ai;
mod canvas;
mod cloud;
mod daily;
mod files;
mod graph;
mod history;
mod locks;
mod notes;
mod render;
mod settings;
mod sync;

pub use ai::*;
pub use canvas::*;
pub use daily::*;
pub use files::*;
pub use graph::*;
pub use history::*;
pub use locks::*;
pub use notes::*;
pub use render::*;
pub use settings::*;
pub use sync::*;

uniffi::setup_scaffolding!();

// ------------------------------------------------------------------ errors

/// What went wrong, in words for the person, or a state the app handles.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MarrowError {
    /// Show `reason` to the person.
    #[error("{reason}")]
    Failed { reason: String },
    /// The note is locked: ask for its password, then try again.
    #[error("This note is locked.")]
    Locked,
    /// The note is in a locked folder: ask for the folder's password.
    #[error("The folder “{folder}” is locked.")]
    LockedFolder { folder: String },
    /// Another app changed the note since it was read: read it again.
    #[error("This note was changed somewhere else.")]
    ChangedElsewhere,
    #[error("No vault is open.")]
    NoVault,
}

pub type Result<T, E = MarrowError> = std::result::Result<T, E>;

const CHANGED: &str = "CHANGED_ELSEWHERE";

impl From<String> for MarrowError {
    fn from(s: String) -> Self {
        use marrow_core::vault_locks::{LOCKED, LOCKED_FOLDER};
        if s == LOCKED {
            MarrowError::Locked
        } else if let Some(f) = s.strip_prefix(LOCKED_FOLDER) {
            MarrowError::LockedFolder { folder: f.to_string() }
        } else if s == CHANGED {
            MarrowError::ChangedElsewhere
        } else {
            MarrowError::Failed { reason: s }
        }
    }
}

impl From<&str> for MarrowError {
    fn from(s: &str) -> Self {
        s.to_string().into()
    }
}

impl From<marrow_core::Error> for MarrowError {
    fn from(e: marrow_core::Error) -> Self {
        MarrowError::Failed { reason: e.to_string() }
    }
}

impl From<std::io::Error> for MarrowError {
    fn from(e: std::io::Error) -> Self {
        MarrowError::Failed { reason: e.to_string() }
    }
}

pub(crate) fn fail(msg: impl Into<String>) -> MarrowError {
    MarrowError::Failed { reason: msg.into() }
}

pub(crate) fn vp(s: &str) -> Result<VaultPath> {
    Ok(VaultPath::new(s.trim_matches('/'))?)
}

// --------------------------------------------------------------- callbacks

/// The app hears about changes here. Called from background threads.
#[uniffi::export(with_foreign)]
pub trait MarrowListener: Send + Sync {
    /// Files changed (saved here, by sync, or by another app). Paths are
    /// vault-relative; a folder's path means things inside it changed.
    fn vault_changed(&self, paths: Vec<String>);
    /// Sync started, finished or stopped.
    fn sync_changed(&self, status: SyncStatus);
    /// A password was entered or forgotten.
    fn locks_changed(&self);
}

/// Where the app keeps secrets: the iOS Keychain, or Android's encrypted
/// storage. Sync passwords, the sync passphrase and AI keys go here.
#[uniffi::export(with_foreign)]
pub trait SecretStore: Send + Sync {
    fn get(&self, id: String) -> Option<String>;
    fn set(&self, id: String, value: Option<String>);
}

// ------------------------------------------------------------------ object

/// One per app. Holds the open vault and everything around it.
#[derive(uniffi::Object)]
pub struct Marrow {
    /// The app's private folder: caches, settings, backups.
    pub(crate) data_dir: PathBuf,
    pub(crate) service: Mutex<Option<VaultService>>,
    pub(crate) locks: Locks,
    pub(crate) listener: Arc<Mutex<Option<Arc<dyn MarrowListener>>>>,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) sync: Mutex<Option<sync::Worker>>,
    /// Sign-ins waiting for the browser to come back, by their `state`.
    pub(crate) cloud_pending: Mutex<std::collections::HashMap<String, marrow_cloud::AuthStart>>,
    /// The phone's own sign-in (Google on Android).
    pub(crate) platform_tokens: Arc<Mutex<Option<Arc<dyn cloud::PlatformTokens>>>>,
}

impl Marrow {
    pub(crate) fn engine(&self) -> Result<EngineGuard<'_>> {
        let guard = self.service.lock().map_err(|_| fail("Marrow stopped working; restart it."))?;
        if guard.is_none() {
            return Err(MarrowError::NoVault);
        }
        Ok(EngineGuard { guard })
    }

    /// Runs `f` with the open vault's engine.
    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Engine) -> Result<T>) -> Result<T> {
        let g = self.engine()?;
        let svc = g.guard.as_ref().ok_or(MarrowError::NoVault)?;
        let mut e = svc.engine();
        f(&mut e)
    }

    pub(crate) fn root(&self) -> Result<PathBuf> {
        self.with(|e| Ok(e.vault().root().to_path_buf()))
    }

    pub(crate) fn tell(&self, events: Vec<EngineEvent>) {
        tell(&self.listener, events);
    }

    pub(crate) fn tell_locks(&self) {
        if let Some(l) = self.listener.lock().ok().and_then(|l| l.clone()) {
            l.locks_changed();
        }
    }

    pub(crate) fn backups(&self) -> marrow_core::BackupStore {
        marrow_core::BackupStore::new(self.data_dir.join("backups"))
    }

    pub(crate) fn cache_file(&self, root: &Path, ext: &str) -> PathBuf {
        let key = blake3_hex(root.to_string_lossy().as_bytes());
        self.data_dir.join("vaults").join(format!("{}.{ext}", &key[..16]))
    }
}

pub(crate) struct EngineGuard<'a> {
    guard: MutexGuard<'a, Option<VaultService>>,
}

fn blake3_hex(b: &[u8]) -> String {
    marrow_core::cache::hash_bytes(b)
}

/// The paths a batch of engine events touched.
pub(crate) fn touched(events: &[EngineEvent]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: &VaultPath| {
        let s = p.to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    };
    for e in events {
        match e {
            EngineEvent::Vault { event } => match event {
                VaultEvent::Created { path, .. } | VaultEvent::Modified { path } | VaultEvent::Deleted { path, .. } => add(path),
                VaultEvent::Renamed { from, to, .. } => {
                    add(from);
                    add(to);
                }
            },
            EngineEvent::MetadataChanged { path } => add(path),
            EngineEvent::LinksChanged { paths } => paths.iter().for_each(&mut add),
        }
    }
    out
}

pub(crate) fn tell(listener: &Arc<Mutex<Option<Arc<dyn MarrowListener>>>>, events: Vec<EngineEvent>) {
    if events.is_empty() {
        return;
    }
    let paths = touched(&events);
    if let Some(l) = listener.lock().ok().and_then(|l| l.clone()) {
        l.vault_changed(paths);
    }
}

#[uniffi::export]
impl Marrow {
    /// `data_dir`: a private folder for Marrow's own files (caches,
    /// settings); never inside a vault.
    #[uniffi::constructor]
    pub fn new(data_dir: String, secrets: Arc<dyn SecretStore>) -> Arc<Self> {
        let data_dir = PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        Arc::new(Marrow {
            data_dir,
            service: Mutex::new(None),
            locks: Locks::default(),
            listener: Arc::new(Mutex::new(None)),
            secrets,
            sync: Mutex::new(None),
            cloud_pending: Mutex::new(Default::default()),
            platform_tokens: Arc::new(Mutex::new(None)),
        })
    }

    pub fn set_listener(&self, listener: Arc<dyn MarrowListener>) {
        if let Ok(mut l) = self.listener.lock() {
            *l = Some(listener);
        }
    }

    /// Marrow's version, e.g. "1.4.0".
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Keeps secrets in memory.
    #[derive(Default)]
    pub struct Memory(Mutex<std::collections::HashMap<String, String>>);

    impl SecretStore for Memory {
        fn get(&self, id: String) -> Option<String> {
            self.0.lock().unwrap().get(&id).cloned()
        }
        fn set(&self, id: String, value: Option<String>) {
            let mut m = self.0.lock().unwrap();
            match value {
                Some(v) => m.insert(id, v),
                None => m.remove(&id),
            };
        }
    }

    pub fn marrow(dir: &Path) -> Arc<Marrow> {
        Marrow::new(dir.join("data").to_string_lossy().into(), Arc::new(Memory::default()))
    }

    /// A vault in `dir/vault` with these notes, open.
    pub fn vault(dir: &Path, notes: &[(&str, &str)]) -> Arc<Marrow> {
        let root = dir.join("vault");
        for (p, t) in notes {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, t).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        let m = marrow(dir);
        m.open_vault(root.to_string_lossy().into()).unwrap();
        m
    }
}
