//! Commands the window can call, and the bridge that forwards engine events
//! to it.
//!
//! Lock order is always: app state → engine. The watcher thread only ever
//! takes the engine lock, so the two can't deadlock.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use marrow_core::cache::hash_bytes;
use marrow_core::{
    merge3, Backup, BackupStore, Engine, EngineEvent, GraphData, GraphOptions, IndexStats,
    LinkFormat, MergeOutcome, NoteInfo, RecentFile, RenameReport, SearchResults, TreeNode, VaultEvent,
    VaultPath, VaultService, VaultStats,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub service: Mutex<Option<VaultService>>,
    /// Two-way sync with a cloud folder, when it's on for the open vault.
    pub mirror: Mutex<Option<crate::sync::MirrorWorker>>,
}

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub(crate) fn vp(s: &str) -> CmdResult<VaultPath> {
    VaultPath::new(s).map_err(err)
}

/// Sends engine events produced by a command to the window, the same way
/// watcher events are sent. `skip_modified` drops the `modified` event for
/// a note the window itself just saved, so it doesn't look like an outside
/// edit.
pub(crate) fn emit_events(app: &AppHandle, mut events: Vec<EngineEvent>, skip_modified: Option<&VaultPath>) {
    if let Some(p) = skip_modified {
        events.retain(|e| !matches!(e, EngineEvent::Vault { event: VaultEvent::Modified { path } } if path == p));
    }
    if !events.is_empty() {
        let _ = app.emit("vault-changed", events);
    }
}

/// Runs `f` with the open vault's engine locked.
pub(crate) fn with_engine<T>(
    state: &State<AppState>,
    f: impl FnOnce(&mut Engine) -> CmdResult<T>,
) -> CmdResult<T> {
    let guard = state.service.lock().map_err(err)?;
    let svc = guard.as_ref().ok_or("No vault is open")?;
    let mut engine = svc.engine();
    f(&mut engine)
}

// ------------------------------------------------------------ settings

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    pub last_vault: Option<String>,
    /// Vault folder → the folder it's kept in sync with. Per computer, since
    /// the cloud folder's location differs from one computer to the next.
    pub mirrors: std::collections::BTreeMap<String, String>,
}

fn settings_path(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = match crate::portable::data("config") {
        Some(d) => d,
        None => app.path().app_config_dir().map_err(err)?,
    };
    Ok(dir.join("settings.json"))
}

pub(crate) fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub(crate) fn save_settings(app: &AppHandle, s: &Settings) -> CmdResult<()> {
    let p = settings_path(app)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(s).map_err(err)?).map_err(err)
}

// --------------------------------------------------------------- vault

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    root: String,
    name: String,
    notes: usize,
    stats: IndexStats,
}

/// The vault opened last time, if it still exists.
#[tauri::command]
pub fn last_vault(app: AppHandle) -> Option<String> {
    load_settings(&app)
        .last_vault
        .map(|p| crate::portable::from_stored(&p))
        .filter(|p| Path::new(p).is_dir())
        // Portable: a "Notes" folder next to the program opens by itself.
        .or_else(|| {
            let notes = crate::portable::root()?.join("Notes");
            notes.is_dir().then(|| notes.to_string_lossy().into_owned())
        })
}

/// A file in the app's cache folder for this vault ("<key>.<ext>"). In
/// portable mode the key ignores the drive letter, so the cache is reused.
pub(crate) fn vault_cache_file(app: &AppHandle, root: &Path, ext: &str) -> CmdResult<PathBuf> {
    let key = hash_bytes(crate::portable::to_stored(&root.to_string_lossy()).as_bytes());
    let cache_dir = match crate::portable::data("cache") {
        Some(d) => d,
        None => app.path().app_cache_dir().map_err(err)?,
    };
    Ok(cache_dir.join("vaults").join(format!("{}.{ext}", &key[..16])))
}

