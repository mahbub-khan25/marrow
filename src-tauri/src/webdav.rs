//! Sync with a WebDAV server: Nextcloud, ownCloud, Synology, pCloud, Koofr
//! and many others. Notes stay plain files in a folder on the server.
//!
//! The sync engine (marrow-core's mirror) decides what to copy; this only
//! lists, reads, writes and deletes files on the server.

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

use marrow_core::mirror::{skip_name, Remote, RemoteFile, SyncStop, Version};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::blocking::{Client, Response};
use reqwest::{Method, StatusCode, Url};

/// One entry in a server's folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The decoded path on the server, e.g. `/remote.php/dav/files/me/Marrow/a b.md`.
    pub path: String,
    pub is_dir: bool,
    pub etag: Option<String>,
    pub modified_ms: u64,
    pub size: u64,
}

pub struct WebDav {
    /// The sync folder, always ending in `/`.
    base: Url,
    user: String,
    password: String,
    client: Client,
}

impl WebDav {
    pub fn new(url: &str, user: &str, password: &str) -> Result<Self, String> {
        let mut base = Url::parse(url.trim()).map_err(|_| "That doesn't look like a web address. It usually starts with https://".to_string())?;
        if !matches!(base.scheme(), "https" | "http") {
            return Err("The address should start with https:// (or http:// on your own network).".into());
        }
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("Marrow/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(WebDav { base, user: user.into(), password: password.into(), client })
    }

    fn url_for(&self, key: &str) -> Url {
        let mut u = self.base.clone();
        if let Ok(mut segs) = u.path_segments_mut() {
            segs.pop_if_empty();
            for part in key.split('/').filter(|p| !p.is_empty()) {
                segs.push(part);
            }
        }
        u
    }

    fn send(&self, method: Method, url: Url, depth: Option<&str>, body: Option<Vec<u8>>) -> io::Result<Response> {
        let mut req = self.client.request(method, url).basic_auth(&self.user, Some(&self.password));
        if let Some(d) = depth {
            req = req.header("Depth", d).header("Content-Type", "application/xml; charset=utf-8");
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        req.send().map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, short(&e)))
    }

    fn propfind(&self, url: Url, depth: &str) -> io::Result<(StatusCode, Vec<Entry>)> {
        const BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getetag/><d:getlastmodified/><d:getcontentlength/></d:prop></d:propfind>"#;
        let r = self.send(Method::from_bytes(b"PROPFIND").expect("method"), url, Some(depth), Some(BODY.as_bytes().to_vec()))?;
        let status = r.status();
        if !status.is_success() {
            return Ok((status, Vec::new()));
        }
        let text = r.text().map_err(|e| io::Error::other(short(&e)))?;
        Ok((status, parse_multistatus(&text)))
    }

    /// Makes the folders `key` sits in, one level at a time.
    fn make_parents(&self, key: &str) -> io::Result<()> {
        let parts: Vec<&str> = key.split('/').collect();
        for i in 1..parts.len() {
            let dir = format!("{}/", parts[..i].join("/"));
            let r = self.send(Method::from_bytes(b"MKCOL").expect("method"), self.url_for(&dir), None, None)?;
            // 405: it already exists.
            if !(r.status().is_success() || r.status() == StatusCode::METHOD_NOT_ALLOWED) {
                return Err(http_error(r.status(), &dir));
            }
        }
        Ok(())
    }

    /// Makes the sync folder, one level at a time from the top (folders that
    /// already exist are skipped).
    fn make_base(&self) -> Result<(), String> {
        let segments: Vec<String> = self.base.path_segments().map(|s| s.filter(|p| !p.is_empty()).map(str::to_string).collect()).unwrap_or_default();
        let mut last = StatusCode::OK;
        for i in 1..=segments.len() {
            let mut u = self.base.clone();
            u.set_path(&format!("/{}/", segments[..i].join("/")));
            let r = self.send(Method::from_bytes(b"MKCOL").expect("method"), u, None, None).map_err(|e| e.to_string())?;
            last = r.status();
        }
        // 201 made it; 405 means it was already there.
        if last.is_success() || last == StatusCode::METHOD_NOT_ALLOWED {
            Ok(())
        } else {
            Err(format!("couldn't make the sync folder (the server answered {last})"))
        }
    }

