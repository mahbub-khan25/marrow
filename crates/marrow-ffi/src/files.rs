//! Opening a vault, and listing, creating, renaming, moving and deleting
//! its files and folders.

use std::path::{Path, PathBuf};

use marrow_core::vault_locks as vl;
use marrow_core::{Engine, Entry, VaultPath, VaultService};

use crate::{fail, settings, vp, Marrow, MarrowError, Result};

#[derive(Debug, Clone, uniffi::Record)]
pub struct VaultInfo {
    /// Full path of the vault folder.
    pub root: String,
    pub name: String,
    pub notes: u32,
    /// What opening had to repair, in words (usually empty).
    pub repaired: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FileKind {
    Folder,
    Note,
    Canvas,
    Image,
    Pdf,
    Audio,
    Other,
}

/// A file or folder, as a list shows it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FileItem {
    pub path: String,
    /// The name to show: notes and canvases without their extension.
    pub name: String,
    pub kind: FileKind,
    pub modified_ms: i64,
    pub size: u64,
    /// Things inside, for a folder.
    pub items: u32,
    /// The first words of a note (plain text), for the list.
    pub preview: String,
    /// The note is password-locked.
    pub locked: bool,
}

/// What renaming or moving did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MoveReport {
    pub new_path: String,
    /// Links in other notes that were updated to point at the new place.
    pub links_updated: u32,
    pub files_updated: u32,
}

/// What an import brought in.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ImportReport {
    pub folder: String,
    pub notes: u32,
    pub attachments: u32,
    pub warnings: Vec<String>,
}

pub(crate) fn kind_of(path: &str, folder: bool) -> FileKind {
    if folder {
        return FileKind::Folder;
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "md" => FileKind::Note,
        "canvas" => FileKind::Canvas,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "svg" | "avif" => FileKind::Image,
        "pdf" => FileKind::Pdf,
        "m4a" | "mp3" | "wav" | "ogg" | "webm" | "aac" | "flac" | "opus" => FileKind::Audio,
        _ => FileKind::Other,
    }
}

/// `folder/Note.md` → `Note`; `Map.canvas` → `Map`; `img/a.png` → `a.png`.
pub fn display_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".md") {
        name[..name.len() - 3].to_string()
    } else if lower.ends_with(".canvas") {
        name[..name.len() - 7].to_string()
    } else {
        name.to_string()
    }
}

fn is_heading(line: &str) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
}

/// The first words of a note as plain text: no frontmatter, headings marks,
/// list bullets or link brackets.
pub(crate) fn preview_of(text: &str, max: usize) -> String {
    let mut body = text;
    if let Some(rest) = body.strip_prefix("---\n").or_else(|| body.strip_prefix("---\r\n")) {
        if let Some(end) = rest.find("\n---") {
            body = &rest[end + 4..];
            body = body.split_once('\n').map(|(_, r)| r).unwrap_or("");
        }
    }
    let mut out = String::new();
    for line in body.lines() {
        let t = line.trim();
        // Headings name the parts; the words under them say more.
        if t.is_empty() || t.starts_with("```") || t == "---" || t.starts_with('|') || is_heading(t) {
            continue;
        }
        let t = t.trim_start_matches('>').trim();
        let t = t
            .strip_prefix("- [ ] ")
            .or_else(|| t.strip_prefix("- [x] "))
            .or_else(|| t.strip_prefix("- [X] "))
            .or_else(|| t.strip_prefix("- "))
            .or_else(|| t.strip_prefix("* "))
            .unwrap_or(t);
        if t.is_empty() {
            continue;
        }
        let mut clean = String::with_capacity(t.len());
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '[' | ']' | '*' | '_' | '`' | '=' | '~' => {}
                '|' => {
                    // [[Note|alias]] shows the alias: drop what came before.
                    if let Some(i) = clean.rfind(' ').map(|i| i + 1).or(Some(0)) {
                        clean.truncate(i);
                    }
                }
                '!' if chars.peek() == Some(&'[') => {}
                _ => clean.push(c),
            }
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(clean.trim());
        if out.chars().count() >= max {
            break;
        }
    }
    if out.chars().count() > max {
        out = out.chars().take(max).collect::<String>().trim_end().to_string() + "…";
    }
    out
}

