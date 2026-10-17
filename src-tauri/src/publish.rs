//! Writing a published website to a folder.
//!
//! The window renders the pages; these commands only put files in the
//! output folder. A `.marrow-site.json` list of what was written lets a later
//! publish to the same folder remove pages that are gone, without ever
//! touching files Marrow didn't write. A folder that isn't empty and wasn't
//! published by Marrow is refused, and so is a folder inside the vault.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use tauri::State;

use crate::commands::{err, vp, with_engine, AppState, CmdResult};

const MANIFEST: &str = ".marrow-site.json";

/// `rel` under `out`, refusing anything that would step outside it.
fn inside(out: &Path, rel: &str) -> CmdResult<PathBuf> {
    let p = Path::new(rel);
    if rel.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Not a file inside the site: {rel}"));
    }
    Ok(out.join(p))
}

fn previous(out: &Path) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(out.join(MANIFEST)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Checks the output folder before publishing (creating it if needed).
#[tauri::command]
pub fn site_begin(state: State<AppState>, out: String) -> CmdResult<()> {
    let out = PathBuf::from(&out);
    let vault = with_engine(&state, |e| Ok(e.vault().root().to_path_buf()))?;
    let canon = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    std::fs::create_dir_all(&out).map_err(err)?;
    let (o, v) = (canon(&out), canon(&vault));
    if o.starts_with(&v) {
        return Err("Pick a folder outside the vault, so the website's pages don't show up as notes.".into());
    }
    if v.starts_with(&o) {
        return Err("That folder holds the vault. Pick an empty folder for the website.".into());
    }
    let empty = std::fs::read_dir(&out).map_err(err)?.next().is_none();
    if !empty && previous(&out).is_none() {
        return Err("That folder isn't empty. Pick an empty folder, or one Marrow published to before.".into());
    }
    Ok(())
}

/// Writes one page (or the stylesheet, script, search index).
#[tauri::command]
pub fn site_write(out: String, rel: String, contents: String) -> CmdResult<()> {
    let dest = inside(Path::new(&out), &rel)?;
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    marrow_core::atomic::write(dest, contents).map_err(err)
}

/// Copies an image or other attachment from the vault into the site.
#[tauri::command]
pub fn site_copy(state: State<AppState>, out: String, rel: String, path: String) -> CmdResult<()> {
    let src = vp(&path)?;
    let dest = inside(Path::new(&out), &rel)?;
    let from = with_engine(&state, |e| Ok(src.to_os(e.vault().root())))?;
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    std::fs::copy(from, dest).map(|_| ()).map_err(err)
}

/// Opens the published site's home page in the default browser.
#[tauri::command]
pub fn site_open(app: tauri::AppHandle, out: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let index = Path::new(&out).join("index.html");
    if !index.is_file() {
        return Err("The website hasn't been published there yet.".into());
    }
    app.opener().open_path(index.to_string_lossy(), None::<&str>).map_err(err)
}

/// Removes pages from an earlier publish that weren't written this time, and
/// remembers what was. Returns how many old files were removed.
#[tauri::command]
pub fn site_finish(out: String, files: Vec<String>) -> CmdResult<usize> {
    let out = PathBuf::from(out);
    let now: BTreeSet<String> = files.into_iter().collect();
    let mut removed = 0;
    for old in previous(&out).unwrap_or_default() {
        if now.contains(&old) {
            continue;
        }
        if let Ok(p) = inside(&out, &old) {
            if std::fs::remove_file(&p).is_ok() {
                removed += 1;
                // Tidy folders left empty.
                let mut dir = p.parent().map(Path::to_path_buf);
                while let Some(d) = dir {
                    if d == out || std::fs::remove_dir(&d).is_err() {
                        break;
                    }
                    dir = d.parent().map(Path::to_path_buf);
                }
            }
        }
    }
    marrow_core::atomic::write(out.join(MANIFEST), serde_json::to_string_pretty(&now).map_err(err)?).map_err(err)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside() {
        let out = Path::new("/site");
        assert!(inside(out, "a/b.html").is_ok());
        assert!(inside(out, "../x").is_err());
        assert!(inside(out, "/etc/passwd").is_err());
        assert!(inside(out, "a/../../x").is_err());
        assert!(inside(out, "").is_err());
    }

    #[test]
    fn finish_removes_only_old_pages() {
        let t = tempfile::tempdir().unwrap();
        let out = t.path().to_string_lossy().to_string();
        site_write(out.clone(), "a.html".into(), "a".into()).unwrap();
        site_write(out.clone(), "dir/b.html".into(), "b".into()).unwrap();
        std::fs::write(t.path().join("mine.txt"), "not Marrow's").unwrap();
        site_finish(out.clone(), vec!["a.html".into(), "dir/b.html".into()]).unwrap();
        // Next publish: b is gone.
        let removed = site_finish(out.clone(), vec!["a.html".into()]).unwrap();
        assert_eq!(removed, 1);
        assert!(!t.path().join("dir").exists());
        assert!(t.path().join("a.html").exists());
        assert!(t.path().join("mine.txt").exists());
    }
}
