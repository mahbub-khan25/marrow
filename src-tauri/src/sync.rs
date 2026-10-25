//! Cloud folders, vaults created in them, two-way sync between a vault and
//! a cloud folder, and merging the conflict copies sync apps make.
//!
//! Sync runs on its own thread: once when it starts, every 15 seconds, and
//! shortly after each save. It only touches files, so it never takes the
//! engine lock; the vault's watcher sees what it copies in, like any other
//! outside change.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use marrow_core::cloud::{self, CloudLocation};
use marrow_core::conflicts::{self, ConflictCopy};
use marrow_core::mirror::{self, Mirror, Remote, SyncReport, SyncStop};
use marrow_core::merge2;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::{emit_events, err, load_settings, save_settings, vp, with_engine, AppState, CmdResult};

const INTERVAL: Duration = Duration::from_secs(15);
/// Saves arriving close together are synced together.
const SETTLE: Duration = Duration::from_millis(1500);

enum Msg {
    Poke { force: bool },
    Stop,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorStatus {
    /// The folder the vault is kept in sync with.
    pub remote: String,
    /// "Dropbox", "OneDrive"…, or the folder's name.
    pub label: String,
    /// "dropbox", "onedrive", "gdrive", "icloud", or none for a plain folder.
    pub provider: Option<String>,
    /// When the last sync finished (ms since 1970).
    pub last_sync: Option<u64>,
    pub syncing: bool,
    /// Why the last run stopped, if it did.
    pub stop: Option<SyncStop>,
    /// The same, in words.
    pub message: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MirrorEvent {
    status: MirrorStatus,
    report: Option<SyncReport>,
}

pub struct MirrorWorker {
    tx: Sender<Msg>,
    status: Arc<Mutex<MirrorStatus>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A cloud service a vault syncs with directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudTarget {
    /// Nextcloud, ownCloud, Synology, pCloud, Koofr and other WebDAV servers.
    #[serde(rename_all = "camelCase")]
    Webdav { url: String, user: String },
}

impl CloudTarget {
    /// Where its password lives in the credential store.
    fn secret_id(&self) -> String {
        match self {
            CloudTarget::Webdav { url, user } => format!("sync:webdav:{user}@{url}"),
        }
    }

    fn connect(&self, app: &AppHandle) -> Result<Box<dyn Remote>, String> {
        match self {
            CloudTarget::Webdav { url, user } => {
                let password = crate::ai::keys::get(app, &self.secret_id()).unwrap_or_default();
                Ok(Box::new(crate::webdav::WebDav::new(url, user, &password)?))
            }
        }
    }
}

/// Starts syncing `local` with the folder `remote` (stopping any earlier sync).
pub fn start_mirror(app: &AppHandle, state: &State<AppState>, local: &Path, remote: PathBuf) {
    let label = remote
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| remote.to_string_lossy().into_owned());
    let status = MirrorStatus { remote: remote.to_string_lossy().into_owned(), label, ..Default::default() };
    start_worker(app, state, local, status, move |status| {
        // Name the provider (looking for cloud folders can be slow, so it's done here).
        if let Some(l) = cloud::provider_of(&remote, &cloud::detect()) {
            if let Ok(mut s) = status.lock() {
                s.label = l.label.clone();
                s.provider = Some(l.provider.clone());
            }
        }
        Ok(Box::new(mirror::FolderRemote::new(remote)))
    });
}

/// Starts syncing `local` with a cloud service (stopping any earlier sync).
pub fn start_cloud(app: &AppHandle, state: &State<AppState>, local: &Path, target: CloudTarget) {
    let (remote, label, provider) = match &target {
        CloudTarget::Webdav { url, .. } => {
            let host = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| url.clone());
            (url.clone(), host, "webdav")
        }
    };
    let status = MirrorStatus { remote, label, provider: Some(provider.into()), ..Default::default() };
    let app2 = app.clone();
    start_worker(app, state, local, status, move |_| target.connect(&app2));
}

fn start_worker(
    app: &AppHandle,
    state: &State<AppState>,
    local: &Path,
    status: MirrorStatus,
    make: impl FnOnce(&Arc<Mutex<MirrorStatus>>) -> Result<Box<dyn Remote>, String> + Send + 'static,
) {
    stop_mirror(state);
    let (tx, rx) = channel::<Msg>();
    let status = Arc::new(Mutex::new(status));
    let worker_status = status.clone();
    let app = app.clone();
    let local = local.to_path_buf();
    std::thread::spawn(move || {
        let m = match make(&worker_status) {
            Ok(remote) => Mirror::with_remote(&local, remote),
            Err(e) => {
                if let Ok(mut s) = worker_status.lock() {
                    let stop = SyncStop::Io { message: e };
                    s.message = Some(stop.to_string());
                    s.stop = Some(stop);
                    let _ = app.emit("mirror-changed", MirrorEvent { status: s.clone(), report: None });
                }
                return;
            }
        };
        let emit = |report: Option<SyncReport>| {
            if let Ok(s) = worker_status.lock() {
                let _ = app.emit(
                    "mirror-changed",
                    MirrorEvent {
                        status: s.clone(),
                        report,
                    },
                );
            }
        };
        let mut force = false;
        loop {
            if let Ok(mut s) = worker_status.lock() {
                s.syncing = true;
            }
            let result = m.sync(force);
            force = false;
            let report = {
                let mut s = match worker_status.lock() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                s.syncing = false;
                match result {
                    Ok(r) => {
                        s.last_sync = Some(now_ms());
                        s.stop = None;
                        s.message = None;
                        Some(r)
                    }
                    Err(stop) => {
                        s.message = Some(stop.to_string());
                        s.stop = Some(stop);
                        None
                    }
                }
            };
            emit(report);

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
    });
    if let Ok(mut slot) = state.mirror.lock() {
        *slot = Some(MirrorWorker { tx, status });
    }
}

pub fn stop_mirror(state: &State<AppState>) {
    if let Ok(mut slot) = state.mirror.lock() {
        if let Some(w) = slot.take() {
            let _ = w.tx.send(Msg::Stop);
        }
    }
}

/// Asks for a sync soon (after a save).
pub fn poke(state: &State<AppState>) {
    if let Ok(slot) = state.mirror.lock() {
        if let Some(w) = slot.as_ref() {
            let _ = w.tx.send(Msg::Poke { force: false });
        }
    }
}

fn vault_root(state: &State<AppState>) -> CmdResult<PathBuf> {
    with_engine(state, |e| Ok(e.vault().root().to_path_buf()))
}

// ------------------------------------------------------------ commands

/// Cloud folders on this computer (Dropbox, OneDrive, Google Drive, iCloud).
#[tauri::command]
pub async fn cloud_locations() -> Vec<CloudLocation> {
    cloud::detect()
}

/// Makes a new, empty vault folder `name` inside `parent` and returns its path.
#[tauri::command(async)]
pub fn create_vault(parent: String, name: String) -> CmdResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the vault a name.".into());
    }
    if name.starts_with('.') || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("A vault name can't start with a dot or contain / \\ : * ? \" < > |".into());
    }
    let parent = PathBuf::from(parent);
    if !parent.is_dir() {
        std::fs::create_dir_all(&parent).map_err(|e| format!("Couldn't make {}: {e}", parent.display()))?;
    }
    let dir = parent.join(name);
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("There's already a folder called “{name}” there. Pick another name, or open that folder instead."));
    }
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.to_string_lossy().into_owned())
}