#[tauri::command]
pub fn open_vault(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<VaultInfo> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Folder not found: {path}"));
    }
    // The cache lives in the app's cache folder, one file per vault. (In
    // portable mode the key ignores the drive letter, so the cache is reused.)
    let cache = vault_cache_file(&app, &root, "redb")?;

    // Close the previous vault first so its watcher (and any sync) stops.
    crate::sync::stop_mirror(&state);
    drop(state.service.lock().map_err(err)?.take());

    let emitter = app.clone();
    let svc = VaultService::start(&root, &cache, move |events: Vec<EngineEvent>| {
        let _ = emitter.emit("vault-changed", events);
    })
    .map_err(err)?;

    let info = {
        let e = svc.engine();
        let root = e.vault().root().to_path_buf();
        VaultInfo {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            root: root.to_string_lossy().into_owned(),
            notes: e.vault().markdown_files().count(),
            stats: e.stats(),
        }
    };
    // Let the window load images and other files from this vault (and only this vault).
    let _ = app.asset_protocol_scope().allow_directory(&root, true);
    {
        let settings = read_vault_settings(&root);
        svc.engine().link_format = settings.link_format;
    }
    *state.service.lock().map_err(err)? = Some(svc);
    let mut settings = load_settings(&app);
    settings.last_vault = Some(crate::portable::to_stored(&info.root));
    let mirror = settings.mirrors.get(&info.root).cloned();
    save_settings(&app, &settings)?;
    if let Some(remote) = mirror {
        crate::sync::start_mirror(&app, &state, Path::new(&info.root), PathBuf::from(remote));
    }
    Ok(info)
}

#[tauri::command]
pub fn tree(state: State<AppState>) -> CmdResult<TreeNode> {
    with_engine(&state, |e| Ok(e.tree()))
}

// --------------------------------------------------------------- notes

#[tauri::command]
pub fn read_note(state: State<AppState>, path: String) -> CmdResult<String> {
    let p = vp(&path)?;
    with_engine(&state, |e| e.read_note(&p).map_err(err))
}

/// A new version is kept at most this often while a note is being edited.
const HISTORY_GAP_MS: i64 = 5 * 60 * 1000;

#[tauri::command]
pub fn write_note(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    text: String,
    expected: Option<String>,
) -> CmdResult<()> {
    let p = vp(&path)?;
    let backups = backup_store(&app)?;
    let result = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        // Refuse to overwrite a version we haven't seen: another app may have
        // written in the moment before the watcher told us.
        if let Some(expected) = &expected {
            if let Ok(current) = e.read_note(&p) {
                if &current != expected {
                    return Err(CHANGED_ON_DISK.to_string());
                }
            }
        }
        // Keep the text being replaced as a version, every few minutes at most.
        if let Ok(old) = e.read_note(&p) {
            let h = marrow_core::history::History::for_vault(&root);
            let _ = h.before_save(p.as_str(), &old, &text, marrow_core::history::now_ms(), HISTORY_GAP_MS);
        }
        match e.write_note(&p, &text) {
            Ok(events) => {
                backups.remove(&root, p.as_str());
                crate::sync::poke(&state);
                Ok(events)
            }
            Err(write_err) => {
                // Keep the text somewhere safe before reporting the failure.
                let kept = backups.put(&root, p.as_str(), &text).is_ok();
                let why = plain_io_error(&write_err);
                Err(if kept {
                    format!("{why} Your text was kept in a backup and will be offered when you reopen this note.")
                } else {
                    format!("{why} The backup failed too, so keep this window open and copy your text somewhere safe.")
                })
            }
        }
    })?;
    emit_events(&app, result, Some(&p));
    Ok(())
}

/// Says what went wrong with a file operation in words a person can act on.
fn plain_io_error(e: &marrow_core::Error) -> String {
    use std::io::ErrorKind::*;
    match e {
        marrow_core::Error::Io { source, .. } => match source.kind() {
            PermissionDenied => "The file is locked or read-only. Another program (a sync app or antivirus) may be using it.".into(),
            StorageFull => "The disk is full.".into(),
            NotFound => "The folder this note lives in is gone.".into(),
            ReadOnlyFilesystem => "The drive is read-only.".into(),
            _ => format!("The system said: {source}."),
        },
        other => format!("{other}."),
    }
}

