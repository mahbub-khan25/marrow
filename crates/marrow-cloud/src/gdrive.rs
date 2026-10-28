//! Google Drive: the vault is the folder `Marrow/<vault>` in My Drive.
//!
//! Marrow asks only for access to the files it creates itself (the
//! `drive.file` permission), so it never sees anything else in the Drive.
//! Drive finds files by ID, not by path, so each listing also maps paths to IDs.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::{Arc, Mutex};

use marrow_core::mirror::{Remote, RemoteFile, SyncStop, Version};
use reqwest::Method;
use serde_json::json;

use crate::http::{enc, hidden, stop, Api, Body, Reply};
use crate::oauth::TokenSource;
use crate::time;

const FOLDER: &str = "application/vnd.google-apps.folder";
/// The folder in My Drive that holds the vaults.
pub const TOP: &str = "Marrow";
const FIELDS: &str = "id,name,mimeType,parents,md5Checksum,modifiedTime,size";

#[derive(Debug, Clone)]
struct Item {
    id: String,
    version: Version,
    modified_ms: u64,
}

#[derive(Default)]
struct Map {
    loaded: bool,
    /// The vault folder's ID.
    vault: Option<String>,
    /// Path → file.
    files: HashMap<String, Item>,
    /// Folder path → ID ("" is the vault folder).
    dirs: HashMap<String, String>,
}

pub struct GoogleDrive {
    api: Api,
    base: String,
    account: String,
    vault: String,
    map: Mutex<Map>,
}

