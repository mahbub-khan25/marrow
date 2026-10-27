//! Password-locked notes and folders: the window's commands. The logic is
//! shared with the phone apps in `marrow_core::vault_locks`.

use marrow_core::vault_locks as vl;
use marrow_core::{EngineEvent, VaultPath};
use tauri::{AppHandle, Emitter, State};

pub use marrow_core::vault_locks::{check_move, conceal, moved, moving, needs_lock, reveal, tidy, LockInfo, LockedPaths, Locks};

use crate::commands::{emit_events, vp, with_engine, AppState, CmdResult};

/// Tells the window when some readable copies couldn't be removed.
fn report(app: &AppHandle, problems: &[String]) {
    if let Some(n) = vl::problems_notice(problems) {
        log::warn!("lock: {} readable copies left", problems.len());
        let _ = app.emit("notice", format!("{n} If this keeps happening, see Settings → About → Show the log."));
    }
}

/// Before moving `from` to `to`: seals notes that will land in a locked
/// folder and wipes their readable copies (see [`vl::seal_for_move`]).
pub fn seal_before_move(
    app: &AppHandle,
    locks: &Locks,
    e: &mut marrow_core::Engine,
    from: &VaultPath,
    to: &VaultPath,
) -> CmdResult<Vec<EngineEvent>> {
    let store = crate::commands::backup_store(app).ok();
    let (events, sealed) = vl::seal_for_move(locks, e, from, to, store.as_ref())?;
    let problems = vl::wipe_plain_copies(e, &sealed, None);
    report(app, &problems);
    Ok(events)
}

/// Whether a note (or folder) is locked, and whether it's open this session.
#[tauri::command(async)]
pub fn lock_info(state: State<AppState>, locks: State<Locks>, path: String) -> CmdResult<LockInfo> {
    let p = vp(&path)?;
    with_engine(&state, |e| vl::lock_info(&locks, e, &p))
}

/// Every locked note and folder, for the file tree's lock icons.
#[tauri::command(async)]
pub fn locked_paths(state: State<AppState>) -> CmdResult<LockedPaths> {
    with_engine(&state, vl::locked_paths)
}

/// Tries a password on a note or a locked folder; keeps the key if it fits.
#[tauri::command(async)]
pub fn unlock(app: AppHandle, state: State<AppState>, locks: State<Locks>, path: String, password: String) -> CmdResult<()> {
    let p = vp(&path)?;
    with_engine(&state, |e| vl::unlock(&locks, e, &p, &password))?;
    let _ = app.emit("locks-changed", ());
    Ok(())
}

/// Locks one note with a new password.
#[tauri::command(async)]
pub fn lock_note(
    app: AppHandle,
    state: State<AppState>,
    locks: State<Locks>,
    path: String,
    password: String,
    hint: Option<String>,
) -> CmdResult<()> {
    let p = vp(&path)?;
    let store = crate::commands::backup_store(&app).ok();
    let done = with_engine(&state, |e| vl::lock_note(&locks, e, &p, &password, hint.as_deref(), store.as_ref()))?;
    report(&app, &done.problems);
    emit_events(&app, done.events, Some(&p));
    let _ = app.emit("locks-changed", ());
    Ok(())
}

/// Locks every note in a folder (and its subfolders) with one password;
/// notes added later are locked too. Returns how many notes were locked.
#[tauri::command(async)]
pub fn lock_folder(
    app: AppHandle,
    state: State<AppState>,
    locks: State<Locks>,
    path: String,
    password: String,
    hint: Option<String>,
) -> CmdResult<usize> {
    let folder = vp(&path)?;
    let store = crate::commands::backup_store(&app).ok();
    let done = with_engine(&state, |e| vl::lock_folder(&locks, e, &folder, &password, hint.as_deref(), store.as_ref()))?;
    report(&app, &done.problems);
    emit_events(&app, done.events, None);
    let _ = app.emit("locks-changed", ());
    Ok(done.count)
}

/// Removes the lock from a note, or from a folder and every note in it
/// locked with the folder's password. The password must have been entered.
#[tauri::command(async)]
pub fn remove_lock(app: AppHandle, state: State<AppState>, locks: State<Locks>, path: String) -> CmdResult<usize> {
    let p = vp(&path)?;
    let (events, count) = with_engine(&state, |e| vl::remove_lock(&locks, e, &p))?;
    emit_events(&app, events, None);
    let _ = app.emit("locks-changed", ());
    Ok(count)
}

/// Forgets every password entered this session.
#[tauri::command(async)]
pub fn lock_now(app: AppHandle, locks: State<Locks>) {
    locks.forget_all();
    let _ = app.emit("locks-changed", ());
}