/// Error text `write_note` returns when the file changed under the editor.
const CHANGED_ON_DISK: &str = "CHANGED_ON_DISK";

fn backup_store(app: &AppHandle) -> CmdResult<BackupStore> {
    Ok(BackupStore::new(
        match crate::portable::data("backups") {
            Some(d) => d,
            None => app.path().app_data_dir().map_err(err)?.join("backups"),
        },
    ))
}

/// Text kept from a failed save of this note, if any.
#[tauri::command]
pub fn backup_for(
    app: AppHandle,
    state: State<AppState>,
    path: String,
) -> CmdResult<Option<Backup>> {
    let store = backup_store(&app)?;
    with_engine(&state, |e| Ok(store.get(e.vault().root(), &path)))
}

#[tauri::command]
pub fn discard_backup(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    let store = backup_store(&app)?;
    with_engine(&state, |e| {
        store.remove(e.vault().root(), &path);
        Ok(())
    })
}

/// Three-way merge: `base` = last saved text, `mine` = editor, `theirs` = disk.
#[tauri::command]
pub fn merge_text(base: String, mine: String, theirs: String) -> MergeOutcome {
    merge3(&base, &mine, &theirs)
}

/// Picks `base.ext`, `base 1.ext`, `base 2.ext`... inside `folder`.
fn unique_child(e: &Engine, folder: &VaultPath, base: &str, ext: &str) -> CmdResult<VaultPath> {
    for n in 0..10_000 {
        let name = match (n, ext.is_empty()) {
            (0, true) => base.to_string(),
            (0, false) => format!("{base}.{ext}"),
            (n, true) => format!("{base} {n}"),
            (n, false) => format!("{base} {n}.{ext}"),
        };
        let p = folder.join(&name).map_err(err)?;
        if e.vault().get(&p).is_none() && !p.to_os(e.vault().root()).exists() {
            return Ok(p);
        }
    }
    Err("Could not find a free name".into())
}

/// Creates an empty note in `folder` (root if empty) and returns its path.
#[tauri::command]
pub fn create_note(
    app: AppHandle,
    state: State<AppState>,
    folder: String,
    name: Option<String>,
) -> CmdResult<String> {
    let folder = vp(&folder)?;
    let base = name.unwrap_or_else(|| "Untitled".into());
    let (p, events) = with_engine(&state, |e| {
        let p = unique_child(e, &folder, &base, "md")?;
        let ev = e.create_note(&p, "").map_err(err)?;
        Ok((p, ev))
    })?;
    emit_events(&app, events, None);
    Ok(p.to_string())
}

#[tauri::command]
pub fn create_folder(app: AppHandle, state: State<AppState>, parent: String) -> CmdResult<String> {
    let parent = vp(&parent)?;
    let (p, events) = with_engine(&state, |e| {
        let p = unique_child(e, &parent, "New folder", "")?;
        let ev = e.create_folder(&p).map_err(err)?;
        Ok((p, ev))
    })?;
    emit_events(&app, events, None);
    Ok(p.to_string())
}

