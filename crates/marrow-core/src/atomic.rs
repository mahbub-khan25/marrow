//! Crash-safe file writes.
//!
//! A plain `fs::write` empties the file first and then fills it, so a crash,
//! power cut or full disk in between leaves a truncated note. Here the new
//! contents go to a hidden temporary file beside the target, are flushed to
//! the disk, and then replace the target in one step. Whatever happens, the
//! file holds either the old contents or the new ones, never a mix.
//!
//! The temporary file is named `.<name>.marrow-<pid>-<n>.tmp`. It starts with
//! a dot, so the vault, the watcher and folder sync all skip it, and
//! [`clean_up`] removes any a crash left behind.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Marks the temporary files this module makes.
const TAG: &str = ".marrow-";

/// Replaces `path` with `bytes`, crash-safely. Creates the file if needed
/// (but not its folder). Follows a symlink and replaces the file it points to.
/// Takes the same arguments as `std::fs::write`.
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let (path, bytes) = (path.as_ref(), bytes.as_ref());
    let target = real_target(path);
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_string_lossy()
        .into_owned();

    let (tmp, mut file) = create_temp(&dir, &name)?;
    let result = (|| {
        file.write_all(bytes)?;
        // Data on the disk before the rename makes it visible.
        file.sync_all()?;
        keep_permissions(&target, &file);
        drop(file);
        replace(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    sync_dir(&dir);
    Ok(())
}

/// Like [`write`], for text.
pub fn write_str(path: &Path, text: &str) -> io::Result<()> {
    write(path, text.as_bytes())
}

/// Writes a file that must not exist yet. Fails with `AlreadyExists` if it
/// does. The file appears complete or not at all (on drives without hard
/// links, such as FAT USB sticks, an interrupted create can leave it empty).
pub fn create(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let (target, bytes) = (path.as_ref(), bytes.as_ref());
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let (tmp, mut file) = create_temp(&dir, &name)?;
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // A hard link appears in one step and refuses to replace anything.
    let linked = fs::hard_link(&tmp, target);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => {
            sync_dir(&dir);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => {
            // No hard links here: reserve the name, then fill it.
            OpenOptions::new().write(true).create_new(true).open(target)?;
            write(target, bytes).inspect_err(|_| {
                if fs::metadata(target).map(|m| m.len() == 0).unwrap_or(false) {
                    let _ = fs::remove_file(target);
                }
            })
        }
    }
}

/// Removes temporary files that a crash left in `root` and its folders.
/// Only files older than a few minutes are touched, in case another copy of
/// Marrow is saving right now. Returns how many were removed.
pub fn clean_up(root: &Path) -> usize {
    let cutoff = SystemTime::now() - Duration::from_secs(10 * 60);
    let mut removed = 0;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_skipped_dir(e))
        .flatten()
    {
        if !entry.file_type().is_file() || !is_temp_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t < cutoff)
            .unwrap_or(false);
        if old && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn is_skipped_dir(e: &walkdir::DirEntry) -> bool {
    e.file_type().is_dir() && matches!(e.file_name().to_str(), Some(".git" | "node_modules" | ".trash"))
}

/// True for names this module gives its temporary files.
pub fn is_temp_name(name: &str) -> bool {
    // `.~marrow-…` is what earlier versions of folder sync used.
    name.starts_with('.') && name.ends_with(".tmp") && (name.contains(TAG) || name.starts_with(".~marrow-"))
}

fn real_target(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

fn create_temp(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    // Keep the temporary name short enough for 255-character file-name limits.
    let short: String = name.chars().take(120).collect();
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{short}{TAG}{}-{n}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => return Ok((tmp, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Moves `tmp` over `target`. On Windows another program (antivirus, search
/// indexer, a sync app) may hold the target open for a moment, so this
/// retries for about a second before giving up.
fn replace(tmp: &Path, target: &Path) -> io::Result<()> {
    let mut wait = Duration::from_millis(5);
    let mut tries = 0;
    loop {
        match fs::rename(tmp, target) {
            Ok(()) => return Ok(()),
            Err(e) if tries < 9 && is_transient(&e) => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(250));
                tries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn is_transient(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    // ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
    cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33))
}

#[cfg(unix)]
fn keep_permissions(target: &Path, file: &File) {
    if let Ok(m) = fs::metadata(target) {
        let _ = file.set_permissions(m.permissions());
    }
}

#[cfg(not(unix))]
fn keep_permissions(_target: &Path, _file: &File) {}

/// Makes the rename itself survive a power cut (Unix needs the folder
/// flushed; Windows has no equivalent and doesn't need one).
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temps(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_temp_name(n))
            .collect()
    }

    #[test]
    fn replaces_and_creates() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Note.md");
        write(&p, b"first").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "first");
        write(&p, b"second, longer").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "second, longer");
        write(&p, b"3").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "3");
        assert!(temps(d.path()).is_empty());
    }

    #[test]
    fn failure_leaves_the_old_file_and_no_temp() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Note.md");
        fs::write(&p, "original").unwrap();
        // A folder where the file should be makes the final step fail.
        let blocked = d.path().join("Blocked.md");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("inside"), "x").unwrap();
        assert!(write(&blocked, b"new").is_err());
        assert!(temps(d.path()).is_empty());
        assert_eq!(fs::read_to_string(&p).unwrap(), "original");
    }

    #[test]
    fn missing_folder_is_an_error_not_a_panic() {
        let d = tempfile::tempdir().unwrap();
        assert!(write(d.path().join("nope/Note.md"), b"x").is_err());
    }

    #[test]
    fn create_refuses_existing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("New.md");
        create(&p, b"one").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "one");
        let e = create(&p, b"two").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&p).unwrap(), "one");
    }

    #[test]
    fn long_names_still_work() {
        let d = tempfile::tempdir().unwrap();
        let name = format!("{}.md", "a".repeat(200));
        let p = d.path().join(&name);
        write(&p, b"x").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "x");
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_and_keeps_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real.md");
        fs::write(&real, "old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let link = d.path().join("link.md");
        symlink(&real, &link).unwrap();
        write(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[test]
    fn clean_up_removes_only_old_temps() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("sub")).unwrap();
        let old = d.path().join("sub/.Note.md.marrow-1-1.tmp");
        let fresh = d.path().join(".Other.md.marrow-1-2.tmp");
        let mine = d.path().join(".hidden-note.tmp");
        for p in [&old, &fresh, &mine] {
            fs::write(p, "x").unwrap();
        }
        let long_ago = SystemTime::now() - Duration::from_secs(3600);
        File::options().write(true).open(&old).unwrap().set_modified(long_ago).unwrap();
        File::options().write(true).open(&mine).unwrap().set_modified(long_ago).unwrap();
        assert_eq!(clean_up(d.path()), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(mine.exists());
    }

    /// A save that dies halfway (here: the process "crashes" after writing
    /// the temporary file) leaves the note exactly as it was.
    #[test]
    fn interrupted_save_keeps_the_original() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Note.md");
        fs::write(&p, "everything I wrote yesterday").unwrap();
        let (tmp, mut f) = create_temp(d.path(), "Note.md").unwrap();
        f.write_all(b"half of the new te").unwrap();
        drop(f); // crash: the rename never happens
        assert_eq!(fs::read_to_string(&p).unwrap(), "everything I wrote yesterday");
        assert!(tmp.exists());
    }
}