/// The cloud folder the open vault is inside, if any.
#[tauri::command]
pub async fn vault_cloud(state: State<'_, AppState>) -> CmdResult<Option<CloudLocation>> {
    let root = vault_root(&state)?;
    Ok(cloud::provider_of(&root, &cloud::detect()).cloned())
}

#[tauri::command(async)]
pub fn mirror_status(state: State<AppState>) -> Option<MirrorStatus> {
    let slot = state.mirror.lock().ok()?;
    let w = slot.as_ref()?;
    let s = w.status.lock().ok()?.clone();
    Some(s)
}

/// How many files a folder already has, before syncing with it.
#[tauri::command]
pub async fn mirror_preview(remote: String) -> usize {
    mirror::count_files(Path::new(&remote))
}

/// Talks to a WebDAV server off the async threads (its client blocks).
async fn on_blocking_thread<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

/// Checks a WebDAV server before syncing with it: signs in, makes the sync
/// folder if needed, and says how many files are already there.
#[tauri::command]
pub async fn webdav_check(url: String, user: String, password: String) -> CmdResult<usize> {
    on_blocking_thread(move || {
        let dav = crate::webdav::WebDav::new(&url, &user, &password)?;
        dav.check().map_err(|e| e.to_string())?;
        dav.list().map(|l| l.len()).map_err(err)
    })
    .await
}