/// Moves a file or folder to the vault's `.marrow/trash`.
#[tauri::command]
pub fn trash(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<()> {
    let p = vp(&path)?;
    let events = with_engine(&state, |e| e.trash(&p).map_err(err))?;
    emit_events(&app, events, None);
    Ok(())
}

/// Characters Windows (and so any vault synced to Windows) can't hold in a name.
const BAD_NAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Renames a file or folder in place (`new_name` is just the last segment,
/// without `.md` for notes) and rewrites links to it across the vault.
#[tauri::command]
pub fn rename(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    new_name: String,
) -> CmdResult<RenameReport> {
    let from = vp(&path)?;
    let name = new_name.trim();
    if name.is_empty() {
        return Err("A name can't be empty.".into());
    }
    if name.starts_with('.') {
        return Err("A name can't start with a dot; those files are hidden.".into());
    }
    if let Some(c) = name.chars().find(|c| BAD_NAME_CHARS.contains(c)) {
        return Err(format!("A name can't contain {c}"));
    }
    let (events, report) = with_engine(&state, |e| {
        let is_note =
            matches!(e.vault().get(&from), Some(marrow_core::Entry::File(_))) && from.is_markdown();
        let file_name = if is_note && !name.to_lowercase().ends_with(".md") {
            format!("{name}.md")
        } else {
            name.to_string()
        };
        let to = from.parent().join(&file_name).map_err(err)?;
        if to == from {
            return Ok((
                Vec::new(),
                RenameReport {
                    new_path: to,
                    ..Default::default()
                },
            ));
        }
        let case_only = to.as_str().to_lowercase() == from.as_str().to_lowercase();
        if !case_only && e.vault().get(&to).is_some() {
            return Err(format!("“{file_name}” already exists in this folder."));
        }
        let result = e.rename_with_links(&from, &to).map_err(err)?;
        backup_store(&app)?.rename(e.vault().root(), from.as_str(), to.as_str());
        let _ = marrow_core::history::History::for_vault(e.vault().root()).moved(from.as_str(), to.as_str());
        Ok(result)
    })?;
    emit_events(&app, events, None);
    Ok(report)
}

/// Moves a file or folder into another folder ("" = vault root), rewriting links.
#[tauri::command]
pub fn move_path(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    folder: String,
) -> CmdResult<RenameReport> {
    let from = vp(&path)?;
    let folder = vp(&folder)?;
    let to = folder.join(from.name()).map_err(err)?;
    if to == from {
        return Ok(RenameReport {
            new_path: to,
            ..Default::default()
        });
    }
    if folder.starts_with(&from) {
        return Err("A folder can't be moved inside itself.".into());
    }
    let (events, report) = with_engine(&state, |e| {
        if e.vault().get(&to).is_some() {
            return Err(format!("“{}” already exists there.", from.name()));
        }
        let result = e.rename_with_links(&from, &to).map_err(err)?;
        backup_store(&app)?.rename(e.vault().root(), from.as_str(), to.as_str());
        let _ = marrow_core::history::History::for_vault(e.vault().root()).moved(from.as_str(), to.as_str());
        Ok(result)
    })?;
    emit_events(&app, events, None);
    Ok(report)
}

#[tauri::command]
pub fn search(
    state: State<AppState>,
    query: String,
    limit: Option<usize>,
) -> CmdResult<SearchResults> {
    with_engine(&state, |e| Ok(e.search(&query, limit.unwrap_or(100))))
}

/// Places where other notes name this one (or one of `names`) without linking.
#[tauri::command]
pub fn unlinked_mentions(state: State<AppState>, path: String, names: Vec<String>) -> CmdResult<Vec<marrow_core::engine::Mention>> {
    let p = vp(&path)?;
    with_engine(&state, |e| Ok(e.unlinked_mentions(&p, &names)))
}

#[derive(Serialize)]
pub struct TagCount {
    tag: String,
    count: usize,
}

/// Every tag in the vault with how many notes use it, most used first.
#[tauri::command]
pub fn tags(state: State<AppState>) -> CmdResult<Vec<TagCount>> {
    with_engine(&state, |e| {
        let mut v: Vec<TagCount> = e
            .tags()
            .into_iter()
            .map(|(tag, count)| TagCount { tag, count })
            .collect();
        v.sort_by(|a, b| a.tag.cmp(&b.tag));
        Ok(v)
    })
}

#[tauri::command]
pub fn graph(state: State<AppState>, options: Option<GraphOptions>) -> CmdResult<GraphData> {
    with_engine(&state, |e| Ok(e.graph_data(options.unwrap_or_default())))
}

// --------------------------------------------------------------- links

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkItem {
    line: u32,
    ch: u32,
    /// The whole source line, trimmed, for context.
    snippet: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkGroup {
    source: String,
    title: String,
    items: Vec<BacklinkItem>,
}

#[tauri::command]
pub fn backlinks(state: State<AppState>, path: String) -> CmdResult<Vec<BacklinkGroup>> {
    let p = vp(&path)?;
    with_engine(&state, |e| {
        let mut out = Vec::new();
        for b in e.backlinks(&p) {
            let text = e.read_note(&b.source).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let items = b
                .refs
                .iter()
                .map(|r| {
                    let line = lines.get(r.pos.start.line as usize).copied().unwrap_or("");
                    // The editor counts UTF-16 units, not bytes.
                    let col = (r.pos.start.col as usize).min(line.len());
                    let ch = line
                        .get(..col)
                        .map(|s| s.encode_utf16().count() as u32)
                        .unwrap_or(0);
                    BacklinkItem {
                        line: r.pos.start.line,
                        ch,
                        snippet: line.trim().chars().take(240).collect(),
                    }
                })
                .collect();
            out.push(BacklinkGroup {
                title: b.source.stem().to_string(),
                source: b.source.to_string(),
                items,
            });
        }
        Ok(out)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLinks {
    /// Resolved outgoing targets.
    outgoing: Vec<String>,
    /// Link texts (without `.md`) that point nowhere yet.
    unresolved: Vec<String>,
}

#[tauri::command]
pub fn note_links(state: State<AppState>, path: String) -> CmdResult<NoteLinks> {
    let p = vp(&path)?;
    with_engine(&state, |e| {
        Ok(NoteLinks {
            outgoing: e.outgoing(&p).keys().map(|k| k.to_string()).collect(),
            unresolved: e.unresolved(&p).keys().cloned().collect(),
        })
    })
}

#[tauri::command]
pub fn resolve_link(
    state: State<AppState>,
    link: String,
    source: String,
) -> CmdResult<Option<String>> {
    let src = vp(&source)?;
    with_engine(&state, |e| {
        Ok(e.resolve_link(&link, &src).map(|p| p.to_string()))
    })
}

/// Resolves a link, creating the target note at the vault root (or at the
/// path the link names) if it doesn't exist yet. Returns the target path.
#[tauri::command]
pub fn follow_link(
    app: AppHandle,
    state: State<AppState>,
    link: String,
    source: String,
) -> CmdResult<String> {
    let src = vp(&source)?;
    let (path, events) = with_engine(&state, |e| {
        if let Some(p) = e.resolve_link(&link, &src) {
            return Ok((p.to_string(), Vec::new()));
        }
        let target = marrow_core::parse::split_subpath(&link)
            .0
            .trim()
            .trim_start_matches('/');
        if target.is_empty() {
            return Err("Empty link".into());
        }
        // Links name notes without ".md"; never create attachments from a link.
        let path = if target.to_lowercase().ends_with(".md") {
            vp(target)?
        } else {
            vp(&format!("{target}.md"))?
        };
        let ev = e.create_note(&path, "").map_err(err)?;
        Ok((path.to_string(), ev))
    })?;
    emit_events(&app, events, None);
    Ok(path)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    path: String,
    name: String,
    folder: String,
    is_note: bool,
    /// Text to put inside `[[...]]` when linking from `source`.
    link_text: String,
}

#[tauri::command]
pub fn find_files(
    state: State<AppState>,
    query: String,
    source: Option<String>,
    limit: Option<usize>,
) -> CmdResult<Vec<FileHit>> {
    let src = match source.as_deref() {
        Some(s) if !s.is_empty() => vp(s)?,
        _ => VaultPath::root(),
    };
    with_engine(&state, |e| {
        Ok(e.find_files(&query, limit.unwrap_or(30))
            .into_iter()
            .map(|p| FileHit {
                name: if p.is_markdown() {
                    p.stem().to_string()
                } else {
                    p.name().to_string()
                },
                folder: p.parent().to_string(),
                is_note: p.is_markdown(),
                link_text: e.link_text(&p, &src),
                path: p.to_string(),
            })
            .collect())
    })
}

// ------------------------------------------------------------ vault info

#[tauri::command]
pub fn note_info(state: State<AppState>, path: String) -> CmdResult<Option<NoteInfo>> {
    let p = vp(&path)?;
    with_engine(&state, |e| Ok(e.note_info(&p)))
}

#[tauri::command]
pub fn recent_files(state: State<AppState>, limit: Option<usize>) -> CmdResult<Vec<RecentFile>> {
    with_engine(&state, |e| Ok(e.recent_files(limit.unwrap_or(20))))
}

/// The notes in a folder as database rows (for database views).
#[tauri::command]
pub fn query_notes(state: State<AppState>, folder: String, recursive: Option<bool>) -> CmdResult<marrow_core::QueryResult> {
    let f = vp(folder.trim_matches('/'))?;
    with_engine(&state, |e| Ok(e.query(&f, recursive.unwrap_or(false))))
}

#[tauri::command]
pub fn vault_stats(state: State<AppState>) -> CmdResult<VaultStats> {
    with_engine(&state, |e| Ok(e.vault_stats()))
}

/// Creates a file of any type (e.g. a `.canvas`) with a unique name in `folder`.
#[tauri::command]
pub fn create_file(
    app: AppHandle,
    state: State<AppState>,
    folder: String,
    name: String,
    ext: String,
    contents: String,
) -> CmdResult<String> {
    let folder = vp(&folder)?;
    let (p, events) = with_engine(&state, |e| {
        let p = unique_child(e, &folder, name.trim(), ext.trim_start_matches('.'))?;
        let ev = e.create_note(&p, &contents).map_err(err)?;
        Ok((p, ev))
    })?;
    emit_events(&app, events, None);
    Ok(p.to_string())
}

/// Copies a file from anywhere on the computer into `folder` in the vault
/// (made unique), e.g. a cover image picked with the file dialog. Files
/// already inside the vault are not copied; their vault path is returned.
#[tauri::command]
pub fn import_file(app: AppHandle, state: State<AppState>, source: String, folder: String) -> CmdResult<String> {
    let src = PathBuf::from(&source);
    let folder = vp(&folder)?;
    let (p, events) = with_engine(&state, |e| {
        let root = e.vault().root().to_path_buf();
        if let Ok(canon) = dunce::canonicalize(&src) {
            if let Ok(rel) = canon.strip_prefix(&root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                return Ok((vp(&rel)?, Vec::new()));
            }
        }
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let p = unique_child(e, &folder, &stem, &ext)?;
        let dest = p.to_os(&root);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        std::fs::copy(&src, &dest).map_err(err)?;
        let ev = e.on_paths_changed(std::slice::from_ref(&p)).map_err(err)?;
        Ok((p, ev))
    })?;
    emit_events(&app, events, None);
    Ok(p.to_string())
}

/// Saves pasted or dropped file bytes (the request body) into `folder` and
/// returns the new file's vault path. Headers: `name` (URL-encoded file name)
/// and `folder`.
#[tauri::command]
pub fn save_attachment(app: AppHandle, state: State<AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected the file's bytes".into());
    };
    let header = |k: &str| {
        request
            .headers()
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(percent_decode)
            .unwrap_or_default()
    };
    store_attachment(&app, &state, &header("name"), &header("folder"), bytes)
}

/// Writes `bytes` as a new file named like `name` in `folder`.
fn store_attachment(app: &AppHandle, state: &State<AppState>, name: &str, folder: &str, bytes: &[u8]) -> CmdResult<String> {
    let folder = vp(folder)?;
    // Only the file name, never a path.
    let name = name.rsplit(['/', '\\']).next().unwrap_or("").trim().to_string();
    let name = if name.is_empty() { "Pasted file".to_string() } else { name };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, x)) if !s.is_empty() && !x.is_empty() && x.len() <= 8 => (s.to_string(), x.to_lowercase()),
        _ => (name.clone(), String::new()),
    };
    let stem: String = stem.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c }).collect();
    let (p, events) = with_engine(state, |e| {
        let root = e.vault().root().to_path_buf();
        let p = unique_child(e, &folder, &stem, &ext)?;
        let dest = p.to_os(&root);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        std::fs::write(&dest, bytes).map_err(err)?;
        let ev = e.on_paths_changed(std::slice::from_ref(&p)).map_err(err)?;
        Ok((p, ev))
    })?;
    emit_events(app, events, None);
    Ok(p.to_string())
}

