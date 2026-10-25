//! Import from Notion, Evernote, Word/OneNote and web pages: reads the
//! files the person picked, converts them (marrow_core::import) and writes
//! the notes into a new folder in the vault.

use std::path::{Path, PathBuf};

use marrow_core::import::{self, Output, Summary};
use marrow_core::VaultPath;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::commands::{emit_events, err, with_engine, AppState, CmdResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// The folder the notes went into.
    pub folder: String,
    #[serde(flatten)]
    pub summary: Summary,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into())
}

/// Converts the picked files. `kind`: notion, evernote, word or html.
fn convert(kind: &str, paths: &[PathBuf], folder: &str, app: &AppHandle) -> Result<Output, String> {
    let mut out = Output::default();
    let total = paths.len();
    for (i, p) in paths.iter().enumerate() {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let _ = app.emit("import-progress", json!({ "done": i, "total": total, "current": name }));
        let bytes = std::fs::read(p).map_err(|e| format!("Couldn't read {name}: {e}"))?;
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let result = match (kind, ext.as_str()) {
            ("notion", _) | (_, "zip") => {
                let sub = if paths.len() > 1 { format!("{folder}/{}", import::clean_name(&stem(p))) } else { folder.to_string() };
                import::notion::import(&bytes, &sub).map(|o| merge(&mut out, o))
            }
            ("evernote", _) | (_, "enex") => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let sub = format!("{folder}/{}", import::clean_name(&stem(p)));
                import::enex::import(&text, &sub, &mut out).map(|_| ())
            }
            ("word", _) | (_, "docx") => import::docx::import(&bytes, &stem(p), folder, &mut out),
            ("html", _) | (_, "html") | (_, "htm") => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                let load = |rel: &str| -> Option<Vec<u8>> {
                    let target = dir.join(rel);
                    // Only files next to the page (or below it).
                    let canon = dunce::canonicalize(&target).ok()?;
                    let base = dunce::canonicalize(&dir).ok()?;
                    canon.starts_with(&base).then(|| std::fs::read(canon).ok()).flatten()
                };
                import::html::import(&text, &stem(p), folder, &mut out, &load)
            }
            (_, "md") | (_, "markdown") | (_, "txt") => {
                let path = out.reserve(folder, &stem(p), "md");
                out.note(path, String::from_utf8_lossy(&bytes).to_string());
                Ok(())
            }
            _ => Err(format!("Marrow can't import “{name}”.")),
        };
        if let Err(e) = result {
            if paths.len() == 1 {
                return Err(e);
            }
            out.warn(format!("{name}: {e}"));
        }
    }
    Ok(out)
}

fn merge(into: &mut Output, from: Output) {
    into.summary.notes += from.summary.notes;
    into.summary.attachments += from.summary.attachments;
    into.summary.warnings.extend(from.summary.warnings);
    into.files.extend(from.files);
}

/// An export file found in Downloads, ready to import.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundExport {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
}

/// Is `name` an export from `kind` ("notion": Export-….zip, "evernote": .enex)?
pub fn looks_like_export(kind: &str, name: &str) -> bool {
    let lower = name.to_lowercase();
    match kind {
        "notion" => lower.ends_with(".zip") && (lower.starts_with("export-") || lower.contains("notion")),
        "evernote" => lower.ends_with(".enex"),
        _ => false,
    }
}

/// Recent exports from Notion or Evernote in the Downloads folder, newest first.
#[tauri::command(async)]
pub fn find_exports(app: AppHandle, kind: String) -> CmdResult<Vec<FoundExport>> {
    use tauri::Manager;
    let Ok(dir) = app.path().download_dir() else { return Ok(Vec::new()) };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let month_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(45 * 86_400);
    let mut out: Vec<FoundExport> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !looks_like_export(&kind, &name) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            if !meta.is_file() || modified < month_ago {
                return None;
            }
            let modified_ms = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64;
            Some(FoundExport { path: e.path().to_string_lossy().to_string(), name, size: meta.len(), modified_ms })
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.modified_ms));
    out.truncate(6);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::looks_like_export;

    #[test]
    fn recognises_exports() {
        assert!(looks_like_export("notion", "Export-8f2a1c3e-9b7d-4e1a-b2c3-d4e5f6a7b8c9.zip"));
        assert!(looks_like_export("notion", "My Notion backup.zip"));
        assert!(!looks_like_export("notion", "photos.zip"));
        assert!(looks_like_export("evernote", "Recipes.enex"));
        assert!(!looks_like_export("evernote", "Recipes.zip"));
    }
}

/// Imports `paths` into a new folder named like `folder` ("Notion", "Notion 2"…).
#[tauri::command]
pub async fn import_files(app: AppHandle, state: State<'_, AppState>, locks: State<'_, crate::locks::Locks>, kind: String, paths: Vec<String>, folder: String) -> CmdResult<ImportReport> {
    if paths.is_empty() {
        return Err("Pick something to import.".into());
    }
    // A folder name that's free.
    let base = import::clean_name(&folder);
    let folder = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        for n in 1.. {
            let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
            if !root.join(&name).exists() {
                return Ok(name);
            }
        }
        unreachable!()
    })?;
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let app2 = app.clone();
    let folder2 = folder.clone();
    let out = tauri::async_runtime::spawn_blocking(move || convert(&kind, &paths, &folder2, &app2)).await.map_err(err)??;
    let _ = app.emit("import-progress", json!({ "writing": out.files.len() }));
    let events = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        let mut written: Vec<VaultPath> = Vec::new();
        for f in &out.files {
            let vp = VaultPath::new(&f.path).map_err(err)?;
            let dest = vp.to_os(&root);
            if dest.exists() {
                continue;
            }
            if let Some(dir) = dest.parent() {
                std::fs::create_dir_all(dir).map_err(err)?;
            }
            // Notes landing in a locked folder (or a locked vault) are sealed.
            let bytes = match std::str::from_utf8(&f.bytes) {
                Ok(text) if vp.is_markdown() => {
                    crate::locks::conceal(&locks, &root, vp.as_str(), None, text)?.into_bytes()
                }
                _ => f.bytes.clone(),
            };
            marrow_core::atomic::create(&dest, &bytes).map_err(err)?;
            written.push(vp);
        }
        e.on_paths_changed(&written).map_err(err)
    })?;
    emit_events(&app, events, None);
    Ok(ImportReport { folder, summary: out.summary })
}
