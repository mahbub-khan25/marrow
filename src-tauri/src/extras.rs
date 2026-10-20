//! CSS snippets and plugins kept in the vault: `.marrow/snippets/*.css` and
//! `.marrow/plugins/<id>/` (`manifest.json` + `main.js`). They travel with
//! the vault. Which ones are on is a vault setting; the window applies them.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::{err, with_engine, AppState, CmdResult};

fn dir(root: &Path, kind: &str) -> CmdResult<PathBuf> {
    match kind {
        "snippets" | "plugins" => Ok(root.join(marrow_core::vault::CONFIG_DIR).join(kind)),
        _ => Err(format!("Unknown kind: {kind}")),
    }
}

fn plain_rel(rel: &str) -> CmdResult<&Path> {
    let p = Path::new(rel);
    if rel.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("Not a plain path: {rel}"));
    }
    Ok(p)
}

/// The folder (created if needed), for "Open folder".
#[tauri::command(async)]
pub fn extras_dir(state: State<AppState>, kind: String) -> CmdResult<String> {
    let d = with_engine(&state, |e| dir(e.vault().root(), &kind))?;
    std::fs::create_dir_all(&d).map_err(err)?;
    Ok(d.to_string_lossy().into_owned())
}

#[derive(Serialize)]
pub struct Snippet {
    name: String,
    css: String,
}

/// Every `.css` file in the snippets folder, by name.
#[tauri::command(async)]
pub fn snippets_list(state: State<AppState>) -> CmdResult<Vec<Snippet>> {
    let d = with_engine(&state, |e| dir(e.vault().root(), "snippets"))?;
    let Ok(rd) = std::fs::read_dir(&d) else { return Ok(Vec::new()) };
    let mut out: Vec<Snippet> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".css")?.to_string();
            let css = std::fs::read_to_string(e.path()).ok()?;
            Some(Snippet { name: stem, css })
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    Ok(out)
}

#[derive(Serialize)]
pub struct Plugin {
    /// The plugin's folder name.
    dir: String,
    manifest: Option<serde_json::Value>,
    code: Option<String>,
    /// Why it can't be loaded, if it can't.
    problem: Option<String>,
}

/// Every plugin folder, with its manifest and code.
#[tauri::command(async)]
pub fn plugins_list(state: State<AppState>) -> CmdResult<Vec<Plugin>> {
    let d = with_engine(&state, |e| dir(e.vault().root(), "plugins"))?;
    let Ok(rd) = std::fs::read_dir(&d) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for e in rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()) {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let manifest = std::fs::read_to_string(p.join("manifest.json")).ok();
        let code = std::fs::read_to_string(p.join("main.js")).ok();
        let (manifest, problem) = match manifest.map(|m| serde_json::from_str::<serde_json::Value>(&m)) {
            None => (None, Some("manifest.json is missing".to_string())),
            Some(Err(e)) => (None, Some(format!("manifest.json isn't valid JSON: {e}"))),
            Some(Ok(v)) => (Some(v), code.is_none().then(|| "main.js is missing".to_string())),
        };
        out.push(Plugin { dir: name, manifest, code, problem });
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(out)
}

/// Writes a file in the snippets or plugins folder (for the examples).
/// Returns false if it already existed (it's left alone).
#[tauri::command(async)]
pub fn extras_write(state: State<AppState>, kind: String, rel: String, contents: String) -> CmdResult<bool> {
    let d = with_engine(&state, |e| dir(e.vault().root(), &kind))?;
    let dest = d.join(plain_rel(&rel)?);
    if dest.exists() {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    marrow_core::atomic::create(dest, contents).map_err(err)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_paths() {
        assert!(plain_rel("a.css").is_ok());
        assert!(plain_rel("p/main.js").is_ok());
        assert!(plain_rel("../x").is_err());
        assert!(plain_rel("/x").is_err());
        assert!(dir(Path::new("/v"), "other").is_err());
    }
}

/// Opens the folder with Marrow's diagnostic log, to attach to a bug report.
#[tauri::command(async)]
pub fn open_log_folder(app: AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let dir = match crate::portable::data("logs") {
        Some(d) => d,
        None => app.path().app_log_dir().map_err(err)?,
    };
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(err)
}