/// Saves the image on the system clipboard as a PNG, for when the webview
/// doesn't hand pasted images to the page (WebKitGTK). None if there's no image.
#[tauri::command]
pub fn save_clipboard_image(app: AppHandle, state: State<AppState>, name: String, folder: String) -> CmdResult<Option<String>> {
    let Ok(mut cb) = arboard::Clipboard::new() else { return Ok(None) };
    let Ok(img) = cb.get_image() else { return Ok(None) };
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, img.width as u32, img.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(err)?;
        w.write_image_data(&img.bytes).map_err(err)?;
    }
    store_attachment(&app, &state, &name, &folder, &png_bytes).map(Some)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Writes an exported file (HTML) wherever the person chose to save it.
#[tauri::command]
pub fn write_export(path: String, contents: String) -> CmdResult<()> {
    std::fs::write(&path, contents).map_err(err)
}

/// Every to-do in the vault (open ones, or all).
#[tauri::command]
pub fn tasks(state: State<AppState>, include_done: bool) -> CmdResult<Vec<marrow_core::engine::TaskItem>> {
    with_engine(&state, |e| Ok(e.tasks(include_done)))
}

// --------------------------------------------------------- version history

/// Earlier versions of a note, newest first.
#[tauri::command]
pub fn history_list(state: State<AppState>, path: String) -> CmdResult<Vec<marrow_core::history::Version>> {
    let p = vp(&path)?;
    with_engine(&state, |e| Ok(marrow_core::history::History::for_vault(e.vault().root()).list(p.as_str())))
}

