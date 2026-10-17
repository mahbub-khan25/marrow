//! Password-locked notes and folders, on the app side.
//!
//! Keys live only in memory, for this session, and are forgotten by
//! *Lock all now*, after the idle time set in Settings, and when Marrow
//! closes. Everything that reads or writes a note goes through [`reveal`]
//! and [`conceal`], so the window only ever sees plain text and the disk
//! only ever gets sealed text for a locked note.
//!
//! Locked folders are listed in `<vault>/.marrow/locked.json` with a small
//! sealed "check" text, so the folder's password can be tested (and new
//! notes in it sealed) even when it holds no notes yet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use marrow_core::lock::{self, Key};
use marrow_core::VaultPath;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::{emit_events, err, vp, with_engine, AppState, CmdResult};

/// The error the window recognises: "ask for the password, then retry".
pub const LOCKED: &str = "LOCKED";
/// The same for a new note in a locked folder: `LOCKED_FOLDER:<folder>`.
pub const LOCKED_FOLDER: &str = "LOCKED_FOLDER:";
const CHECK: &str = "marrow";

#[derive(Default)]
pub struct Locks {
    /// Keys by the salt they were made with.
    keys: Mutex<HashMap<String, Key>>,
}

impl Locks {
    fn key(&self, salt_id: &str) -> Option<Key> {
        self.keys.lock().ok()?.get(salt_id).cloned()
    }

    fn keep(&self, key: Key) {
        if let Ok(mut k) = self.keys.lock() {
            k.insert(key.salt_id(), key);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut k) = self.keys.lock() {
            k.clear();
        }
    }
}

// ------------------------------------------------------------ folder list

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    folders: Vec<LockedFolder>,
}

#[derive(Serialize, Deserialize, Clone)]
struct LockedFolder {
    path: String,
    /// "marrow" sealed with the folder's key.
    check: String,
}

fn registry_file(root: &Path) -> PathBuf {
    root.join(".marrow").join("locked.json")
}

fn load(root: &Path) -> Registry {
    std::fs::read_to_string(registry_file(root)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn store(root: &Path, r: &Registry) -> CmdResult<()> {
    let f = registry_file(root);
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    marrow_core::atomic::write(f, serde_json::to_string_pretty(r).map_err(err)?).map_err(err)
}

/// The locked folder holding `path` (or `path` itself), innermost first.
fn folder_for(reg: &Registry, path: &str) -> Option<LockedFolder> {
    reg.folders
        .iter()
        .filter(|f| path == f.path || path.starts_with(&format!("{}/", f.path)) || f.path.is_empty())
        .max_by_key(|f| f.path.len())
        .cloned()
}

/// Keeps the folder list in step with a rename or move.
pub fn moved(root: &Path, from: &str, to: &str) {
    let mut reg = load(root);
    let mut changed = false;
    for f in &mut reg.folders {
        if f.path == from {
            f.path = to.to_string();
            changed = true;
        } else if let Some(rest) = f.path.strip_prefix(&format!("{from}/")) {
            f.path = format!("{to}/{rest}");
            changed = true;
        }
    }
    if changed {
        let _ = store(root, &reg);
    }
}

// ------------------------------------------------------ reading & writing

/// Plain text of a note: unchanged if it isn't locked, decrypted if its key
/// is known this session, otherwise the `LOCKED` error.
pub fn reveal(locks: &Locks, text: String) -> CmdResult<String> {
    if !lock::is_locked(&text) {
        return Ok(text);
    }
    let h = lock::header(&text).map_err(err)?;
    let key = locks.key(&h.salt_id()).ok_or_else(|| LOCKED.to_string())?;
    lock::open(&text, &key).map_err(|e| match e {
        lock::LockError::WrongPassword => LOCKED.to_string(),
        e => e.to_string(),
    })
}

/// What to write for `plain` at `path`, where `disk` is the file now:
/// sealed for a locked note or a note in a locked folder.
pub fn conceal(locks: &Locks, root: &Path, path: &str, disk: Option<&str>, plain: &str) -> CmdResult<String> {
    if let Some(d) = disk.filter(|d| lock::is_locked(d)) {
        let h = lock::header(d).map_err(err)?;
        let key = locks.key(&h.salt_id()).ok_or_else(|| LOCKED.to_string())?;
        return Ok(lock::seal(plain, &key, h.hint.as_deref()));
    }
    if let Some(f) = folder_for(&load(root), path) {
        let h = lock::header(&f.check).map_err(err)?;
        return match locks.key(&h.salt_id()) {
            Some(key) => Ok(lock::seal(plain, &key, h.hint.as_deref())),
            // A new, empty note can be made; its text waits for the password.
            None if plain.trim().is_empty() => Ok(plain.to_string()),
            None => Err(format!("{LOCKED_FOLDER}{}", f.path)),
        };
    }
    Ok(plain.to_string())
}

// --------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    /// The file on disk is sealed.
    pub locked: bool,
    /// Its key (or its folder's) is known this session.
    pub unlocked: bool,
    pub hint: Option<String>,
    /// The locked folder it's in (or is), if any.
    pub folder: Option<String>,
}

#[derive(Serialize)]
pub struct LockedPaths {
    notes: Vec<String>,
    folders: Vec<String>,
}

/// Whether a note (or folder) is locked, and whether it's open this session.
#[tauri::command]
pub fn lock_info(state: State<AppState>, locks: State<Locks>, path: String) -> CmdResult<LockInfo> {
    let p = vp(&path)?;
    with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        let folder = folder_for(&load(&root), p.as_str());
        let disk = if p.is_markdown() { e.read_note(&p).ok() } else { None };
        let h = match disk.as_deref().filter(|d| lock::is_locked(d)) {
            Some(d) => Some(lock::header(d).map_err(err)?),
            None => match &folder {
                Some(f) => lock::header(&f.check).ok(),
                None => None,
            },
        };
        Ok(LockInfo {
            locked: disk.as_deref().is_some_and(lock::is_locked),
            unlocked: h.as_ref().is_some_and(|h| locks.key(&h.salt_id()).is_some()),
            hint: h.and_then(|h| h.hint),
            folder: folder.map(|f| f.path),
        })
    })
}