    fn version_of(&self, key: &str) -> io::Result<Version> {
        let (status, entries) = self.propfind(self.url_for(key), "0")?;
        if !status.is_success() {
            return Err(http_error(status, key));
        }
        entries.first().map(version).ok_or_else(|| io::Error::other(format!("{key}: no details from the server")))
    }
}

fn version(e: &Entry) -> Version {
    match &e.etag {
        Some(t) => Version::Tag(t.trim_matches('"').trim_start_matches("W/").trim_matches('"').to_string()),
        None => Version::Tag(format!("{}-{}", e.modified_ms, e.size)),
    }
}

fn short(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the server took too long to answer".into()
    } else if e.is_connect() {
        "couldn't connect to the server".into()
    } else {
        e.to_string()
    }
}

fn http_error(status: StatusCode, what: &str) -> io::Error {
    let why = match status.as_u16() {
        401 | 403 => "the server turned down the user name or password".to_string(),
        404 => "not found on the server".to_string(),
        507 => "the server is out of space".to_string(),
        _ => format!("the server answered {status}"),
    };
    io::Error::other(format!("{what}: {why}"))
}

impl Remote for WebDav {
    fn id(&self) -> String {
        format!("webdav:{}@{}", self.user, self.base)
    }

    fn check(&self) -> Result<(), SyncStop> {
        let (status, _) = self
            .propfind(self.base.clone(), "0")
            .map_err(|e| SyncStop::Offline { message: e.to_string() })?;
        match status.as_u16() {
            200..=299 => Ok(()),
            401 | 403 => Err(SyncStop::SignIn { message: "user name or password not accepted".into() }),
            // The sync folder doesn't exist yet: make it, and any folders above it.
            404 => self.make_base().map_err(|e| SyncStop::Offline { message: e }),
            _ => Err(SyncStop::Offline { message: format!("the server answered {status}") }),
        }
    }

    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        let base_path = crate::commands::percent_decode(self.base.path());
        let mut out = BTreeMap::new();
        // Folder by folder: many servers turn off "Depth: infinity".
        let mut todo = vec![String::new()];
        while let Some(dir) = todo.pop() {
            let (status, entries) = self.propfind(self.url_for(&format!("{dir}/")), "1")?;
            if !status.is_success() {
                return Err(http_error(status, if dir.is_empty() { "the sync folder" } else { &dir }));
            }
            for e in entries {
                let Some(rel) = relative(&e.path, &base_path) else { continue };
                if rel.is_empty() || rel == dir {
                    continue;
                }
                if rel.split('/').any(skip_name) {
                    continue;
                }
                if e.is_dir {
                    todo.push(rel);
                } else {
                    out.insert(rel, RemoteFile { version: version(&e), modified_ms: e.modified_ms });
                }
            }
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let r = self.send(Method::GET, self.url_for(key), None, None)?;
        if !r.status().is_success() {
            return Err(http_error(r.status(), key));
        }
        Ok(r.bytes().map_err(|e| io::Error::other(short(&e)))?.to_vec())
    }

    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let mut r = self.send(Method::PUT, self.url_for(key), None, Some(bytes.to_vec()))?;
        // 409 (or 404 on some servers): the folder isn't there yet.
        if matches!(r.status().as_u16(), 404 | 409) && key.contains('/') {
            self.make_parents(key)?;
            r = self.send(Method::PUT, self.url_for(key), None, Some(bytes.to_vec()))?;
        }
        if !r.status().is_success() {
            return Err(http_error(r.status(), key));
        }
        // Most servers say the new version right away; otherwise ask.
        if let Some(tag) = r.headers().get("etag").and_then(|v| v.to_str().ok()) {
            return Ok(version(&Entry { path: String::new(), is_dir: false, etag: Some(tag.to_string()), modified_ms: 0, size: 0 }));
        }
        self.version_of(key)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let r = self.send(Method::DELETE, self.url_for(key), None, None)?;
        if r.status().is_success() || r.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(http_error(r.status(), key))
        }
    }
}

