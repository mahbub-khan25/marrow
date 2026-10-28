//! Pretend Google Drive, OneDrive and Dropbox servers, small but following
//! each API, so whole syncs can be tested without an account.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

pub struct Request {
    pub method: String,
    /// Path and query, still %-encoded.
    pub target: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn path(&self) -> String {
        decode(self.target.split('?').next().unwrap_or_default())
    }
    pub fn query(&self, name: &str) -> Option<String> {
        let q = self.target.split_once('?')?.1;
        q.split('&').find_map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k == name).then(|| decode(&v.replace('+', " ")))
        })
    }
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_default()
    }
}

pub fn decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}

pub type Reply = (u16, Vec<(String, String)>, Vec<u8>);

pub fn json_reply(status: u16, v: Value) -> Reply {
    (status, vec![("Content-Type".into(), "application/json".into())], v.to_string().into_bytes())
}

/// Serves `handle` on a free port; returns `http://127.0.0.1:<port>`.
pub fn serve(handle: impl Fn(Request) -> Reply + Send + Sync + 'static) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let handle = Arc::new(handle);
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let handle = handle.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut w = s;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut parts = line.split_whitespace();
                    let (method, target) = (parts.next().unwrap_or_default().to_string(), parts.next().unwrap_or_default().to_string());
                    let mut headers = BTreeMap::new();
                    loop {
                        let mut h = String::new();
                        if r.read_line(&mut h).unwrap_or(0) == 0 {
                            return;
                        }
                        let h = h.trim_end();
                        if h.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = h.split_once(':') {
                            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                        }
                    }
                    let n: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let mut body = vec![0u8; n];
                    if r.read_exact(&mut body).is_err() {
                        return;
                    }
                    let (status, hs, out) = handle(Request { method, target, headers, body });
                    let mut head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", out.len());
                    for (k, v) in hs {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    if w.write_all(head.as_bytes()).and_then(|_| w.write_all(&out)).is_err() {
                        return;
                    }
                }
            });
        }
    });
    format!("http://{addr}")
}

/// Tokens: "good" works; anything else is turned down with 401.
pub fn authorised(r: &Request) -> bool {
    r.headers.get("authorization").map(String::as_str) == Some("Bearer good")
}

fn unauthorised() -> Reply {
    json_reply(401, json!({"error": {"code": "unauthenticated", "message": "bad token"}}))
}

static NEXT: AtomicUsize = AtomicUsize::new(1);