/// Every locked note and folder, for the file tree's lock icons.
#[tauri::command]
pub fn locked_paths(state: State<AppState>) -> CmdResult<LockedPaths> {
    with_engine(&state, |e| {
        let folders = load(e.vault().root()).folders.into_iter().map(|f| f.path).collect();
        let notes = e.locked_notes().into_iter().map(|p| p.to_string()).collect();
        Ok(LockedPaths { notes, folders })
    })
}

/// Tries a password on a note or a locked folder; keeps the key if it fits.
#[tauri::command]
pub fn unlock(app: AppHandle, state: State<AppState>, locks: State<Locks>, path: String, password: String) -> CmdResult<()> {
    let p = vp(&path)?;
    let sealed = with_engine(&state, |e| {
        if p.is_markdown() {
            if let Ok(t) = e.read_note(&p) {
                if lock::is_locked(&t) {
                    return Ok(t);
                }
            }
        }
        folder_for(&load(e.vault().root()), p.as_str())
            .map(|f| f.check)
            .ok_or_else(|| "This isn't locked.".to_string())
    })?;
    let (_, key) = lock::open_with_password(&sealed, &password).map_err(err)?;
    locks.keep(key);
    let _ = app.emit("locks-changed", ());
    Ok(())
}

/// Locks one note with a new password.
#[tauri::command]
pub fn lock_note(
    app: AppHandle,
    state: State<AppState>,
    locks: State<Locks>,
    path: String,
    password: String,
    hint: Option<String>,
) -> CmdResult<()> {
    let p = vp(&path)?;
    check_password(&password)?;
    let key = lock::derive(&password, lock::new_salt(), lock::Cost::default()).map_err(err)?;
    let events = with_engine(&state, |e| {
        let text = e.read_note(&p).map_err(err)?;
        if lock::is_locked(&text) {
            return Err("This note is already locked.".into());
        }
        let ev = e.write_note(&p, &lock::seal(&text, &key, hint.as_deref())).map_err(err)?;
        // Earlier versions would still be readable: remove them.
        let _ = marrow_core::history::History::for_vault(e.vault().root()).forget(p.as_str());
        Ok(ev)
    })?;
    locks.keep(key);
    emit_events(&app, events, Some(&p));
    let _ = app.emit("locks-changed", ());
    Ok(())
}

/// Locks every note in a folder (and its subfolders) with one password;
/// notes added later are locked too. Returns how many notes were locked.
#[tauri::command]
pub fn lock_folder(
    app: AppHandle,
    state: State<AppState>,
    locks: State<Locks>,
    path: String,
    password: String,
    hint: Option<String>,
) -> CmdResult<usize> {
    let folder = vp(&path)?;
    check_password(&password)?;
    let key = lock::derive(&password, lock::new_salt(), lock::Cost::default()).map_err(err)?;
    let (events, count) = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        let mut reg = load(&root);
        if let Some(f) = folder_for(&reg, folder.as_str()) {
            return Err(format!("“{}” is already locked.", if f.path.is_empty() { "The vault" } else { &f.path }));
        }
        let notes: Vec<VaultPath> = e.vault().markdown_files().filter(|n| n.starts_with(&folder)).cloned().collect();
        let history = marrow_core::history::History::for_vault(&root);
        let mut events = Vec::new();
        let mut count = 0;
        for n in &notes {
            let Ok(text) = e.read_note(n) else { continue };
            if lock::is_locked(&text) {
                continue;
            }
            events.extend(e.write_note(n, &lock::seal(&text, &key, hint.as_deref())).map_err(err)?);
            let _ = history.forget(n.as_str());
            count += 1;
        }
        reg.folders.push(LockedFolder {
            path: folder.to_string(),
            check: lock::seal(CHECK, &key, hint.as_deref()),
        });
        store(&root, &reg)?;
        Ok((events, count))
    })?;
    locks.keep(key);
    emit_events(&app, events, None);
    let _ = app.emit("locks-changed", ());
    Ok(count)
}

