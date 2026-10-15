//! Version history: earlier copies of each note, kept in
//! `<vault>/.marrow/history/<note path>/<unix ms>.md`.
//!
//! A copy of the text on disk is taken just before a save replaces it, at
//! most every few minutes per note, so a long writing session leaves a
//! handful of versions rather than hundreds. Older versions thin out:
//! everything from the last day, then one per hour for a month, then one per
//! day for a year.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::vault::CONFIG_DIR;

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;
/// Never more than this many versions of one note.
const MAX_VERSIONS: usize = 500;

/// One saved version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    /// Milliseconds since 1970, also the version's id.
    pub time_ms: i64,
    /// Size in bytes.
    pub size: u64,
}

pub struct History {
    dir: PathBuf,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl History {
    pub fn for_vault(root: &Path) -> Self {
        History { dir: root.join(CONFIG_DIR).join("history") }
    }

    fn note_dir(&self, path: &str) -> PathBuf {
        let mut d = self.dir.clone();
        for part in path.split('/').filter(|p| !p.is_empty() && *p != "." && *p != "..") {
            d.push(part);
        }
        d
    }

    /// Versions of `path`, newest first.
    pub fn list(&self, path: &str) -> Vec<Version> {
        let Ok(rd) = fs::read_dir(self.note_dir(path)) else { return Vec::new() };
        let mut out: Vec<Version> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let t: i64 = name.strip_suffix(".md")?.parse().ok()?;
                let size = e.metadata().ok()?.len();
                Some(Version { time_ms: t, size })
            })
            .collect();
        out.sort_by_key(|v| std::cmp::Reverse(v.time_ms));
        out
    }

    pub fn read(&self, path: &str, time_ms: i64) -> io::Result<String> {
        fs::read_to_string(self.note_dir(path).join(format!("{time_ms}.md")))
    }

    /// Keeps `text` as a version of `path` (unless it's the same as the
    /// newest one). Returns the new version's time.
    pub fn snapshot(&self, path: &str, text: &str, now: i64) -> io::Result<Option<i64>> {
        let newest = self.list(path).into_iter().next();
        if let Some(v) = &newest {
            if self.read(path, v.time_ms).is_ok_and(|t| t == text) {
                return Ok(None);
            }
        }
        let dir = self.note_dir(path);
        fs::create_dir_all(&dir)?;
        let mut t = now;
        while dir.join(format!("{t}.md")).exists() {
            t += 1;
        }
        fs::write(dir.join(format!("{t}.md")), text)?;
        self.prune(path, now);
        Ok(Some(t))
    }

    /// Before a save: keeps the text being replaced, if the newest version
    /// is older than `min_gap_ms`.
    pub fn before_save(&self, path: &str, old: &str, new: &str, now: i64, min_gap_ms: i64) -> io::Result<()> {
        if old == new || old.trim().is_empty() {
            return Ok(());
        }
        if let Some(v) = self.list(path).first() {
            if now - v.time_ms < min_gap_ms {
                return Ok(());
            }
        }
        self.snapshot(path, old, now).map(|_| ())
    }

    /// Deletes every version of `path` (a note just locked mustn't keep
    /// readable copies of itself here).
    pub fn forget(&self, path: &str) -> io::Result<()> {
        let d = self.note_dir(path);
        if d.exists() && d != self.dir {
            fs::remove_dir_all(d)?;
        }
        Ok(())
    }

    /// Follows a rename or move (of a note or a whole folder).
    pub fn moved(&self, from: &str, to: &str) -> io::Result<()> {
        let a = self.note_dir(from);
        if !a.exists() {
            return Ok(());
        }
        let b = self.note_dir(to);
        if let Some(parent) = b.parent() {
            fs::create_dir_all(parent)?;
        }
        if from.to_lowercase() == to.to_lowercase() {
            // Only the case changed: on Windows and macOS `b` "exists" already.
            let tmp = a.with_extension(format!("renaming-{}", now_ms()));
            fs::rename(&a, &tmp)?;
            return fs::rename(&tmp, &b);
        }
        if !b.exists() {
            return fs::rename(&a, &b);
        }
        // Both exist (e.g. a name reused): move the files across.
        merge_dirs(&a, &b)?;
        fs::remove_dir_all(&a)
    }

    /// Thins out old versions.
    fn prune(&self, path: &str, now: i64) {
        let versions = self.list(path);
        let mut keep_buckets = std::collections::HashSet::new();
        let dir = self.note_dir(path);
        for (i, v) in versions.iter().enumerate() {
            let age = now - v.time_ms;
            let keep = if i >= MAX_VERSIONS || age > 365 * DAY {
                false
            } else if age <= DAY {
                true
            } else {
                // Newest first, so the first one seen in a bucket is kept.
                let bucket = if age <= 30 * DAY { ("h", v.time_ms / HOUR) } else { ("d", v.time_ms / DAY) };
                keep_buckets.insert(bucket)
            };
            if !keep {
                let _ = fs::remove_file(dir.join(format!("{}.md", v.time_ms)));
            }
        }
    }
}

