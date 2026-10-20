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
use marrow_core::mirror::{self, Mirror, SyncReport, SyncStop};
use marrow_core::merge2;
use serde::Serialize;
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

/// Starts syncing `local` with `remote` (stopping any earlier sync).
pub fn start_mirror(app: &AppHandle, state: &State<AppState>, local: &Path, remote: PathBuf) {
    stop_mirror(state);
    let (tx, rx) = channel::<Msg>();
    let label = remote
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| remote.to_string_lossy().into_owned());
    let status = Arc::new(Mutex::new(MirrorStatus {
        remote: remote.to_string_lossy().into_owned(),
        label,
        ..Default::default()
    }));
    let worker_status = status.clone();
    let app = app.clone();
    let local = local.to_path_buf();
    std::thread::spawn(move || {
        // Name the provider (looking for cloud folders can be slow, so it's done here).
        if let Some(l) = cloud::provider_of(&remote, &cloud::detect()) {
            if let Ok(mut s) = worker_status.lock() {
                s.label = l.label.clone();
                s.provider = Some(l.provider.clone());
            }
        }
        let m = Mirror::new(&local, &remote);
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
            settings.mirrors.insert(key, r.to_string_lossy().into_owned());
            save_settings(&app, &settings)?;
            start_mirror(&app, &state, &root, r);
        }
        None => {
            stop_mirror(&state);
            if let Some(old) = settings.mirrors.remove(&key) {
                Mirror::new(&root, old).reset();
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