fn next() -> usize {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn now() -> String {
    // Each change a second later, so versions and times always differ.
    let n = next() as u64;
    format!("2026-10-27T{:02}:{:02}:{:02}Z", (n / 3600) % 24, (n / 60) % 60, n % 60)
}

// ---------------------------------------------------------------- Dropbox

#[derive(Default)]
pub struct DropboxState {
    /// lower-case path → (path as written, bytes, hash)
    pub files: BTreeMap<String, (String, Vec<u8>, String)>,
    /// lower-case folder path → path as written
    pub folders: BTreeMap<String, String>,
}

pub fn dropbox() -> (String, Arc<Mutex<DropboxState>>) {
    let state = Arc::new(Mutex::new(DropboxState::default()));
    let st = state.clone();
    let url = serve(move |r| {
        if !authorised(&r) {
            return unauthorised();
        }
        let mut s = st.lock().unwrap();
        let not_found = || json_reply(409, json!({"error_summary": "path/not_found/..", "error": {".tag": "path"}}));
        let add_parents = |s: &mut DropboxState, p: &str| {
            let mut cur = String::new();
            let parts: Vec<&str> = p.trim_start_matches('/').split('/').collect();
            for part in &parts[..parts.len() - 1] {
                cur.push('/');
                cur.push_str(part);
                s.folders.entry(cur.to_lowercase()).or_insert(cur.clone());
            }
        };
        match r.path().as_str() {
            "/2/files/list_folder" | "/2/files/list_folder/continue" => {
                let a = r.json();
                let path = a["path"].as_str().unwrap_or_default().to_lowercase();
                let recursive = a["recursive"].as_bool().unwrap_or(false);
                if !path.is_empty() && !s.folders.contains_key(&path) {
                    return not_found();
                }
                let prefix = format!("{path}/");
                let inside = |p: &str| p.starts_with(&prefix) && (recursive || !p[prefix.len()..].contains('/'));
                let mut entries = vec![];
                for (lower, display) in &s.folders {
                    if inside(lower) {
                        entries.push(json!({".tag": "folder", "name": display.rsplit('/').next(), "path_lower": lower, "path_display": display}));
                    }
                }
                for (lower, (display, bytes, hash)) in &s.files {
                    if inside(lower) {
                        entries.push(json!({".tag": "file", "name": display.rsplit('/').next(), "path_lower": lower, "path_display": display, "content_hash": hash, "size": bytes.len(), "server_modified": "2026-10-27T00:00:00Z"}));
                    }
                }
                json_reply(200, json!({"entries": entries, "cursor": "c", "has_more": false}))
            }
            "/2/files/get_metadata" => {
                let p = r.json()["path"].as_str().unwrap_or_default().to_lowercase();
                if s.folders.contains_key(&p) {
                    json_reply(200, json!({".tag": "folder"}))
                } else {
                    not_found()
                }
            }
            "/2/files/create_folder_v2" => {
                let p = r.json()["path"].as_str().unwrap_or_default().to_string();
                if s.folders.contains_key(&p.to_lowercase()) {
                    return json_reply(409, json!({"error_summary": "path/conflict/folder/.."}));
                }
                s.folders.insert(p.to_lowercase(), p);
                json_reply(200, json!({}))
            }
            "/2/files/download" => {
                let arg: Value = serde_json::from_str(r.headers.get("dropbox-api-arg").unwrap()).unwrap();
                let p = arg["path"].as_str().unwrap().to_lowercase();
                match s.files.get(&p) {
                    Some((_, b, _)) => (200, vec![], b.clone()),
                    None => not_found(),
                }
            }
            "/2/files/upload" => {
                let raw = r.headers.get("dropbox-api-arg").unwrap();
                assert!(raw.is_ascii(), "Dropbox-API-Arg must be ASCII");
                let arg: Value = serde_json::from_str(raw).unwrap();
                let p = arg["path"].as_str().unwrap().to_string();
                add_parents(&mut s, &p);
                let hash = format!("h{}", next());
                let display = s.files.get(&p.to_lowercase()).map(|f| f.0.clone()).unwrap_or(p.clone());
                s.files.insert(p.to_lowercase(), (display, r.body.clone(), hash.clone()));
                json_reply(200, json!({"content_hash": hash}))
            }
            "/2/files/delete_v2" => {
                let p = r.json()["path"].as_str().unwrap_or_default().to_lowercase();
                if s.files.remove(&p).is_some() {
                    return json_reply(200, json!({}));
                }
                if s.folders.remove(&p).is_some() {
                    let prefix = format!("{p}/");
                    s.files.retain(|k, _| !k.starts_with(&prefix));
                    s.folders.retain(|k, _| !k.starts_with(&prefix));
                    return json_reply(200, json!({}));
                }
                not_found()
            }
            "/2/users/get_current_account" => json_reply(200, json!({"email": "khan@example.com"})),
            other => panic!("Dropbox fake: no {other}"),
        }
    });
    (url, state)
}

// ---------------------------------------------------------------- OneDrive

#[derive(Default)]
pub struct OneDriveState {
    /// Paths below the app folder.
    pub files: BTreeMap<String, (Vec<u8>, String)>,
    pub folders: BTreeSet<String>,
    /// How many uploads failed because a folder was missing.
    pub missing_parent: usize,
}

pub fn onedrive() -> (String, Arc<Mutex<OneDriveState>>) {
    let state = Arc::new(Mutex::new(OneDriveState::default()));
    let st = state.clone();
    let url = serve(move |r| {
        if !authorised(&r) {
            return unauthorised();
        }
        let mut s = st.lock().unwrap();
        let path = r.path();
        let not_found = || json_reply(404, json!({"error": {"code": "itemNotFound", "message": "The resource could not be found."}}));
        if path == "/v1.0/me/drive" {
            return json_reply(200, json!({"owner": {"user": {"displayName": "Khan", "email": "khan@outlook.com"}}}));
        }
        let rest = path.strip_prefix("/v1.0/me/drive/special/approot").expect("approot");
        // "" | "/children" | ":/a/b:" | ":/a/b:/children" | ":/a/b:/content"
        let (item, suffix) = match rest.strip_prefix(":/") {
            Some(r) => {
                let (p, suf) = r.split_once(':').unwrap();
                (p.to_string(), suf.to_string())
            }
            None => (String::new(), rest.to_string()),
        };
        let parent_exists = |s: &OneDriveState, p: &str| p.rsplit_once('/').map(|(d, _)| s.folders.contains(d)).unwrap_or(true);
        let children = |s: &OneDriveState, dir: &str| {
            let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
            let mut v = vec![];
            for f in &s.folders {
                if let Some(n) = f.strip_prefix(&prefix) {
                    if !n.contains('/') && !n.is_empty() {
                        let count = s.files.keys().chain(s.folders.iter()).filter(|k| k.starts_with(&format!("{f}/")) && !k[f.len() + 1..].contains('/')).count();
                        v.push(json!({"name": n, "folder": {"childCount": count}}));
                    }
                }
            }
            for (f, (b, tag)) in &s.files {
                if let Some(n) = f.strip_prefix(&prefix) {
                    if !n.contains('/') {
                        v.push(json!({"name": n, "file": {}, "size": b.len(), "cTag": tag, "lastModifiedDateTime": "2026-10-27T00:00:00Z"}));
                    }
                }
            }
            v
        };
        match (r.method.as_str(), suffix.as_str()) {
            ("GET", "/children") => {
                if !item.is_empty() && !s.folders.contains(&item) {
                    return not_found();
                }
                // Two pages, to follow nextLink.
                let all = children(&s, &item);
                let page = r.query("page").map(|p| p == "2").unwrap_or(false);
                let half = all.len() / 2;
                if page {
                    json_reply(200, json!({"value": all[half..]}))
                } else {
                    let next = format!("http://{}{}?page=2", r.headers["host"], r.target.split('?').next().unwrap());
                    json_reply(200, json!({"value": all[..half], "@odata.nextLink": next}))
                }
            }
            ("POST", "/children") => {
                let name = r.json()["name"].as_str().unwrap().to_string();
                let p = if item.is_empty() { name } else { format!("{item}/{name}") };
                if !item.is_empty() && !s.folders.contains(&item) {
                    return not_found();
                }
                if !s.folders.insert(p) {
                    return json_reply(409, json!({"error": {"code": "nameAlreadyExists", "message": "exists"}}));
                }
                json_reply(201, json!({}))
            }
            ("GET", "") => {
                if s.folders.contains(&item) {
                    let count = children(&s, &item).len();
                    json_reply(200, json!({"id": "x", "folder": {"childCount": count}}))
                } else if s.files.contains_key(&item) {
                    json_reply(200, json!({"id": "x", "file": {}}))
                } else {
                    not_found()
                }
            }
            ("GET", "/content") => match s.files.get(&item) {
                Some((b, _)) => (200, vec![], b.clone()),
                None => not_found(),
            },
            ("PUT", "/content") => {
                if !parent_exists(&s, &item) {
                    s.missing_parent += 1;
                    return not_found();
                }
                let tag = format!("c{}", next());
                s.files.insert(item, (r.body.clone(), tag.clone()));
                json_reply(201, json!({"cTag": tag, "lastModifiedDateTime": now(), "size": r.body.len()}))
            }
            ("DELETE", "") => {
                if s.files.remove(&item).is_some() || s.folders.remove(&item) {
                    (204, vec![], vec![])
                } else {
                    not_found()
                }
            }
            (m, suf) => panic!("OneDrive fake: no {m} {suf} ({path})"),
        }
    });
    (url, state)
}

// ------------------------------------------------------------ Google Drive

#[derive(Debug, Clone)]
pub struct DriveFile {
    pub name: String,
    pub folder: bool,
    pub parents: Vec<String>,
    pub bytes: Vec<u8>,
    pub md5: String,
    pub trashed: bool,
    pub modified: String,
}

#[derive(Default)]
pub struct DriveState {
    pub files: BTreeMap<String, DriveFile>,
}

impl DriveState {
    /// The path of a file from My Drive, e.g. `Marrow/Notes/a.md`.
    pub fn path_of(&self, id: &str) -> String {
        let f = &self.files[id];
        match f.parents.first().map(String::as_str) {
            Some("root") | None => f.name.clone(),
            Some(p) => format!("{}/{}", self.path_of(p), f.name),
        }
    }
    pub fn live_paths(&self) -> Vec<String> {
        self.files.iter().filter(|(_, f)| !f.trashed && !f.folder).map(|(id, _)| self.path_of(id)).collect()
    }
}

fn drive_json(id: &str, f: &DriveFile) -> Value {
    let mut v = json!({"id": id, "name": f.name, "parents": f.parents, "modifiedTime": f.modified,
        "mimeType": if f.folder { "application/vnd.google-apps.folder" } else { "application/octet-stream" }});
    if !f.folder {
        v["md5Checksum"] = json!(f.md5);
        v["size"] = json!(f.bytes.len().to_string());
    }
    v
}

/// The few kinds of query Marrow sends.
fn drive_matches(q: &str, f: &DriveFile) -> bool {
    if f.trashed {
        return false;
    }
    for cond in q.split(" and ") {
        let cond = cond.trim();
        let quoted = |c: &str| c.split_once('\'').map(|(_, r)| r.rsplit_once('\'').unwrap().0.replace("\\'", "'"));
        let ok = if cond == "trashed = false" {
            true
        } else if cond.starts_with("name = ") {
            quoted(cond).as_deref() == Some(f.name.as_str())
        } else if cond.starts_with("mimeType = ") {
            f.folder
        } else if cond.ends_with(" in parents") {
            quoted(cond).is_some_and(|p| f.parents.contains(&p))
        } else {
            panic!("Drive fake: can't read {cond}")
        };
        if !ok {
            return false;
        }
    }
    true
}

fn parse_multipart(r: &Request) -> (Value, Vec<u8>) {
    let ty = &r.headers["content-type"];
    assert!(ty.starts_with("multipart/related"), "{ty}");
    let b = ty.split("boundary=").nth(1).unwrap();
    let sep = format!("--{b}\r\n");
    let body = &r.body;
    let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from);
    let first = find(body, sep.as_bytes(), 0).unwrap() + sep.len();
    let meta_start = find(body, b"\r\n\r\n", first).unwrap() + 4;
    let second = find(body, sep.as_bytes(), meta_start).unwrap();
    let meta: Value = serde_json::from_slice(&body[meta_start..second - 2]).unwrap();
    let data_start = find(body, b"\r\n\r\n", second + sep.len()).unwrap() + 4;
    let end = find(body, format!("\r\n--{b}--").as_bytes(), data_start).unwrap();
    (meta, body[data_start..end].to_vec())
}