/// The first words of a note as plain text, for a list or a card.
#[uniffi::export]
pub fn preview(text: String, max: u32) -> String {
    preview_of(&text, max as usize)
}

/// The start of a file, enough for a preview, without reading all of it.
fn head(path: &Path, bytes: usize) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    buf.truncate(n);
    // Cut at a whole character.
    match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            String::from_utf8_lossy(&e.into_bytes()[..valid]).into_owned()
        }
    }
}

/// Characters Windows (and so any vault synced to Windows) can't hold in a name.
const BAD_NAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

pub(crate) fn check_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(fail("A name can't be empty."));
    }
    if name.starts_with('.') {
        return Err(fail("A name can't start with a dot; those files are hidden."));
    }
    if let Some(c) = name.chars().find(|c| BAD_NAME_CHARS.contains(c)) {
        return Err(fail(format!("A name can't contain {c}")));
    }
    Ok(())
}

/// Picks `base.ext`, `base 1.ext`, `base 2.ext`… inside `folder`.
pub(crate) fn unique_child(e: &Engine, folder: &VaultPath, base: &str, ext: &str) -> Result<VaultPath> {
    for n in 0..10_000 {
        let name = match (n, ext.is_empty()) {
            (0, true) => base.to_string(),
            (0, false) => format!("{base}.{ext}"),
            (n, true) => format!("{base} {n}"),
            (n, false) => format!("{base} {n}.{ext}"),
        };
        let p = folder.join(&name)?;
        if e.vault().get(&p).is_none() && !p.to_os(e.vault().root()).exists() {
            return Ok(p);
        }
    }
    Err(fail("Could not find a free name"))
}

/// Notes a new vault starts with, so the app isn't empty on first open.
const WELCOME: &str = "# Welcome to Marrow\n\nEvery note is a plain Markdown file in a folder you own. Marrow saves as you type.\n\n## Try these\n- [ ] Tap **Today** to start today's note\n- [ ] Link notes by typing `[[`, like [[Ideas]]\n- [ ] Add a #tag, then find it under Tags\n- [ ] Ask your notes a question (set up an AI in Settings first)\n\n> [!tip] Sync\n> To see these notes on your computer too, set up sync in Settings.\n";
const IDEAS: &str = "# Ideas\n\nA place for ideas. This note is linked from [[Welcome]], so it shows that link under *Linked from*.\n\n- Notes can link to each other\n- #ideas\n";

impl Marrow {
    fn start(&self, root: &Path) -> Result<VaultInfo> {
        if !root.is_dir() {
            return Err(fail(format!("Folder not found: {}", root.display())));
        }
        let cache = self.cache_file(root, "redb");
        if let Some(d) = cache.parent() {
            std::fs::create_dir_all(d)?;
        }
        self.stop_sync();
        if let Ok(mut s) = self.service.lock() {
            s.take();
        }
        let listener = self.listener.clone();
        let svc = VaultService::start(root, &cache, move |events| crate::tell(&listener, events))
            .or_else(|_| {
                // A cache that can't be opened (left by another version) is rebuilt.
                let _ = std::fs::remove_file(&cache);
                let listener = self.listener.clone();
                VaultService::start(root, &cache, move |events| crate::tell(&listener, events))
            })?;
        let info = {
            let mut e = svc.engine();
            let root = e.vault().root().to_path_buf();
            let mut repaired = e.take_recovered();
            if let Err(x) = vl::tidy(&root) {
                repaired.push(format!("locked folders: {x}"));
            }
            e.link_format = settings::link_format(&settings::read_vault_settings(&root).link_format);
            VaultInfo {
                name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                root: root.to_string_lossy().into_owned(),
                notes: e.vault().markdown_files().count() as u32,
                repaired,
            }
        };
        *self.service.lock().map_err(|_| fail("busy"))? = Some(svc);
        let root_s = info.root.clone();
        self.update_settings(|s| s.last_vault = Some(root_s))?;
        self.resume_sync();
        Ok(info)
    }
}

#[uniffi::export]
impl Marrow {
    /// Opens the vault in `path` (a folder), closing any open one.
    pub fn open_vault(&self, path: String) -> Result<VaultInfo> {
        self.start(Path::new(&path))
    }

    /// The vault opened last time, if it's still there.
    pub fn last_vault(&self) -> Option<String> {
        self.app_settings().last_vault.filter(|p| Path::new(p).is_dir())
    }