/// The path of `href` inside the sync folder, with `/` between parts.
fn relative(path: &str, base_path: &str) -> Option<String> {
    let rest = path.strip_prefix(base_path).or_else(|| path.strip_prefix(base_path.trim_end_matches('/')))?;
    Some(rest.trim_matches('/').to_string())
}

/// Reads a PROPFIND reply (any namespace prefix).
pub fn parse_multistatus(xml: &str) -> Vec<Entry> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut cur: Option<Entry> = None;
    let mut field = String::new();
    let mut ok_props = true;
    // Properties are only trusted from the "200 OK" propstat.
    let mut props: (Option<String>, Option<String>, Option<String>, bool) = (None, None, None, false);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match name.as_str() {
                    "response" => cur = Some(Entry { path: String::new(), is_dir: false, etag: None, modified_ms: 0, size: 0 }),
                    "propstat" => {
                        ok_props = true;
                        props = (None, None, None, false);
                    }
                    "collection" => props.3 = true,
                    _ => {}
                }
                field = name;
            }
            Ok(Event::Empty(e)) => {
                if String::from_utf8_lossy(e.local_name().as_ref()).eq_ignore_ascii_case("collection") {
                    props.3 = true;
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                if let Some(c) = cur.as_mut() {
                    match field.as_str() {
                        "href" => c.path = href_path(text.trim()),
                        "getetag" => props.0 = Some(text.trim().to_string()),
                        "getlastmodified" => props.1 = Some(text.trim().to_string()),
                        "getcontentlength" => props.2 = Some(text.trim().to_string()),
                        "status" => ok_props = text.contains(" 200"),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match name.as_str() {
                    "propstat" if ok_props => {
                        if let Some(c) = cur.as_mut() {
                            c.etag = props.0.take().filter(|s| !s.is_empty());
                            c.modified_ms = props.1.as_deref().and_then(http_date_ms).unwrap_or(0);
                            c.size = props.2.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
                            c.is_dir = props.3;
                        }
                    }
                    "response" => {
                        if let Some(c) = cur.take() {
                            out.push(c);
                        }
                    }
                    _ => {}
                }
                field.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// An href (a full URL or a path) as a decoded path.
fn href_path(href: &str) -> String {
    let path = match Url::parse(href) {
        Ok(u) => u.path().to_string(),
        Err(_) => href.to_string(),
    };
    crate::commands::percent_decode(&path)
}

/// "Sat, 24 Oct 2026 22:54:00 GMT" → ms since 1970.
pub fn http_date_ms(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    let [_, d, mon, y, time, ..] = parts.as_slice() else { return None };
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let m = MONTHS.iter().position(|x| mon.to_lowercase().starts_with(x))? as i64 + 1;
    let (d, y): (i64, i64) = (d.parse().ok()?, y.parse().ok()?);
    let hms: Vec<i64> = time.split(':').filter_map(|x| x.parse().ok()).collect();
    let [hh, mm, ss] = hms.as_slice() else { return None };
    // Days since 1970 for a civil date.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEXTCLOUD: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:oc="http://owncloud.org/ns">
 <d:response><d:href>/remote.php/dav/files/khan/Marrow/</d:href>
  <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype><d:getetag>&quot;6531a&quot;</d:getetag><d:getlastmodified>Sat, 24 Oct 2026 22:54:00 GMT</d:getlastmodified></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  <d:propstat><d:prop><d:getcontentlength/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
 </d:response>
 <d:response><d:href>/remote.php/dav/files/khan/Marrow/Trip%20plan.md</d:href>
  <d:propstat><d:prop><d:resourcetype/><d:getetag>&quot;a1b2&quot;</d:getetag><d:getlastmodified>Sun, 25 Oct 2026 01:02:03 GMT</d:getlastmodified><d:getcontentlength>42</d:getcontentlength></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
 </d:response>
 <d:response><d:href>https://cloud.example.com/remote.php/dav/files/khan/Marrow/Daily/</d:href>
  <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
 </d:response>
</d:multistatus>"#;

    #[test]
    fn reads_a_nextcloud_listing() {
        let e = parse_multistatus(NEXTCLOUD);
        assert_eq!(e.len(), 3);
        assert!(e[0].is_dir);
        assert_eq!(e[1].path, "/remote.php/dav/files/khan/Marrow/Trip plan.md");
        assert!(!e[1].is_dir);
        assert_eq!(e[1].etag.as_deref(), Some("\"a1b2\""));
        assert_eq!(e[1].size, 42);
        assert_eq!(e[1].modified_ms, http_date_ms("Sun, 25 Oct 2026 01:02:03 GMT").unwrap());
        assert_eq!(e[2].path, "/remote.php/dav/files/khan/Marrow/Daily/");
        assert!(e[2].is_dir);
        assert_eq!(version(&e[1]), Version::Tag("a1b2".into()));
    }

    #[test]
    fn reads_capital_prefixes_too() {
        let xml = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/a.md</D:href><D:propstat><D:prop><D:resourcetype></D:resourcetype><D:getlastmodified>Mon, 02 Jan 2006 15:04:05 GMT</D:getlastmodified><D:getcontentlength>3</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
        let e = parse_multistatus(xml);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "/dav/a.md");
        assert_eq!(e[0].etag, None);
        // No ETag: the time and size stand in for one.
        assert_eq!(version(&e[0]), Version::Tag(format!("{}-3", e[0].modified_ms)));
    }

    #[test]
    fn dates_and_paths() {
        assert_eq!(http_date_ms("Thu, 01 Jan 1970 00:00:01 GMT"), Some(1000));
        assert_eq!(http_date_ms("Sat, 24 Oct 2026 22:54:00 GMT"), Some(1_792_882_440_000));
        assert_eq!(http_date_ms("not a date"), None);
        assert_eq!(relative("/dav/Marrow/a/b.md", "/dav/Marrow/").as_deref(), Some("a/b.md"));
        assert_eq!(relative("/dav/Marrow", "/dav/Marrow/").as_deref(), Some(""));
        assert_eq!(relative("/other/x.md", "/dav/Marrow/"), None);
        let w = WebDav::new("https://cloud.example.com/remote.php/dav/files/khan/Marrow", "u", "p").unwrap();
        assert_eq!(w.url_for("Daily/2026/10 October/a#b.md").as_str(), "https://cloud.example.com/remote.php/dav/files/khan/Marrow/Daily/2026/10%20October/a%23b.md");
        assert!(WebDav::new("ftp://x", "u", "p").is_err());
    }

    /// Against a real server: `MARROW_WEBDAV_URL=http://user:pass@host/folder/ cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn round_trip_with_a_real_server() {
        let raw = std::env::var("MARROW_WEBDAV_URL").expect("MARROW_WEBDAV_URL");
        let u = Url::parse(&raw).unwrap();
        let (user, pass) = (u.username().to_string(), u.password().unwrap_or("").to_string());
        let mut clean = u.clone();
        clean.set_username("").unwrap();
        clean.set_password(None).unwrap();
        let w = WebDav::new(clean.as_str(), &user, &pass).unwrap();
        w.check().unwrap();
        let v1 = w.write("deep/er/Note one.md", b"hello").unwrap();
        assert_eq!(w.read("deep/er/Note one.md").unwrap(), b"hello");
        let list = w.list().unwrap();
        assert_eq!(list.get("deep/er/Note one.md").map(|f| &f.version), Some(&v1));
        let v2 = w.write("deep/er/Note one.md", b"hello again").unwrap();
        assert_ne!(v1, v2);
        w.delete("deep/er/Note one.md").unwrap();
        assert!(!w.list().unwrap().contains_key("deep/er/Note one.md"));
    }

    /// Two vaults (say a computer and a phone) kept in step through a real server.
    #[test]
    #[ignore]
    fn two_devices_sync_through_a_real_server() {
        use marrow_core::mirror::Mirror;
        let raw = std::env::var("MARROW_WEBDAV_URL").expect("MARROW_WEBDAV_URL");
        let u = Url::parse(&raw).unwrap();
        let (user, pass) = (u.username().to_string(), u.password().unwrap_or("").to_string());
        let mut clean = u.join("two-devices/").unwrap();
        clean.set_username("").unwrap();
        clean.set_password(None).unwrap();
        let dav = || Box::new(WebDav::new(clean.as_str(), &user, &pass).unwrap());
        let t = tempfile::tempdir().unwrap();
        let (pc, phone) = (t.path().join("pc"), t.path().join("phone"));
        std::fs::create_dir_all(pc.join("Daily/2026/10 October")).unwrap();
        std::fs::create_dir_all(&phone).unwrap();
        std::fs::write(pc.join("Daily/2026/10 October/2026-10-24.md"), "## Log\n- one\n\n## Tasks\n").unwrap();
        std::fs::write(pc.join("Trip plan.md"), "Book the ferry\n").unwrap();
        let (a, b) = (Mirror::with_remote(&pc, dav()), Mirror::with_remote(&phone, dav()));
        a.sync(false).unwrap();
        b.sync(false).unwrap();
        assert_eq!(std::fs::read_to_string(phone.join("Trip plan.md")).unwrap(), "Book the ferry\n");
        // Both edit the same note in different places, then sync.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(pc.join("Daily/2026/10 October/2026-10-24.md"), "## Log\n- one\n- from the computer\n\n## Tasks\n").unwrap();
        std::fs::write(phone.join("Daily/2026/10 October/2026-10-24.md"), "## Log\n- one\n\n## Tasks\n- [ ] from the phone\n").unwrap();
        a.sync(false).unwrap();
        let rep = b.sync(false).unwrap();
        assert_eq!(rep.merged, vec!["Daily/2026/10 October/2026-10-24.md"]);
        a.sync(false).unwrap();
        let want = "## Log\n- one\n- from the computer\n\n## Tasks\n- [ ] from the phone\n";
        assert_eq!(std::fs::read_to_string(pc.join("Daily/2026/10 October/2026-10-24.md")).unwrap(), want);
        assert_eq!(std::fs::read_to_string(phone.join("Daily/2026/10 October/2026-10-24.md")).unwrap(), want);
        // Deleted on the phone: gone from the computer (into Marrow's trash).
        std::fs::remove_file(phone.join("Trip plan.md")).unwrap();
        b.sync(false).unwrap();
        a.sync(false).unwrap();
        assert!(!pc.join("Trip plan.md").exists());
        assert!(pc.join(".marrow/trash/Trip plan.md").exists());
    }

    /// Encrypted sync through a real server: nothing readable reaches it.
    #[test]
    #[ignore]
    fn encrypted_sync_through_a_real_server() {
        use marrow_core::mirror::Mirror;
        use marrow_core::sealed::Sealed;
        let raw = std::env::var("MARROW_WEBDAV_URL").expect("MARROW_WEBDAV_URL");
        let u = Url::parse(&raw).unwrap();
        let (user, pass) = (u.username().to_string(), u.password().unwrap_or("").to_string());
        let mut clean = u.join("encrypted/").unwrap();
        clean.set_username("").unwrap();
        clean.set_password(None).unwrap();
        let sealed = || Box::new(Sealed::new(Box::new(WebDav::new(clean.as_str(), &user, &pass).unwrap()), "a long sync passphrase"));
        let t = tempfile::tempdir().unwrap();
        let (pc, phone) = (t.path().join("pc"), t.path().join("phone"));
        std::fs::create_dir_all(pc.join("Health")).unwrap();
        std::fs::create_dir_all(&phone).unwrap();
        std::fs::write(pc.join("Health/Blood test.md"), "Cholesterol 4.2\n").unwrap();
        Mirror::with_remote(&pc, sealed()).sync(false).unwrap();
        Mirror::with_remote(&phone, sealed()).sync(false).unwrap();
        assert_eq!(std::fs::read_to_string(phone.join("Health/Blood test.md")).unwrap(), "Cholesterol 4.2\n");
        // What the server holds: one hidden settings file and scrambled names.
        let dav = WebDav::new(clean.as_str(), &user, &pass).unwrap();
        let names: Vec<String> = dav.list().unwrap().into_keys().collect();
        assert_eq!(names.len(), 1);
        assert!(!names[0].contains("Health") && !names[0].contains("Blood"));
        let body = String::from_utf8_lossy(&dav.read(&names[0]).unwrap()).into_owned();
        assert!(!body.contains("Cholesterol"));
    }
}