/// Removes the lock from a note, or from a folder and every note in it
/// locked with the folder's password. The password must have been entered.
#[tauri::command]
pub fn remove_lock(app: AppHandle, state: State<AppState>, locks: State<Locks>, path: String) -> CmdResult<usize> {
    let p = vp(&path)?;
    let (events, count) = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        let mut reg = load(&root);
        let mut events = Vec::new();
        let mut count = 0;
        let mut open_note = |e: &mut marrow_core::Engine, n: &VaultPath, only_salt: Option<&str>| -> CmdResult<()> {
            let text = e.read_note(n).map_err(err)?;
            if !lock::is_locked(&text) {
                return Ok(());
            }
            let h = lock::header(&text).map_err(err)?;
            if only_salt.is_some_and(|s| s != h.salt_id()) {
                return Ok(());
            }
            let key = locks.key(&h.salt_id()).ok_or_else(|| LOCKED.to_string())?;
            let plain = lock::open(&text, &key).map_err(err)?;
            events.extend(e.write_note(n, &plain).map_err(err)?);
            count += 1;
            Ok(())
        };
        if let Some(i) = reg.folders.iter().position(|f| f.path == p.as_str()) {
            let h = lock::header(&reg.folders[i].check).map_err(err)?;
            let salt = h.salt_id();
            if locks.key(&salt).is_none() {
                return Err(format!("{LOCKED_FOLDER}{}", p.as_str()));
            }
            let notes: Vec<VaultPath> = e.vault().markdown_files().filter(|n| n.starts_with(&p)).cloned().collect();
            for n in &notes {
                open_note(e, n, Some(&salt))?;
            }
            reg.folders.remove(i);
            store(&root, &reg)?;
        } else {
            if let Some(f) = folder_for(&reg, p.as_str()) {
                return Err(format!(
                    "This note is in the locked folder “{}”. Move it out of the folder first, or remove the folder's lock.",
                    f.path
                ));
            }
            open_note(e, &p, None)?;
        }
        Ok((events, count))
    })?;
    emit_events(&app, events, None);
    let _ = app.emit("locks-changed", ());
    Ok(count)
}

/// Forgets every password entered this session.
#[tauri::command]
pub fn lock_now(app: AppHandle, locks: State<Locks>) {
    locks.forget_all();
    let _ = app.emit("locks-changed", ());
}

fn check_password(pw: &str) -> CmdResult<()> {
    if pw.chars().count() < 6 {
        return Err("Use a password of at least 6 characters.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_lookup_prefers_the_innermost() {
        let reg = Registry {
            folders: vec![
                LockedFolder { path: "A".into(), check: String::new() },
                LockedFolder { path: "A/B".into(), check: String::new() },
            ],
        };
        assert_eq!(folder_for(&reg, "A/B/n.md").unwrap().path, "A/B");
        assert_eq!(folder_for(&reg, "A/n.md").unwrap().path, "A");
        assert_eq!(folder_for(&reg, "A").unwrap().path, "A");
        assert!(folder_for(&reg, "AB/n.md").is_none());
    }

    #[test]
    fn conceal_and_reveal() {
        let dir = tempfile::tempdir().unwrap();
        let locks = Locks::default();
        let key = lock::derive("secret", [3u8; 16], lock::Cost { m: 64, t: 1, p: 1 }).unwrap();
        let sealed = lock::seal("old", &key, Some("h"));
        // Without the key: refused both ways.
        assert_eq!(reveal(&locks, sealed.clone()), Err(LOCKED.into()));
        assert_eq!(conceal(&locks, dir.path(), "n.md", Some(&sealed), "new"), Err(LOCKED.into()));
        locks.keep(key.clone());
        assert_eq!(reveal(&locks, sealed.clone()).unwrap(), "old");
        let next = conceal(&locks, dir.path(), "n.md", Some(&sealed), "new").unwrap();
        assert_eq!(reveal(&locks, next.clone()).unwrap(), "new");
        assert_eq!(lock::header(&next).unwrap().hint.as_deref(), Some("h"));
        // Plain notes pass through.
        assert_eq!(conceal(&locks, dir.path(), "p.md", Some("x"), "y").unwrap(), "y");
        // A locked folder seals new notes, and refuses text without the key.
        store(dir.path(), &Registry { folders: vec![LockedFolder { path: "F".into(), check: lock::seal(CHECK, &key, None) }] }).unwrap();
        assert!(lock::is_locked(&conceal(&locks, dir.path(), "F/new.md", None, "hello").unwrap()));
        locks.forget_all();
        assert_eq!(conceal(&locks, dir.path(), "F/new.md", None, "hello"), Err("LOCKED_FOLDER:F".into()));
        assert_eq!(conceal(&locks, dir.path(), "F/new.md", None, "  ").unwrap(), "  ");
        moved(dir.path(), "F", "G");
        assert_eq!(load(dir.path()).folders[0].path, "G");
    }
}
