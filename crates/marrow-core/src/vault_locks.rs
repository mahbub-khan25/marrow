//! Password-locked notes and folders: which are locked, and the keys
//! entered this session. Shared by the desktop and phone apps.
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

use serde::{Deserialize, Serialize};

use crate::lock::{self, Key};
use crate::{BackupStore, Engine, EngineEvent, VaultPath};

/// Errors are words for the person (or the [`LOCKED`] markers).
pub type LockResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// The error the window recognises: "ask for the password, then retry".
pub const LOCKED: &str = "LOCKED";
/// The same for a new note in a locked folder: `LOCKED_FOLDER:<folder>`.
pub const LOCKED_FOLDER: &str = "LOCKED_FOLDER:";
pub const CHECK: &str = "marrow";

#[derive(Default)]
pub struct Locks {
    /// Keys by the salt they were made with.
    keys: Mutex<HashMap<String, Key>>,
}

impl Locks {
    pub fn key(&self, salt_id: &str) -> Option<Key> {
        self.keys.lock().ok()?.get(salt_id).cloned()
    }

    pub fn keep(&self, key: Key) {
        if let Ok(mut k) = self.keys.lock() {
            k.insert(key.salt_id(), key);
        }
    }

    pub fn forget_all(&self) {
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

/// The locked-folder list. A missing file means no locked folders. A file
/// that can't be read or understood is an error, never "no locked folders":
/// guessing wrong would save notes in a locked folder unencrypted.
fn load(root: &Path) -> LockResult<Registry> {
    let f = registry_file(root);
    match std::fs::read_to_string(&f) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| REGISTRY_BROKEN.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Registry::default()),
        Err(e) => Err(format!("{REGISTRY_BROKEN} ({e})")),
    }
}

const REGISTRY_BROKEN: &str = "Marrow can't read its list of locked folders (.marrow/locked.json), so it won't save notes until that's fixed. Restore the file from a backup, or remove it if you have no locked folders.";

/// True when `path` must be stored sealed: the note on disk is locked, or it
/// is in a locked folder.
pub fn needs_lock(root: &Path, path: &str, disk: Option<&str>) -> LockResult<bool> {
    Ok(disk.is_some_and(lock::is_locked) || folder_for(&load(root)?, path).is_some())
}

/// Before moving `from` to `to`: notes arriving in a locked folder will be
/// sealed with its password, so it must have been entered.
pub fn check_move(locks: &Locks, root: &Path, from: &str, to: &str) -> LockResult<()> {
    let reg = load(root)?;
    if let Some(dest) = folder_for(&reg, to) {
        let same = folder_for(&reg, from).is_some_and(|f| f.path == dest.path);
        let h = lock::header(&dest.check).map_err(err)?;
        if !same && locks.key(&h.salt_id()).is_none() {
            return Err(format!("{LOCKED_FOLDER}{}", dest.path));
        }
    }
    Ok(())
}

/// Before moving `from` to `to`: notes that will land in a locked folder
/// are sealed where they are now, so they're never readable inside it, not
/// even for a moment or after a crash halfway through the move. Text kept
/// for them (failed saves, crash drafts) is sealed too. Returns the events
/// and the notes it sealed; pass those to [`wipe_plain_copies`].
pub fn seal_for_move(
    locks: &Locks,
    e: &mut Engine,
    from: &VaultPath,
    to: &VaultPath,
    store: Option<&BackupStore>,
) -> LockResult<(Vec<EngineEvent>, Vec<VaultPath>)> {
    let root = e.vault().root().to_path_buf();
    let reg = load(&root)?;
    let notes: Vec<VaultPath> = e.vault().markdown_files().filter(|n| *n == from || n.starts_with(from)).cloned().collect();
    let mut events = Vec::new();
    let mut sealed = Vec::new();
    for n in notes {
        let dest = format!("{}{}", to.as_str(), &n.as_str()[from.as_str().len()..]);
        let Some(f) = folder_for(&reg, &dest) else { continue };
        let text = e.read_note(&n).map_err(err)?;
        let kept = store.and_then(|s| s.get(&root, n.as_str())).filter(|b| !lock::is_locked(&b.text));
        // A note sealed by an earlier, interrupted move is still checked:
        // its kept text and readable history may not have been dealt with.
        if lock::is_locked(&text) && kept.is_none() {
            sealed.push(n);
            continue;
        }
        let h = lock::header(&f.check).map_err(err)?;
        let key = locks.key(&h.salt_id()).ok_or_else(|| format!("{LOCKED_FOLDER}{}", f.path))?;
        // Kept text first, so a failure here leaves everything as it was.
        if let (Some(s), Some(b)) = (store, kept) {
            if s.put(&root, n.as_str(), &lock::seal(&b.text, &key, h.hint.as_deref())).is_err() {
                return Err(format!("Couldn't protect the unsaved text kept for “{}”, so it wasn't moved.", n.name()));
            }
        }
        if !lock::is_locked(&text) {
            events.extend(e.write_note(&n, &lock::seal(&text, &key, h.hint.as_deref())).map_err(err)?);
        }
        sealed.push(n);
    }
    Ok((events, sealed))
}