    /// Makes a new vault folder `name` inside `parent`, with a welcome note,
    /// and returns its path (an existing empty folder is used as it is).
    pub fn create_vault(&self, parent: String, name: String, welcome: bool) -> Result<String> {
        let name = name.trim();
        check_name(name)?;
        let dir = PathBuf::from(parent).join(name);
        if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(fail(format!("There's already a folder called “{name}” there.")));
        }
        std::fs::create_dir_all(&dir)?;
        if welcome {
            marrow_core::atomic::write(dir.join("Welcome.md"), WELCOME)?;
            marrow_core::atomic::write(dir.join("Ideas.md"), IDEAS)?;
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    /// The open vault, if any.
    pub fn vault_info(&self) -> Option<VaultInfo> {
        self.with(|e| {
            let root = e.vault().root().to_path_buf();
            Ok(VaultInfo {
                name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                root: root.to_string_lossy().into_owned(),
                notes: e.vault().markdown_files().count() as u32,
                repaired: vec![],
            })
        })
        .ok()
    }

    /// Closes the vault (stops sync and watching).
    pub fn close_vault(&self) {
        self.stop_sync();
        if let Ok(mut s) = self.service.lock() {
            s.take();
        }
    }

    /// Looks for files changed by other apps (call when the app comes back).
    pub fn rescan(&self) -> Result<()> {
        let events = self.with(|e| Ok(e.rescan()?))?;
        self.tell(events);
        Ok(())
    }

    /// The files and folders in `folder` ("" for the top of the vault).
    /// Folders first, then by name; the app re-sorts as the person likes.
    pub fn list_folder(&self, folder: String) -> Result<Vec<FileItem>> {
        let f = vp(&folder)?;
        self.with(|e| {
            let root = e.vault().root().to_path_buf();
            let mut out = Vec::new();
            for (p, entry) in e.vault().children(&f) {
                if p.is_hidden() {
                    continue;
                }
                out.push(item(e, &root, p, entry));
            }
            out.sort_by(|a, b| (a.kind != FileKind::Folder).cmp(&(b.kind != FileKind::Folder)).then_with(|| natural(&a.name, &b.name)));
            Ok(out)
        })
    }

    /// One file or folder, as a list shows it.
    pub fn file_item(&self, path: String) -> Result<FileItem> {
        let p = vp(&path)?;
        self.with(|e| {
            let root = e.vault().root().to_path_buf();
            let entry = *e.vault().get(&p).ok_or_else(|| fail(format!("“{}” isn't in the vault.", display_name(&path))))?;
            Ok(item(e, &root, &p, &entry))
        })
    }

    /// Every note, newest change first.
    pub fn all_notes(&self, limit: u32) -> Result<Vec<FileItem>> {
        self.with(|e| {
            let root = e.vault().root().to_path_buf();
            let mut notes: Vec<(VaultPath, Entry)> = e
                .vault()
                .files()
                .filter(|p| !p.is_hidden() && matches!(kind_of(p.as_str(), false), FileKind::Note | FileKind::Canvas))
                .filter_map(|p| e.vault().get(p).map(|en| (p.clone(), *en)))
                .collect();
            let mtime = |en: &Entry| match en {
                Entry::File(s) => s.mtime_ms,
                _ => 0,
            };
            notes.sort_by_key(|(_, en)| std::cmp::Reverse(mtime(en)));
            notes.truncate(limit as usize);
            Ok(notes.iter().map(|(p, en)| item(e, &root, p, en)).collect())
        })
    }

    /// Every folder in the vault ("" first), for "Move to…".
    pub fn all_folders(&self) -> Result<Vec<String>> {
        self.with(|e| {
            let mut out = vec![String::new()];
            let mut folders: Vec<String> = e
                .vault()
                .entries()
                .filter(|(p, en)| matches!(en, Entry::Folder) && !p.is_root() && !p.is_hidden())
                .map(|(p, _)| p.to_string())
                .collect();
            folders.sort_by(|a, b| natural(a, b));
            out.extend(folders);
            Ok(out)
        })
    }

    /// The full path of a vault file, to show an image or share a file.
    pub fn file_path(&self, path: String) -> Result<String> {
        let p = vp(&path)?;
        Ok(p.to_os(&self.root()?).to_string_lossy().into_owned())
    }

    /// Makes an empty note named `name` (or "Untitled") in `folder`, with
    /// `text`; returns its path.
    pub fn create_note(&self, folder: String, name: Option<String>, text: String) -> Result<String> {
        let folder = vp(&folder)?;
        let base = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Untitled".into());
        check_name(&base)?;
        let (p, events) = self.with(|e| {
            let p = unique_child(e, &folder, &base, "md")?;
            let body = vl::conceal(&self.locks, e.vault().root(), p.as_str(), None, &text)?;
            let ev = e.create_note(&p, &body)?;
            Ok((p, ev))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(p.to_string())
    }

    /// Makes a file (`ext` "canvas", "md"…) with `text` in it; returns its path.
    pub fn create_file(&self, folder: String, name: String, ext: String, text: String) -> Result<String> {
        let folder = vp(&folder)?;
        let name = name.trim();
        check_name(name)?;
        let (p, events) = self.with(|e| {
            let p = unique_child(e, &folder, name, &ext)?;
            let body = if p.is_markdown() { vl::conceal(&self.locks, e.vault().root(), p.as_str(), None, &text)? } else { text };
            Ok((p.clone(), e.create_note(&p, &body)?))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(p.to_string())
    }

    /// Makes a folder in `parent`; returns its path.
    pub fn create_folder(&self, parent: String, name: String) -> Result<String> {
        let parent = vp(&parent)?;
        let name = name.trim();
        check_name(name)?;
        let (p, events) = self.with(|e| {
            let p = unique_child(e, &parent, name, "")?;
            Ok((p.clone(), e.create_folder(&p)?))
        })?;
        self.tell(events);
        Ok(p.to_string())
    }

    /// Renames a file or folder in place (`new_name` without `.md` for
    /// notes) and updates links to it across the vault.
    pub fn rename(&self, path: String, new_name: String) -> Result<MoveReport> {
        let from = vp(&path)?;
        let name = new_name.trim();
        check_name(name)?;
        let to = self.with(|e| {
            let is_note = matches!(e.vault().get(&from), Some(Entry::File(_))) && from.is_markdown();
            let is_canvas = from.as_str().to_ascii_lowercase().ends_with(".canvas");
            let file_name = if is_note && !name.to_lowercase().ends_with(".md") {
                format!("{name}.md")
            } else if is_canvas && !name.to_lowercase().ends_with(".canvas") {
                format!("{name}.canvas")
            } else {
                name.to_string()
            };
            Ok(from.parent().join(&file_name)?)
        })?;
        self.move_to(from, to, true)
    }

    /// Moves a file or folder into `folder` ("" for the top), updating links.
    pub fn move_path(&self, path: String, folder: String) -> Result<MoveReport> {
        let from = vp(&path)?;
        let folder = vp(&folder)?;
        if folder.starts_with(&from) {
            return Err(fail("A folder can't be moved inside itself."));
        }
        let to = folder.join(from.name())?;
        self.move_to(from, to, false)
    }

    /// Moves a file or folder to the vault's trash (`.marrow/trash`).
    pub fn trash(&self, path: String) -> Result<()> {
        let p = vp(&path)?;
        let events = self.with(|e| Ok(e.trash(&p)?))?;
        self.tell(events);
        self.poke_sync();
        Ok(())
    }

    /// A copy of a note beside it ("Note 1"); returns the copy's path.
    pub fn duplicate(&self, path: String) -> Result<String> {
        let p = vp(&path)?;
        let (copy, events) = self.with(|e| {
            let raw = e.read_note(&p)?;
            let ext = p.extension();
            let copy = unique_child(e, &p.parent(), &format!("{} copy", display_name(p.as_str())), &ext)?;
            Ok((copy.clone(), e.create_note(&copy, &raw)?))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(copy.to_string())
    }

    /// Saves a photo, recording or other file into `folder` (made if
    /// needed) as `name`, renamed if taken; returns its vault path.
    pub fn save_attachment(&self, folder: String, name: String, bytes: Vec<u8>) -> Result<String> {
        let folder = vp(&folder)?;
        let clean: String = name.chars().map(|c| if BAD_NAME_CHARS.contains(&c) { '-' } else { c }).collect();
        let clean = clean.trim().trim_start_matches('.').to_string();
        let (stem, ext) = match clean.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), e.to_string()),
            _ => (if clean.is_empty() { "Attachment".to_string() } else { clean.clone() }, String::new()),
        };
        let (p, events) = self.with(|e| {
            let root = e.vault().root().to_path_buf();
            std::fs::create_dir_all(folder.to_os(&root))?;
            let p = unique_child(e, &folder, &stem, &ext)?;
            marrow_core::atomic::create(p.to_os(&root), &bytes)?;
            Ok((p.clone(), e.on_paths_changed(&[p])?))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(p.to_string())
    }

    /// Imports files exported from another app into a new folder named
    /// like `folder` ("Notion", "Notion 2"…). `kind`: "notion", "evernote",
    /// "word", "html" or "markdown"; `paths` are full paths of the files.
    pub fn import_files(&self, kind: String, paths: Vec<String>, folder: String) -> Result<ImportReport> {
        use marrow_core::import;
        if paths.is_empty() {
            return Err(fail("Pick something to import."));
        }
        let root = self.root()?;
        let base = import::clean_name(&folder);
        let folder = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") }).find(|n| !root.join(n).exists()).unwrap_or(base);
        let mut out = import::Output::default();
        for p in paths.iter().map(PathBuf::from) {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
            let bytes = std::fs::read(&p).map_err(|e| fail(format!("Couldn't read {name}: {e}")))?;
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let result = match (kind.as_str(), ext.as_str()) {
                ("notion", _) | (_, "zip") => {
                    let sub = if paths.len() > 1 { format!("{folder}/{}", import::clean_name(&stem)) } else { folder.clone() };
                    import::notion::import(&bytes, &sub).map(|o| {
                        out.summary.notes += o.summary.notes;
                        out.summary.attachments += o.summary.attachments;
                        out.summary.warnings.extend(o.summary.warnings);
                        out.files.extend(o.files);
                    })
                }
                ("evernote", _) | (_, "enex") => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    import::enex::import(&text, &format!("{folder}/{}", import::clean_name(&stem)), &mut out).map(|_| ())
                }
                ("word", _) | (_, "docx") => import::docx::import(&bytes, &stem, &folder, &mut out),
                ("html", _) | (_, "html") | (_, "htm") => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    import::html::import(&text, &stem, &folder, &mut out, &|_: &str| None)
                }
                (_, "md") | (_, "markdown") | (_, "txt") => {
                    let path = out.reserve(&folder, &stem, "md");
                    out.note(path, String::from_utf8_lossy(&bytes).to_string());
                    Ok(())
                }
                _ => Err(format!("Marrow can't import “{name}”.")),
            };
            if let Err(e) = result {
                if paths.len() == 1 {
                    return Err(fail(e));
                }
                out.warn(format!("{name}: {e}"));
            }
        }
        let events = self.with(|e| {
            let root = e.vault().root().to_path_buf();
            let mut written: Vec<VaultPath> = Vec::new();
            for f in &out.files {
                let p = VaultPath::new(&f.path)?;
                let dest = p.to_os(&root);
                if dest.exists() {
                    continue;
                }
                if let Some(dir) = dest.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let bytes = match std::str::from_utf8(&f.bytes) {
                    Ok(text) if p.is_markdown() => vl::conceal(&self.locks, &root, p.as_str(), None, text)?.into_bytes(),
                    _ => f.bytes.clone(),
                };
                marrow_core::atomic::create(&dest, &bytes)?;
                written.push(p);
            }
            Ok(e.on_paths_changed(&written)?)
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(ImportReport {
            folder,
            notes: out.summary.notes as u32,
            attachments: out.summary.attachments as u32,
            warnings: out.summary.warnings,
        })
    }
}

impl Marrow {
    fn move_to(&self, from: VaultPath, to: VaultPath, rename: bool) -> Result<MoveReport> {
        if to == from {
            return Ok(MoveReport { new_path: to.to_string(), links_updated: 0, files_updated: 0 });
        }
        let backups = self.backups();
        let (events, report) = self.with(|e| {
            let case_only = rename && to.as_str().to_lowercase() == from.as_str().to_lowercase();
            if !case_only && e.vault().get(&to).is_some() {
                return Err(fail(format!("“{}” already exists there.", to.name())));
            }
            let root = e.vault().root().to_path_buf();
            vl::check_move(&self.locks, &root, from.as_str(), to.as_str())?;
            let (mut events, sealed) = vl::seal_for_move(&self.locks, e, &from, &to, Some(&backups))?;
            vl::wipe_plain_copies(e, &sealed, None);
            vl::moving(&root, from.as_str(), to.as_str())?;
            let (moved, report) = e.rename_with_links(&from, &to).map_err(|x| {
                let _ = vl::tidy(&root);
                MarrowError::from(x)
            })?;
            events.extend(moved);
            backups.rename(&root, from.as_str(), to.as_str());
            let _ = marrow_core::history::History::for_vault(&root).moved(from.as_str(), to.as_str());
            vl::moved(&root, from.as_str(), to.as_str())?;
            self.seen_moved(&root, from.as_str(), to.as_str());
            Ok((events, report))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(MoveReport {
            new_path: report.new_path.to_string(),
            links_updated: report.links_updated as u32,
            files_updated: report.files_updated.len() as u32,
        })
    }
}

fn item(e: &Engine, root: &Path, p: &VaultPath, entry: &Entry) -> FileItem {
    let folder = matches!(entry, Entry::Folder);
    let kind = kind_of(p.as_str(), folder);
    let (modified_ms, size) = match entry {
        Entry::File(s) => (s.mtime_ms, s.size),
        _ => (0, 0),
    };
    let mut locked = false;
    let preview = if kind == FileKind::Note {
        let text = head(&p.to_os(root), 2048);
        if marrow_core::lock::is_locked(&text) {
            locked = true;
            String::new()
        } else {
            preview_of(&text, 160)
        }
    } else {
        String::new()
    };
    let items = if folder { e.vault().children(p).into_iter().filter(|(c, _)| !c.is_hidden()).count() as u32 } else { 0 };
    FileItem { path: p.to_string(), name: display_name(p.as_str()), kind, modified_ms, size, items, preview, locked }
}

/// Compares names the way people expect: "Note 2" before "Note 10".
pub(crate) fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let o = n.trim_start_matches('0').len().cmp(&m.trim_start_matches('0').len()).then_with(|| n.trim_start_matches('0').cmp(m.trim_start_matches('0')));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_plain_words() {
        let t = "---\ntags: [a]\n---\n# Title\n\n- [ ] Buy **milk** and [[Bread|bread]]\n> quote here\n";
        assert_eq!(preview_of(t, 200), "Buy milk and bread quote here");
        assert_eq!(preview_of("abcdef", 3), "abc…");
    }

    #[test]
    fn names_sort_naturally() {
        let mut v = vec!["Note 10", "note 2", "Note 1", "apple"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["apple", "Note 1", "note 2", "Note 10"]);
        assert_eq!(display_name("a/b/Map.canvas"), "Map");
        assert_eq!(kind_of("x.JPG", false), FileKind::Image);
    }

    #[test]
    fn vaults_open_and_files_move() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::marrow(dir.path());
        let root = m.create_vault(dir.path().join("vaults").to_string_lossy().into(), "My notes".into(), true).unwrap();
        let info = m.open_vault(root.clone()).unwrap();
        assert_eq!(info.notes, 2);
        let list = m.list_folder("".into()).unwrap();
        assert_eq!(list.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Ideas", "Welcome"]);
        assert!(list[1].preview.starts_with("Every note is a plain Markdown file"));
        let f = m.create_folder("".into(), "Projects".into()).unwrap();
        let r = m.move_path("Ideas.md".into(), f.clone()).unwrap();
        assert_eq!(r.new_path, "Projects/Ideas.md");
        // [[Ideas]] still finds it.
        assert_eq!(r.links_updated, 0);
        let r = m.rename("Projects/Ideas.md".into(), "Big ideas".into()).unwrap();
        assert_eq!(r.new_path, "Projects/Big ideas.md");
        assert!(m.read_note("Welcome.md".into()).unwrap().contains("[[Big ideas]]"));
        assert_eq!(m.all_folders().unwrap(), ["", "Projects"]);
        let a = m.save_attachment("Attachments".into(), "photo.jpg".into(), vec![1, 2, 3]).unwrap();
        assert_eq!(a, "Attachments/photo.jpg");
        assert_eq!(m.save_attachment("Attachments".into(), "photo.jpg".into(), vec![1]).unwrap(), "Attachments/photo 1.jpg");
        assert!(m.rename("Welcome.md".into(), "a/b".into()).is_err());
    }
}