fn merge_dirs(a: &Path, b: &Path) -> io::Result<()> {
    for e in fs::read_dir(a)? {
        let e = e?;
        let to = b.join(e.file_name());
        if e.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            merge_dirs(&e.path(), &to)?;
        } else if !to.exists() {
            fs::rename(e.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_versions_with_a_gap() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::for_vault(dir.path());
        let t0 = 1_000_000_000_000;
        h.before_save("A/n.md", "one", "two", t0, 300_000).unwrap();
        h.before_save("A/n.md", "two", "three", t0 + 60_000, 300_000).unwrap(); // too soon
        h.before_save("A/n.md", "three", "four", t0 + 400_000, 300_000).unwrap();
        h.before_save("A/n.md", "four", "four", t0 + 900_000, 300_000).unwrap(); // unchanged
        let v = h.list("A/n.md");
        assert_eq!(v.len(), 2);
        assert_eq!(h.read("A/n.md", v[0].time_ms).unwrap(), "three");
        assert_eq!(h.read("A/n.md", v[1].time_ms).unwrap(), "one");
        // Same text as the newest isn't stored again.
        assert_eq!(h.snapshot("A/n.md", "three", t0 + 2_000_000).unwrap(), None);
    }

    #[test]
    fn follows_renames_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::for_vault(dir.path());
        h.snapshot("A/n.md", "x", 1).unwrap();
        h.moved("A/n.md", "B/m.md").unwrap();
        assert!(h.list("A/n.md").is_empty());
        assert_eq!(h.list("B/m.md").len(), 1);
        h.moved("B", "C/B").unwrap();
        assert_eq!(h.read("C/B/m.md", 1).unwrap(), "x");
    }

    #[test]
    fn thins_out_old_versions() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::for_vault(dir.path());
        let now = 400 * DAY;
        // Every 10 minutes for 3 days back, plus some very old ones.
        for k in 0..(3 * 24 * 6) {
            let t = now - DAY / 2 - k * 600_000;
            fs::create_dir_all(h.note_dir("n.md")).unwrap();
            fs::write(h.note_dir("n.md").join(format!("{t}.md")), format!("v{k}")).unwrap();
        }
        fs::write(h.note_dir("n.md").join(format!("{}.md", now - 380 * DAY)), "ancient").unwrap();
        h.prune("n.md", now);
        let v = h.list("n.md");
        let recent = v.iter().filter(|x| now - x.time_ms <= DAY).count();
        assert_eq!(recent, 73); // 12 hours × 6 (+ the one at exactly a day), all kept
        let older = v.iter().filter(|x| now - x.time_ms > DAY).count();
        // Two and a half days at one per hour (instead of six).
        assert!((59..=61).contains(&older), "one per hour after a day, got {older}");
        assert!(!v.iter().any(|x| now - x.time_ms > 365 * DAY));
    }
}