fn store(root: &Path, r: &Registry) -> LockResult<()> {
    let f = registry_file(root);
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    crate::atomic::write(f, serde_json::to_string_pretty(r).map_err(err)?).map_err(err)
}

/// The locked folder holding `path` (or `path` itself), innermost first.
fn folder_for(reg: &Registry, path: &str) -> Option<LockedFolder> {
    reg.folders
        .iter()
        .filter(|f| path == f.path || path.starts_with(&format!("{}/", f.path)) || f.path.is_empty())
        .max_by_key(|f| f.path.len())
        .cloned()
}

/// Where a locked folder at `path` ends up when `from` moves to `to`.
fn mapped(path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        Some(to.to_string())
    } else {
        path.strip_prefix(&format!("{from}/")).map(|rest| format!("{to}/{rest}"))
    }
}

/// Before a rename or move: locked folders inside `from` are listed under
/// their new place too, so they stay locked whichever side of the move a
/// crash lands on. [`moved`] drops the old entries afterwards, and [`tidy`]
/// does it on the next open if Marrow stopped in between.
pub fn moving(root: &Path, from: &str, to: &str) -> LockResult<()> {
    let mut reg = load(root)?;
    let extra: Vec<LockedFolder> = reg
        .folders
        .iter()
        .filter_map(|f| mapped(&f.path, from, to).map(|path| LockedFolder { path, check: f.check.clone() }))
        .filter(|n| !reg.folders.iter().any(|f| f.path == n.path))
        .collect();
    if extra.is_empty() {
        return Ok(());
    }
    reg.folders.extend(extra);
    store(root, &reg)
}

/// Keeps the folder list in step with a finished rename or move.
pub fn moved(root: &Path, from: &str, to: &str) -> LockResult<()> {
    let mut reg = load(root)?;
    let before = reg.folders.len();
    let mut changed = false;
    let paths: Vec<String> = reg.folders.iter().map(|f| f.path.clone()).collect();
    reg.folders.retain_mut(|f| match mapped(&f.path, from, to) {
        // Already listed at its new place by `moving`.
        Some(new) if paths.contains(&new) => false,
        Some(new) => {
            f.path = new;
            changed = true;
            true
        }
        None => true,
    });
    if changed || reg.folders.len() != before {
        store(root, &reg)?;
    }
    Ok(())
}

/// On opening a vault: finishes a folder-list update a crash interrupted.
/// A locked folder listed twice (old and new place, same password) keeps
/// only the place that exists on disk. Nothing else is touched.
pub fn tidy(root: &Path) -> LockResult<()> {
    let mut reg = load(root)?;
    let exists = |p: &str| root.join(p).is_dir();
    let all = reg.folders.clone();
    let before = reg.folders.len();
    reg.folders
        .retain(|f| exists(&f.path) || !all.iter().any(|o| o.path != f.path && o.check == f.check && exists(&o.path)));
    if reg.folders.len() != before {
        store(root, &reg)?;
    }
    Ok(())
}

// ------------------------------------------------------ reading & writing