/// Turns on sync with a WebDAV server (turning off any other sync for this vault).
#[tauri::command]
pub async fn set_cloud_sync(app: AppHandle, state: State<'_, AppState>, url: String, user: String, password: String) -> CmdResult<()> {
    let root = vault_root(&state)?;
    let key = root.to_string_lossy().into_owned();
    let (u, n, p) = (url.clone(), user.clone(), password.clone());
    on_blocking_thread(move || {
        let dav = crate::webdav::WebDav::new(&u, &n, &p)?;
        dav.check().map_err(|e| e.to_string())
    })
    .await?;
    let target = CloudTarget::Webdav { url: url.trim().to_string(), user: user.trim().to_string() };
    crate::ai::keys::set(&app, &target.secret_id(), Some(&password))?;
    let mut settings = load_settings(&app);
    if let Some(old) = settings.mirrors.remove(&key) {
        Mirror::new(&root, old).reset();
    }
    settings.clouds.insert(key, target.clone());
    save_settings(&app, &settings)?;
    start_cloud(&app, &state, &root, target);
    Ok(())
}

/// Turns sync on with `remote`, or off with `None`.
#[tauri::command(async)]
pub fn set_mirror(app: AppHandle, state: State<AppState>, remote: Option<String>) -> CmdResult<()> {
    let root = vault_root(&state)?;
    let key = root.to_string_lossy().into_owned();
    let mut settings = load_settings(&app);
    match remote {
        Some(remote) => {
            let remote = PathBuf::from(remote);
            std::fs::create_dir_all(&remote).map_err(err)?;
            let r = dunce::canonicalize(&remote).map_err(err)?;
            let l = dunce::canonicalize(&root).map_err(err)?;
            if r.starts_with(&l) || l.starts_with(&r) {
                return Err("Pick a folder outside this vault (and not one that contains it).".into());
            }
            settings.mirrors.insert(key.clone(), r.to_string_lossy().into_owned());
            if let Some(old) = settings.clouds.remove(&key) {
                let _ = crate::ai::keys::set(&app, &old.secret_id(), None);
                Mirror::new(&root, "").reset();
            }
            save_settings(&app, &settings)?;
            start_mirror(&app, &state, &root, r);
        }
        None => {
            stop_mirror(&state);
            if let Some(old) = settings.mirrors.remove(&key) {
                Mirror::new(&root, old).reset();
            }
            if let Some(old) = settings.clouds.remove(&key) {
                let _ = crate::ai::keys::set(&app, &old.secret_id(), None);
                Mirror::new(&root, "").reset();
            }
            save_settings(&app, &settings)?;
        }
    }
    Ok(())
}

/// Syncs now. `force` goes ahead with a large delete that was paused.
#[tauri::command(async)]
pub fn sync_now(state: State<AppState>, force: bool) {
    if let Ok(slot) = state.mirror.lock() {
        if let Some(w) = slot.as_ref() {
            let _ = w.tx.send(Msg::Poke { force });
        }
    }
}

/// Conflict copies a sync app left in the vault.
#[tauri::command(async)]
pub fn conflict_copies(state: State<AppState>) -> CmdResult<Vec<ConflictCopy>> {
    with_engine(&state, |e| {
        let paths: Vec<String> = e.vault().markdown_files().map(|p| p.to_string()).collect();
        Ok(conflicts::find(&paths))
    })
}

/// Merges a conflict copy into its original and moves the copy to the trash.
/// Returns how many places both versions were kept.
#[tauri::command(async)]
pub fn resolve_conflict(
    app: AppHandle,
    state: State<AppState>,
    locks: State<crate::locks::Locks>,
    copy: String,
    original: String,
) -> CmdResult<usize> {
    let (cp, op) = (vp(&copy)?, vp(&original)?);
    let (conflicts, events) = with_engine(&state, |e| {
        // Locked notes are merged as plain text (needs the password) and sealed again.
        let raw = e.read_note(&op).map_err(err)?;
        let a = crate::locks::reveal(&locks, raw.clone())?;
        let b = crate::locks::reveal(&locks, e.read_note(&cp).map_err(err)?)?;
        let merged = merge2(&a, &b);
        let mut events = vec![];
        if merged.text != a {
            let text = crate::locks::conceal(&locks, e.vault().root(), op.as_str(), Some(&raw), &merged.text)?;
            events.extend(e.write_note(&op, &text).map_err(err)?);
        }
        events.extend(e.trash(&cp).map_err(err)?);
        Ok((merged.conflicts, events))
    })?;
    emit_events(&app, events, None);
    poke(&state);
    Ok(conflicts)
}