#[tauri::command]
pub fn history_read(state: State<AppState>, path: String, time_ms: i64) -> CmdResult<String> {
    let p = vp(&path)?;
    with_engine(&state, |e| marrow_core::history::History::for_vault(e.vault().root()).read(p.as_str(), time_ms).map_err(err))
}

/// Keeps the note's current text as a version now (before a restore, say).
#[tauri::command]
pub fn history_snapshot(state: State<AppState>, path: String, text: String) -> CmdResult<()> {
    let p = vp(&path)?;
    with_engine(&state, |e| {
        marrow_core::history::History::for_vault(e.vault().root())
            .snapshot(p.as_str(), &text, marrow_core::history::now_ms())
            .map(|_| ())
            .map_err(err)
    })
}

// -------------------------------------------------------- vault settings

/// Settings that belong to a vault (stored in `<vault>/.marrow/app.json`
/// so they travel with it).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultSettings {
    pub link_format: LinkFormat,
    /// "root", "current" (the open note's folder) or "folder".
    pub new_note_location: String,
    pub new_note_folder: String,
    pub daily_folder: String,
    /** Notes in this folder are offered as templates (/ menu, buttons). */
    pub template_folder: String,
    /// Starred notes, shown at the top of the file tree.
    pub starred: Vec<String>,
}