pub fn gdrive() -> (String, Arc<Mutex<DriveState>>) {
    let state = Arc::new(Mutex::new(DriveState::default()));
    let st = state.clone();
    let url = serve(move |r| {
        if !authorised(&r) {
            return unauthorised();
        }
        let mut s = st.lock().unwrap();
        let path = r.path();
        let not_found = || json_reply(404, json!({"error": {"code": 404, "message": "File not found."}}));
        let new_id = || format!("id{}", next());
        match (r.method.as_str(), path.as_str()) {
            ("GET", "/drive/v3/about") => json_reply(200, json!({"user": {"emailAddress": "khan@gmail.com"}})),
            ("GET", "/drive/v3/files") => {
                let q = r.query("q").unwrap();
                let all: Vec<Value> = s.files.iter().filter(|(_, f)| drive_matches(&q, f)).map(|(id, f)| drive_json(id, f)).collect();
                // Pages of 3, to follow nextPageToken.
                let start: usize = r.query("pageToken").and_then(|p| p.parse().ok()).unwrap_or(0);
                let page: Vec<Value> = all.iter().skip(start).take(3).cloned().collect();
                let mut v = json!({"files": page});
                if start + 3 < all.len() {
                    v["nextPageToken"] = json!((start + 3).to_string());
                }
                json_reply(200, v)
            }
            ("POST", "/drive/v3/files") => {
                let m = r.json();
                assert_eq!(m["mimeType"], "application/vnd.google-apps.folder");
                let id = new_id();
                let parents = m["parents"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
                s.files.insert(id.clone(), DriveFile { name: m["name"].as_str().unwrap().into(), folder: true, parents, bytes: vec![], md5: String::new(), trashed: false, modified: now() });
                json_reply(200, json!({"id": id}))
            }
            ("POST", "/upload/drive/v3/files") => {
                assert_eq!(r.query("uploadType").as_deref(), Some("multipart"));
                let (meta, bytes) = parse_multipart(&r);
                let id = new_id();
                let parents: Vec<String> = meta["parents"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
                assert!(parents.iter().all(|p| s.files.get(p).is_some_and(|f| f.folder)), "parent folder must exist");
                let f = DriveFile { name: meta["name"].as_str().unwrap().into(), folder: false, parents, bytes, md5: format!("m{}", next()), trashed: false, modified: now() };
                let v = drive_json(&id, &f);
                s.files.insert(id, f);
                json_reply(200, v)
            }
            ("PATCH", p) if p.starts_with("/upload/drive/v3/files/") => {
                let id = p.rsplit('/').next().unwrap().to_string();
                let Some(f) = s.files.get_mut(&id).filter(|f| !f.trashed) else { return not_found() };
                f.bytes = r.body.clone();
                f.md5 = format!("m{}", next());
                f.modified = now();
                let v = drive_json(&id, f);
                json_reply(200, v)
            }
            ("PATCH", p) if p.starts_with("/drive/v3/files/") => {
                let id = p.rsplit('/').next().unwrap().to_string();
                let Some(f) = s.files.get_mut(&id) else { return not_found() };
                if r.json()["trashed"] == json!(true) {
                    f.trashed = true;
                }
                json_reply(200, json!({"id": id}))
            }
            ("GET", p) if p.starts_with("/drive/v3/files/") => {
                assert_eq!(r.query("alt").as_deref(), Some("media"));
                let id = p.rsplit('/').next().unwrap();
                match s.files.get(id).filter(|f| !f.trashed) {
                    Some(f) => (200, vec![], f.bytes.clone()),
                    None => not_found(),
                }
            }
            (m, p) => panic!("Drive fake: no {m} {p}"),
        }
    });
    (url, state)
}
