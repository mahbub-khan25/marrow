//! Password-locked notes and folders (the logic is shared with the desktop
//! app in `marrow_core::vault_locks`). Passwords are remembered only while
//! the app runs, and forgotten by [`Marrow::lock_all`].

use marrow_core::vault_locks as vl;

use crate::{vp, Marrow, Result};

#[derive(Debug, Clone, uniffi::Record)]
pub struct LockState {
    /// The note is stored sealed.
    pub locked: bool,
    /// Its password (or its folder's) was entered this session.
    pub unlocked: bool,
    pub hint: Option<String>,
    /// The locked folder it's in (or is), if any.
    pub folder: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct LockedList {
    pub notes: Vec<String>,
    pub folders: Vec<String>,
}

#[uniffi::export]
impl Marrow {
    pub fn lock_state(&self, path: String) -> Result<LockState> {
        let p = vp(&path)?;
        let i = self.with(|e| Ok(vl::lock_info(&self.locks, e, &p)?))?;
        Ok(LockState { locked: i.locked, unlocked: i.unlocked, hint: i.hint, folder: i.folder })
    }

    /// Every locked note and folder, for lock icons.
    pub fn locked(&self) -> Result<LockedList> {
        let l = self.with(|e| Ok(vl::locked_paths(e)?))?;
        Ok(LockedList { notes: l.notes, folders: l.folders })
    }

    /// Tries a password on a locked note or folder.
    pub fn unlock(&self, path: String, password: String) -> Result<()> {
        let p = vp(&path)?;
        self.with(|e| Ok(vl::unlock(&self.locks, e, &p, &password)?))?;
        self.tell_locks();
        Ok(())
    }

    /// Locks one note with a new password. Returns a warning if some
    /// readable copies couldn't be removed.
    pub fn lock_note(&self, path: String, password: String, hint: Option<String>) -> Result<Option<String>> {
        let p = vp(&path)?;
        let backups = self.backups();
        let done = self.with(|e| Ok(vl::lock_note(&self.locks, e, &p, &password, hint.as_deref().filter(|h| !h.trim().is_empty()), Some(&backups))?))?;
        self.tell(done.events);
        self.tell_locks();
        self.poke_sync();
        Ok(vl::problems_notice(&done.problems))
    }

    /// Locks every note in a folder with one password; notes added later
    /// are locked too. Returns how many notes were locked.
    pub fn lock_folder(&self, path: String, password: String, hint: Option<String>) -> Result<u32> {
        let p = vp(&path)?;
        let backups = self.backups();
        let done = self.with(|e| Ok(vl::lock_folder(&self.locks, e, &p, &password, hint.as_deref().filter(|h| !h.trim().is_empty()), Some(&backups))?))?;
        let count = done.count as u32;
        self.tell(done.events);
        self.tell_locks();
        self.poke_sync();
        Ok(count)
    }

    /// Removes the lock from a note or folder (its password must have been entered).
    pub fn remove_lock(&self, path: String) -> Result<u32> {
        let p = vp(&path)?;
        let (events, count) = self.with(|e| Ok(vl::remove_lock(&self.locks, e, &p)?))?;
        self.tell(events);
        self.tell_locks();
        self.poke_sync();
        Ok(count as u32)
    }

    /// Forgets every password entered (when the app goes to the background
    /// for a while, or the person asks).
    pub fn lock_all(&self) {
        self.locks.forget_all();
        self.tell_locks();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lock_unlock_and_forget() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("Secret.md", "my diary")]);
        m.lock_note("Secret.md".into(), "hunter22".into(), Some("cat".into())).unwrap();
        let raw = std::fs::read_to_string(dir.path().join("vault/Secret.md")).unwrap();
        assert!(!raw.contains("diary"));
        assert_eq!(m.read_note("Secret.md".into()).unwrap(), "my diary");
        m.lock_all();
        assert!(matches!(m.read_note("Secret.md".into()), Err(crate::MarrowError::Locked)));
        let s = m.lock_state("Secret.md".into()).unwrap();
        assert!(s.locked && !s.unlocked);
        assert_eq!(s.hint.as_deref(), Some("cat"));
        assert!(m.unlock("Secret.md".into(), "wrong!!".into()).is_err());
        m.unlock("Secret.md".into(), "hunter22".into()).unwrap();
        m.write_note("Secret.md".into(), "dear diary".into(), None).unwrap();
        assert!(!std::fs::read_to_string(dir.path().join("vault/Secret.md")).unwrap().contains("dear"));
        assert_eq!(m.remove_lock("Secret.md".into()).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(dir.path().join("vault/Secret.md")).unwrap(), "dear diary");
    }
}