impl Default for VaultSettings {
    fn default() -> Self {
        VaultSettings {
            link_format: LinkFormat::Shortest,
            new_note_location: "current".into(),
            new_note_folder: String::new(),
            daily_folder: "Daily".into(),
            template_folder: "Templates".into(),
            starred: Vec::new(),
        }
    }
}

fn vault_settings_path(root: &Path) -> PathBuf {
    root.join(marrow_core::vault::CONFIG_DIR).join("app.json")
}

fn read_vault_settings(root: &Path) -> VaultSettings {
    std::fs::read(vault_settings_path(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_vault_settings(state: State<AppState>) -> CmdResult<VaultSettings> {
    with_engine(&state, |e| Ok(read_vault_settings(e.vault().root())))
}

#[tauri::command]
pub fn set_vault_settings(state: State<AppState>, settings: VaultSettings) -> CmdResult<()> {
    with_engine(&state, |e| {
        let path = vault_settings_path(e.vault().root());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        std::fs::write(&path, serde_json::to_vec_pretty(&settings).map_err(err)?).map_err(err)?;
        e.link_format = settings.link_format;
        Ok(())
    })
}

#[cfg(test)]
mod attachment_tests {
    use super::percent_decode;

    #[test]
    fn decodes_file_names() {
        assert_eq!(percent_decode("Pasted%20image%201.png"), "Pasted image 1.png");
        assert_eq!(percent_decode("%E5%9B%BE.png"), "图.png");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%2"), "a%2");
    }
}
