//! Backups of note text that couldn't be saved.
//!
//! When a save fails (disk full, file locked by another program, permission
//! lost on a network drive), the text is written here instead, outside the
//! vault, in the app's data folder. The next time the note is opened the
//! app offers it back.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cache::hash_bytes;
use crate::error::{IoContext, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub vault: String,
    pub path: String,
    pub text: String,
    /// Milliseconds since the Unix epoch.
    pub saved_at: i64,
}

pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        BackupStore { dir: dir.into() }
    }

    fn file_for(&self, vault: &Path, path: &str) -> PathBuf {
        let key = hash_bytes(format!("{}\u{0}{}", vault.display(), path).as_bytes());
        self.dir.join(format!("{}.json", &key[..24]))
    }

    pub fn put(&self, vault: &Path, path: &str, text: &str) -> Result<()> {
        fs::create_dir_all(&self.dir).at(&self.dir)?;
        let b = Backup {
            vault: vault.display().to_string(),
            path: path.to_string(),
            text: text.to_string(),
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        };
        let file = self.file_for(vault, path);
        let json = serde_json::to_vec_pretty(&b).expect("backup serialises");
        // Crash-safe, so a crash mid-write can't destroy an older backup.
        crate::atomic::write(&file, &json).at(&file)
    }

    pub fn get(&self, vault: &Path, path: &str) -> Option<Backup> {
        let bytes = fs::read(self.file_for(vault, path)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Deletes the kept text. Having nothing to delete is fine; failing to
    /// delete is an error, since the copy may be readable text of a note
    /// that has since been locked.
    pub fn remove(&self, vault: &Path, path: &str) -> std::io::Result<()> {
        match fs::remove_file(self.file_for(vault, path)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Moves a backup along when its note is renamed.
    pub fn rename(&self, vault: &Path, from: &str, to: &str) {
        if let Some(b) = self.get(vault, from) {
            if self.put(vault, to, &b.text).is_ok() {
                let _ = self.remove(vault, from);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_rename_remove() {
        let d = tempfile::tempdir().unwrap();
        let store = BackupStore::new(d.path().join("backups"));
        let vault = Path::new("/notes");
        assert!(store.get(vault, "a.md").is_none());
        store.put(vault, "a.md", "unsaved words").unwrap();
        let b = store.get(vault, "a.md").unwrap();
        assert_eq!(b.text, "unsaved words");
        assert!(b.saved_at > 0);
        // Same path in another vault is a different backup.
        assert!(store.get(Path::new("/other"), "a.md").is_none());

        store.rename(vault, "a.md", "b.md");
        assert!(store.get(vault, "a.md").is_none());
        assert_eq!(store.get(vault, "b.md").unwrap().text, "unsaved words");

        store.remove(vault, "b.md").unwrap();
        // Nothing left to remove is not an error.
        store.remove(vault, "b.md").unwrap();
        assert!(store.get(vault, "b.md").is_none());
    }

    #[test]
    fn a_copy_that_cannot_be_removed_is_reported() {
        let d = tempfile::tempdir().unwrap();
        let store = BackupStore::new(d.path().join("backups"));
        let vault = Path::new("/notes");
        // Something undeletable where the copy lives (a folder with a file in it).
        let f = store.file_for(vault, "c.md");
        fs::create_dir_all(f.join("stuck")).unwrap();
        assert!(store.remove(vault, "c.md").is_err());
    }
}
