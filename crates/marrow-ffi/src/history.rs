//! Earlier versions of each note, and the time machine: the whole vault as
//! it was at some moment.

use marrow_core::history::{History, NoteNow, ThenState};
use marrow_core::vault_locks as vl;

use crate::files::display_name;
use crate::{fail, vp, Marrow, Result};

#[derive(Debug, Clone, uniffi::Record)]
pub struct Version {
    /// When it was kept (ms since 1970), also its id.
    pub time_ms: i64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NoteThen {
    /// Unchanged since.
    Same,
    /// Changed since.
    Changed,
    /// Deleted (or renamed outside Marrow) since.
    Gone,
}

/// A note as it was at some moment.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ThenNote {
    pub path: String,
    pub title: String,
    pub state: NoteThen,
    /// The kept version holding it as it was; `None` means the note as it is
    /// now (or, for a changed note, that no copy from then was kept).
    pub version: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DayActivity {
    /// Days since 1970 (UTC).
    pub day: i64,
    pub versions: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Timeline {
    pub days: Vec<DayActivity>,
    /// When the oldest note was made.
    pub first_ms: Option<i64>,
}

#[uniffi::export]
impl Marrow {
    /// Kept versions of a note, newest first.
    pub fn versions(&self, path: String) -> Result<Vec<Version>> {
        let p = vp(&path)?;
        self.with(|e| Ok(History::for_vault(e.vault().root()).list(p.as_str()).into_iter().map(|v| Version { time_ms: v.time_ms, size: v.size }).collect()))
    }

    /// The text of a kept version.
    pub fn read_version(&self, path: String, time_ms: i64) -> Result<String> {
        let p = vp(&path)?;
        let text = self.with(|e| Ok(History::for_vault(e.vault().root()).read(p.as_str(), time_ms)?))?;
        Ok(vl::reveal(&self.locks, text)?)
    }

    /// Puts a kept version back (keeping the current text as a version
    /// first, so the restore can be undone). Brings back a deleted note too.
    pub fn restore_version(&self, path: String, time_ms: i64) -> Result<()> {
        let old = self.read_version(path.clone(), time_ms)?;
        let p = vp(&path)?;
        let exists = self.with(|e| Ok(e.vault().get(&p).is_some()))?;
        if !exists {
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            let made = self.create_note(folder.to_string(), Some(display_name(name)), old)?;
            if made != path {
                return Err(fail(format!("Restored as “{}”.", display_name(&made))));
            }
            return Ok(());
        }
        let now = self.read_note(path.clone())?;
        self.with(|e| {
            let disk = e.read_note(&p).ok();
            let sealed = vl::conceal(&self.locks, e.vault().root(), p.as_str(), disk.as_deref(), &now)?;
            History::for_vault(e.vault().root()).snapshot(p.as_str(), &sealed, marrow_core::history::now_ms())?;
            Ok(())
        })?;
        self.write_note(path, old, Some(now))
    }

    /// The vault as it was at `time_ms`.
    pub fn vault_at(&self, time_ms: i64) -> Result<Vec<ThenNote>> {
        self.with(|e| {
            let v = e.vault();
            let stats: Vec<(String, marrow_core::vault::FileStat)> = v.markdown_files().filter(|p| !p.is_hidden()).filter_map(|p| Some((p.as_str().to_string(), v.stat(p)?))).collect();
            let now: Vec<NoteNow> = stats.iter().map(|(p, s)| NoteNow { path: p, created_ms: s.ctime_ms, modified_ms: s.mtime_ms }).collect();
            Ok(History::for_vault(v.root())
                .vault_at(&now, time_ms)
                .into_iter()
                .map(|t| ThenNote {
                    title: display_name(&t.path),
                    path: t.path,
                    state: match t.state {
                        ThenState::Same => NoteThen::Same,
                        ThenState::Changed => NoteThen::Changed,
                        ThenState::Gone => NoteThen::Gone,
                    },
                    version: t.version,
                })
                .collect())
        })
    }

    /// How much was kept each day, for the time machine's timeline.
    pub fn timeline(&self) -> Result<Timeline> {
        self.with(|e| {
            let v = e.vault();
            let first_ms = v.markdown_files().filter_map(|p| v.stat(p)).map(|s| s.ctime_ms.min(s.mtime_ms)).min();
            Ok(Timeline {
                days: History::for_vault(v.root()).activity().into_iter().map(|(day, versions)| DayActivity { day, versions }).collect(),
                first_ms,
            })
        })
    }
}