/// Plain text of a note: unchanged if it isn't locked, decrypted if its key
/// is known this session, otherwise the `LOCKED` error.
pub fn reveal(locks: &Locks, text: String) -> LockResult<String> {
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
pub fn conceal(locks: &Locks, root: &Path, path: &str, disk: Option<&str>, plain: &str) -> LockResult<String> {
    if let Some(d) = disk.filter(|d| lock::is_locked(d)) {
        let h = lock::header(d).map_err(err)?;
        let key = locks.key(&h.salt_id()).ok_or_else(|| LOCKED.to_string())?;
        return Ok(lock::seal(plain, &key, h.hint.as_deref()));
    }
    if let Some(f) = folder_for(&load(root)?, path) {
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

/// After notes are locked: removes their readable earlier versions, failed-
/// save backups and crash drafts (when `backups` is given), and the headings
/// and properties the search cache kept. Returns what couldn't be removed,
/// in words, since the person was promised the text is now private.
pub fn wipe_plain_copies(e: &mut Engine, notes: &[VaultPath], backups: Option<&BackupStore>) -> Vec<String> {
    let root = e.vault().root().to_path_buf();
    let history = crate::history::History::for_vault(&root);
    let mut problems = Vec::new();
    for n in notes {
        if let Err(x) = history.forget(n.as_str()) {
            problems.push(format!("earlier versions of “{}” ({x})", n.name()));
        }
        if let Some(s) = backups {
            if let Err(x) = s.remove(&root, n.as_str()) {
                problems.push(format!("unsaved text kept from “{}” ({x})", n.name()));
            }
        }
    }
    if let Err(x) = e.forget_old_metadata() {
        problems.push(format!("the search cache ({x})"));
    }
    problems
}

/// The words shown when some readable copies couldn't be removed.
pub fn problems_notice(problems: &[String]) -> Option<String> {
    (!problems.is_empty()).then(|| {
        format!(
            "Locked, but some readable copies made before couldn't be removed: {}. Close Marrow and lock it again.",
            problems.join("; ")
        )
    })
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct LockedPaths {
    pub notes: Vec<String>,
    pub folders: Vec<String>,
}

/// Whether a note (or folder) is locked, and whether it's open this session.
pub fn lock_info(locks: &Locks, e: &Engine, p: &VaultPath) -> LockResult<LockInfo> {
    let root = e.vault().root().to_path_buf();
    let folder = folder_for(&load(&root)?, p.as_str());
    let disk = if p.is_markdown() { e.read_note(p).ok() } else { None };
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
}

/// Every locked note and folder, for lock icons.
pub fn locked_paths(e: &mut Engine) -> LockResult<LockedPaths> {
    let folders = load(e.vault().root())?.folders.into_iter().map(|f| f.path).collect();
    let notes = e.locked_notes().into_iter().map(|p| p.to_string()).collect();
    Ok(LockedPaths { notes, folders })
}

/// Tries a password on a note or a locked folder; keeps the key if it fits.
pub fn unlock(locks: &Locks, e: &Engine, p: &VaultPath, password: &str) -> LockResult<()> {
    let sealed = 'found: {
        if p.is_markdown() {
            if let Ok(t) = e.read_note(p) {
                if lock::is_locked(&t) {
                    break 'found t;
                }
            }
        }
        folder_for(&load(e.vault().root())?, p.as_str()).map(|f| f.check).ok_or_else(|| "This isn't locked.".to_string())?
    };
    let (_, key) = lock::open_with_password(&sealed, password).map_err(err)?;
    locks.keep(key);
    Ok(())
}

/// A password is long enough to lock with.
pub fn check_password(pw: &str) -> LockResult<()> {
    if pw.chars().count() < 6 {
        return Err("Use a password of at least 6 characters.".into());
    }
    Ok(())
}

/// What locking changed: the vault events, how many notes, and readable
/// copies that couldn't be removed.
pub struct Locked {
    pub events: Vec<EngineEvent>,
    pub count: usize,
    pub problems: Vec<String>,
}

/// Locks one note with a new password.
pub fn lock_note(locks: &Locks, e: &mut Engine, p: &VaultPath, password: &str, hint: Option<&str>, backups: Option<&BackupStore>) -> LockResult<Locked> {
    check_password(password)?;
    let key = lock::derive(password, lock::new_salt(), lock::Cost::default()).map_err(err)?;
    let text = e.read_note(p).map_err(err)?;
    if lock::is_locked(&text) {
        return Err("This note is already locked.".into());
    }
    let events = e.write_note(p, &lock::seal(&text, &key, hint)).map_err(err)?;
    // Readable copies made before the lock must go too.
    let problems = wipe_plain_copies(e, std::slice::from_ref(p), backups);
    locks.keep(key);
    Ok(Locked { events, count: 1, problems })
}

/// Locks every note in a folder (and its subfolders) with one password;
/// notes added later are locked too.
pub fn lock_folder(locks: &Locks, e: &mut Engine, folder: &VaultPath, password: &str, hint: Option<&str>, backups: Option<&BackupStore>) -> LockResult<Locked> {
    check_password(password)?;
    let key = lock::derive(password, lock::new_salt(), lock::Cost::default()).map_err(err)?;
    let root = e.vault().root().to_path_buf();
    let mut reg = load(&root)?;
    if let Some(f) = folder_for(&reg, folder.as_str()) {
        return Err(format!("“{}” is already locked.", if f.path.is_empty() { "The vault" } else { &f.path }));
    }
    let notes: Vec<VaultPath> = e.vault().markdown_files().filter(|n| n.starts_with(folder)).cloned().collect();
    let mut events = Vec::new();
    let mut sealed = Vec::new();
    for n in &notes {
        let Ok(text) = e.read_note(n) else { continue };
        if lock::is_locked(&text) {
            continue;
        }
        events.extend(e.write_note(n, &lock::seal(&text, &key, hint)).map_err(err)?);
        sealed.push(n.clone());
    }
    let problems = wipe_plain_copies(e, &sealed, backups);
    reg.folders.push(LockedFolder { path: folder.to_string(), check: lock::seal(CHECK, &key, hint) });
    store(&root, &reg)?;
    locks.keep(key);
    Ok(Locked { events, count: sealed.len(), problems })
}

/// Removes the lock from a note, or from a folder and every note in it
/// locked with the folder's password. The password must have been entered.
pub fn remove_lock(locks: &Locks, e: &mut Engine, p: &VaultPath) -> LockResult<(Vec<EngineEvent>, usize)> {
    let root = e.vault().root().to_path_buf();
    let mut reg = load(&root)?;
    let mut events = Vec::new();
    let mut count = 0;
    let mut open_note = |e: &mut Engine, n: &VaultPath, only_salt: Option<&str>| -> LockResult<()> {
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
        let notes: Vec<VaultPath> = e.vault().markdown_files().filter(|n| n.starts_with(p)).cloned().collect();
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
        open_note(e, p, None)?;
    }
    Ok((events, count))
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
        moved(dir.path(), "F", "G").unwrap();
        assert_eq!(load(dir.path()).unwrap().folders[0].path, "G");
    }

    #[test]
    fn a_damaged_lock_list_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let locks = Locks::default();
        std::fs::create_dir_all(dir.path().join(".marrow")).unwrap();
        std::fs::write(registry_file(dir.path()), "{ this is not json").unwrap();
        // Saving anything is refused rather than guessed as "not locked".
        assert!(conceal(&locks, dir.path(), "F/n.md", None, "secret").unwrap_err().contains("locked.json"));
        assert!(needs_lock(dir.path(), "n.md", None).is_err());
        assert!(check_move(&locks, dir.path(), "a.md", "F/a.md").is_err());
        // No file at all simply means nothing is locked.
        std::fs::remove_file(registry_file(dir.path())).unwrap();
        assert_eq!(conceal(&locks, dir.path(), "n.md", None, "plain").unwrap(), "plain");
    }

    #[test]
    fn moving_into_a_locked_folder_needs_its_password() {
        let dir = tempfile::tempdir().unwrap();
        let locks = Locks::default();
        let key = lock::derive("secret", [5u8; 16], lock::Cost { m: 64, t: 1, p: 1 }).unwrap();
        store(dir.path(), &Registry { folders: vec![LockedFolder { path: "F".into(), check: lock::seal(CHECK, &key, None) }] }).unwrap();
        assert_eq!(check_move(&locks, dir.path(), "a.md", "F/a.md"), Err(format!("{LOCKED_FOLDER}F")));
        // Within the same locked folder, or out of it, is fine.
        assert!(check_move(&locks, dir.path(), "F/a.md", "F/b.md").is_ok());
        assert!(check_move(&locks, dir.path(), "a.md", "b.md").is_ok());
        locks.keep(key);
        assert!(check_move(&locks, dir.path(), "a.md", "F/a.md").is_ok());
    }

    #[test]
    fn notes_moved_into_a_locked_folder_are_sealed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("F")).unwrap();
        std::fs::write(dir.path().join("a.md"), "my secret plan").unwrap();
        let locks = Locks::default();
        let key = lock::derive("secret", [6u8; 16], lock::Cost { m: 64, t: 1, p: 1 }).unwrap();
        store(dir.path(), &Registry { folders: vec![LockedFolder { path: "F".into(), check: lock::seal(CHECK, &key, None) }] }).unwrap();
        locks.keep(key);
        let mut e = crate::Engine::open(dir.path(), crate::cache::Cache::in_memory().unwrap()).unwrap();
        let from = VaultPath::new("a.md").unwrap();
        let to = VaultPath::new("F/a.md").unwrap();
        let store = crate::BackupStore::new(dir.path().join("backups"));
        // Kept text is filed under the vault's own (resolved) path, as the app does.
        let root = e.vault().root().to_path_buf();
        store.put(&root, "a.md", "unsaved secret words").unwrap();
        // Sealed before it moves: even a crash right now leaves no readable copy.
        let (_, sealed) = seal_for_move(&locks, &mut e, &from, &to, Some(&store)).unwrap();
        assert_eq!(sealed, vec![from.clone()]);
        let before_move = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
        assert!(lock::is_locked(&before_move) && !before_move.contains("secret plan"));
        let kept = store.get(&root, "a.md").unwrap().text;
        assert!(lock::is_locked(&kept) && !kept.contains("secret words"));
        e.rename_with_links(&from, &to).unwrap();
        let on_disk = std::fs::read_to_string(dir.path().join("F/a.md")).unwrap();
        assert_eq!(reveal(&locks, on_disk).unwrap(), "my secret plan");
        // A note moving somewhere unlocked is left as it is.
        std::fs::write(dir.path().join("b.md"), "plain").unwrap();
        e.on_paths_changed(&[VaultPath::new("b.md").unwrap()]).unwrap();
        let (_, none) = seal_for_move(&locks, &mut e, &VaultPath::new("b.md").unwrap(), &VaultPath::new("c.md").unwrap(), Some(&store)).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn a_retried_move_still_seals_what_an_earlier_try_missed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("F")).unwrap();
        let locks = Locks::default();
        let key = lock::derive("secret", [7u8; 16], lock::Cost { m: 64, t: 1, p: 1 }).unwrap();
        store(dir.path(), &Registry { folders: vec![LockedFolder { path: "F".into(), check: lock::seal(CHECK, &key, None) }] }).unwrap();
        // An earlier try sealed the note but stopped before its kept text.
        std::fs::write(dir.path().join("a.md"), lock::seal("my secret plan", &key, None)).unwrap();
        locks.keep(key);
        let mut e = crate::Engine::open(dir.path(), crate::cache::Cache::in_memory().unwrap()).unwrap();
        let root = e.vault().root().to_path_buf();
        let store = crate::BackupStore::new(dir.path().join("backups"));
        store.put(&root, "a.md", "unsaved secret words").unwrap();
        let from = VaultPath::new("a.md").unwrap();
        let (_, sealed) = seal_for_move(&locks, &mut e, &from, &VaultPath::new("F/a.md").unwrap(), Some(&store)).unwrap();
        // The kept text is sealed now, and the note's readable copies get wiped.
        assert_eq!(sealed, vec![from]);
        let kept = store.get(&root, "a.md").unwrap().text;
        assert!(lock::is_locked(&kept) && !kept.contains("secret words"));
        assert_eq!(reveal(&locks, std::fs::read_to_string(dir.path().join("a.md")).unwrap()).unwrap(), "my secret plan");
    }

    #[test]
    fn renaming_a_locked_folder_survives_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let key = lock::derive("secret", [8u8; 16], lock::Cost { m: 64, t: 1, p: 1 }).unwrap();
        let folder = |p: &str| LockedFolder { path: p.into(), check: lock::seal(CHECK, &key, None) };
        store(root, &Registry { folders: vec![folder("F"), folder("F/Inner"), folder("Other")] }).unwrap();
        std::fs::create_dir_all(root.join("F/Inner")).unwrap();
        std::fs::create_dir_all(root.join("Other")).unwrap();
        let paths = || {
            let mut v: Vec<String> = load(root).unwrap().folders.into_iter().map(|f| f.path).collect();
            v.sort();
            v
        };
        // Listed under both names before the folder moves.
        moving(root, "F", "G").unwrap();
        assert_eq!(paths(), ["F", "F/Inner", "G", "G/Inner", "Other"]);
        assert!(folder_for(&load(root).unwrap(), "F/new.md").is_some());
        assert!(folder_for(&load(root).unwrap(), "G/new.md").is_some());
        // Crash after the folder moved: the next open keeps the new place.
        std::fs::rename(root.join("F"), root.join("G")).unwrap();
        tidy(root).unwrap();
        assert_eq!(paths(), ["G", "G/Inner", "Other"]);
        // Crash before it moved: the old place is kept.
        moving(root, "G", "H").unwrap();
        tidy(root).unwrap();
        assert_eq!(paths(), ["G", "G/Inner", "Other"]);
        // A normal rename ends with one entry per folder.
        moving(root, "G", "H").unwrap();
        std::fs::rename(root.join("G"), root.join("H")).unwrap();
        moved(root, "G", "H").unwrap();
        assert_eq!(paths(), ["H", "H/Inner", "Other"]);
    }
}