fn q(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn parent_of(key: &str) -> (&str, &str) {
    key.rsplit_once('/').unwrap_or(("", key))
}

impl GoogleDrive {
    pub fn new(token: Arc<dyn TokenSource>, account: &str, vault: &str) -> Self {
        Self::with_base(token, account, vault, "https://www.googleapis.com")
    }

    pub(crate) fn with_base(token: Arc<dyn TokenSource>, account: &str, vault: &str, base: &str) -> Self {
        GoogleDrive { api: Api::new(token), base: base.trim_end_matches('/').into(), account: account.into(), vault: vault.into(), map: Mutex::new(Map::default()) }
    }

    fn map(&self) -> std::sync::MutexGuard<'_, Map> {
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn check_reply(r: Reply, what: &str) -> io::Result<serde_json::Value> {
        if r.ok() {
            Ok(r.json())
        } else {
            Err(r.error(what))
        }
    }

    /// Every file and folder matching a Drive query (all pages).
    fn search(&self, query: &str) -> io::Result<Vec<serde_json::Value>> {
        let mut out = vec![];
        let mut page: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/drive/v3/files?q={}&fields={}&pageSize=1000&spaces=drive&orderBy=createdTime",
                self.base,
                enc(query),
                enc(&format!("nextPageToken,files({FIELDS})"))
            );
            if let Some(p) = &page {
                url.push_str(&format!("&pageToken={}", enc(p)));
            }
            let v = Self::check_reply(self.api.get(&url)?, "Google Drive")?;
            if let Some(files) = v.get("files").and_then(|f| f.as_array()) {
                out.extend(files.iter().cloned());
            }
            match v.get("nextPageToken").and_then(|p| p.as_str()) {
                Some(p) if !p.is_empty() => page = Some(p.to_string()),
                _ => return Ok(out),
            }
        }
    }

    fn folder_named(&self, name: &str, parent: &str) -> io::Result<Option<String>> {
        let found = self.search(&format!("name = '{}' and mimeType = '{FOLDER}' and '{}' in parents and trashed = false", q(name), q(parent)))?;
        // The oldest, if two devices each made one at the same moment.
        Ok(found.first().and_then(|f| f.get("id")).and_then(|i| i.as_str()).map(str::to_string))
    }

    fn make_folder(&self, name: &str, parent: &str) -> io::Result<String> {
        let r = self.api.call(
            Method::POST,
            &format!("{}/drive/v3/files?fields=id", self.base),
            &[],
            &Body::Json(json!({ "name": name, "mimeType": FOLDER, "parents": [parent] })),
        )?;
        let v = Self::check_reply(r, name)?;
        v.get("id").and_then(|i| i.as_str()).map(str::to_string).ok_or_else(|| io::Error::other("Google Drive didn't say which folder it made"))
    }

    fn top(&self, make: bool) -> io::Result<Option<String>> {
        match self.folder_named(TOP, "root")? {
            Some(id) => Ok(Some(id)),
            None if make => self.make_folder(TOP, "root").map(Some),
            None => Ok(None),
        }
    }

    /// The vault folder's ID, made if it isn't there yet.
    fn vault_id(&self) -> io::Result<String> {
        if let Some(id) = self.map().vault.clone() {
            return Ok(id);
        }
        let top = self.top(true)?.unwrap_or_default();
        let id = match self.folder_named(&self.vault, &top)? {
            Some(id) => id,
            None => self.make_folder(&self.vault, &top)?,
        };
        let mut m = self.map();
        m.vault = Some(id.clone());
        m.dirs.insert(String::new(), id.clone());
        Ok(id)
    }

    /// Reads every file under the vault folder into the map.
    fn load(&self) -> io::Result<()> {
        let vault = self.vault_id()?;
        // Marrow only sees the files it made, so one listing covers them all.
        let all = self.search("trashed = false")?;
        let mut children: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
        for f in &all {
            for p in f.get("parents").and_then(|p| p.as_array()).into_iter().flatten() {
                if let Some(p) = p.as_str() {
                    children.entry(p.to_string()).or_default().push(f);
                }
            }
        }
        let mut files: HashMap<String, Item> = HashMap::new();
        let mut dirs: HashMap<String, String> = HashMap::new();
        dirs.insert(String::new(), vault.clone());
        let mut todo = vec![(vault, String::new())];
        while let Some((id, path)) = todo.pop() {
            for f in children.get(&id).into_iter().flatten() {
                let (Some(fid), Some(name)) = (f.get("id").and_then(|v| v.as_str()), f.get("name").and_then(|v| v.as_str())) else { continue };
                let key = if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
                if f.get("mimeType").and_then(|m| m.as_str()) == Some(FOLDER) {
                    dirs.entry(key.clone()).or_insert_with(|| fid.to_string());
                    todo.push((fid.to_string(), key));
                    continue;
                }
                // Google Docs and the like have no file to download.
                if f.get("mimeType").and_then(|m| m.as_str()).is_some_and(|m| m.starts_with("application/vnd.google-apps.")) {
                    continue;
                }
                let modified_ms = time::ms(f.get("modifiedTime").and_then(|m| m.as_str()).unwrap_or_default());
                let version = match f.get("md5Checksum").and_then(|m| m.as_str()) {
                    Some(md5) => Version::Tag(md5.to_string()),
                    None => Version::Tag(format!("{modified_ms}-{}", f.get("size").and_then(|s| s.as_str()).unwrap_or("0"))),
                };
                let item = Item { id: fid.to_string(), version, modified_ms };
                // Two files with one name (made at once on two devices): the newer wins.
                match files.get(&key) {
                    Some(old) if old.modified_ms >= modified_ms => {}
                    _ => {
                        files.insert(key, item);
                    }
                }
            }
        }
        let mut m = self.map();
        m.files = files;
        m.dirs = dirs;
        m.loaded = true;
        Ok(())
    }

    fn item(&self, key: &str) -> io::Result<Option<Item>> {
        if !self.map().loaded {
            self.load()?;
        }
        Ok(self.map().files.get(key).cloned())
    }

    /// The ID of folder `path` in the vault, making it (and its parents) if needed.
    fn dir_id(&self, path: &str) -> io::Result<String> {
        if let Some(id) = self.map().dirs.get(path).cloned() {
            return Ok(id);
        }
        let (parent, name) = parent_of(path);
        let pid = self.dir_id(parent)?;
        let id = match self.folder_named(name, &pid)? {
            Some(id) => id,
            None => self.make_folder(name, &pid)?,
        };
        self.map().dirs.insert(path.to_string(), id.clone());
        Ok(id)
    }

    fn record(&self, key: &str, v: &serde_json::Value) -> io::Result<Version> {
        let id = v.get("id").and_then(|i| i.as_str()).ok_or_else(|| io::Error::other(format!("{key}: Google Drive didn't say what it saved")))?;
        let modified_ms = time::ms(v.get("modifiedTime").and_then(|m| m.as_str()).unwrap_or_default());
        let version = match v.get("md5Checksum").and_then(|m| m.as_str()) {
            Some(md5) => Version::Tag(md5.to_string()),
            None => Version::Tag(format!("{modified_ms}-{}", v.get("size").and_then(|s| s.as_str()).unwrap_or("0"))),
        };
        self.map().files.insert(key.to_string(), Item { id: id.to_string(), version: version.clone(), modified_ms });
        Ok(version)
    }

    /// Moves now-empty folders above `key` to the trash (not the vault folder).
    fn tidy(&self, key: &str) {
        let mut dir = parent_of(key).0.to_string();
        while !dir.is_empty() {
            let prefix = format!("{dir}/");
            let (empty, id) = {
                let m = self.map();
                let empty = !m.files.keys().any(|k| k.starts_with(&prefix)) && !m.dirs.keys().any(|k| k.starts_with(&prefix));
                (empty, m.dirs.get(&dir).cloned())
            };
            let Some(id) = id.filter(|_| empty) else { return };
            let trashed = self.api.call(Method::PATCH, &format!("{}/drive/v3/files/{id}?fields=id", self.base), &[], &Body::Json(json!({ "trashed": true })));
            if !trashed.map(|r| r.ok()).unwrap_or(false) {
                return;
            }
            self.map().dirs.remove(&dir);
            dir = parent_of(&dir).0.to_string();
        }
    }

    /// The vault folders inside `Marrow`.
    pub fn folders(&self) -> io::Result<Vec<String>> {
        let Some(top) = self.top(false)? else { return Ok(vec![]) };
        Ok(self
            .search(&format!("mimeType = '{FOLDER}' and '{}' in parents and trashed = false", q(&top)))?
            .iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect())
    }

    /// The signed-in person's email address.
    pub fn account(&self) -> io::Result<String> {
        let v = Self::check_reply(self.api.get(&format!("{}/drive/v3/about?fields=user(emailAddress,displayName)", self.base))?, "Google Drive")?;
        Ok(v.pointer("/user/emailAddress").or_else(|| v.pointer("/user/displayName")).and_then(|e| e.as_str()).unwrap_or("Google Drive").to_string())
    }
}

/// A multipart/related body: the file's details, then its bytes.
fn multipart(meta: &serde_json::Value, bytes: &[u8]) -> (Vec<u8>, String) {
    let boundary = format!("marrow-{:016x}", {
        let mut b = [0u8; 8];
        let _ = getrandom::getrandom(&mut b);
        u64::from_le_bytes(b)
    });
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/related; boundary={boundary}"))
}

impl Remote for GoogleDrive {
    fn id(&self) -> String {
        format!("gdrive:{}/{}", self.account, self.vault)
    }

    fn check(&self) -> Result<(), SyncStop> {
        self.vault_id().map(|_| ()).map_err(stop)
    }

    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        self.load()?;
        Ok(self
            .map()
            .files
            .iter()
            .filter(|(k, _)| !hidden(k))
            .map(|(k, i)| (k.clone(), RemoteFile { version: i.version.clone(), modified_ms: i.modified_ms }))
            .collect())
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let item = self.item(key)?.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{key}: not found")))?;
        let r = self.api.get(&format!("{}/drive/v3/files/{}?alt=media", self.base, item.id))?;
        if r.ok() {
            Ok(r.body)
        } else {
            Err(r.error(key))
        }
    }

    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let fields = format!("fields={}", enc(FIELDS));
        if let Some(item) = self.item(key)? {
            let r = self.api.call(
                Method::PATCH,
                &format!("{}/upload/drive/v3/files/{}?uploadType=media&{fields}", self.base, item.id),
                &[],
                &Body::Bytes(bytes.to_vec(), "application/octet-stream".into()),
            )?;
            // Gone since the last listing: make it again below.
            if r.status != 404 {
                let v = Self::check_reply(r, key)?;
                return self.record(key, &v);
            }
            self.map().files.remove(key);
        }
        let (dir, name) = parent_of(key);
        let parent = self.dir_id(dir)?;
        let (body, ty) = multipart(&json!({ "name": name, "parents": [parent] }), bytes);
        let r = self.api.call(Method::POST, &format!("{}/upload/drive/v3/files?uploadType=multipart&{fields}", self.base), &[], &Body::Bytes(body, ty))?;
        let v = Self::check_reply(r, key)?;
        self.record(key, &v)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let Some(item) = self.item(key)? else { return Ok(()) };
        // To Drive's trash, where the person can still get it back.
        let r = self.api.call(Method::PATCH, &format!("{}/drive/v3/files/{}?fields=id", self.base, item.id), &[], &Body::Json(json!({ "trashed": true })))?;
        if !r.ok() && r.status != 404 {
            return Err(r.error(key));
        }
        self.map().files.remove(key);
        self.tidy(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_body() {
        let (body, ty) = multipart(&json!({"name": "a.md"}), b"hello");
        let b = ty.split("boundary=").nth(1).unwrap();
        let text = String::from_utf8(body).unwrap();
        assert!(text.starts_with(&format!("--{b}\r\nContent-Type: application/json")));
        assert!(text.contains("{\"name\":\"a.md\"}"));
        assert!(text.ends_with(&format!("hello\r\n--{b}--\r\n")));
    }

    #[test]
    fn quotes_in_names() {
        assert_eq!(q("Khan's notes"), "Khan\\'s notes");
    }
}
