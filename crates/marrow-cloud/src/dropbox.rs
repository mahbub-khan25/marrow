//! Dropbox: the vault is the folder `Apps/Marrow/<vault>`, Marrow's own app
//! folder (it can't see anything else in the Dropbox).

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use marrow_core::mirror::{Remote, RemoteFile, SyncStop, Version};
use reqwest::Method;
use serde_json::json;

use crate::http::{hidden, stop, Api, Body, Reply};
use crate::oauth::TokenSource;
use crate::time;

pub struct Dropbox {
    api: Api,
    rpc: String,
    content: String,
    account: String,
    vault: String,
}

/// JSON for the `Dropbox-API-Arg` header, which must be plain ASCII.
fn header_json(v: &serde_json::Value) -> String {
    let mut out = String::new();
    for c in v.to_string().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    out
}

fn not_found(r: &Reply) -> bool {
    r.status == 409 && r.text().contains("not_found")
}

impl Dropbox {
    pub fn new(token: Arc<dyn TokenSource>, account: &str, vault: &str) -> Self {
        Self::with_base(token, account, vault, "https://api.dropboxapi.com", "https://content.dropboxapi.com")
    }

    pub(crate) fn with_base(token: Arc<dyn TokenSource>, account: &str, vault: &str, rpc: &str, content: &str) -> Self {
        Dropbox { api: Api::new(token), rpc: format!("{}/2", rpc.trim_end_matches('/')), content: format!("{}/2", content.trim_end_matches('/')), account: account.into(), vault: vault.into() }
    }

    fn path(&self, key: &str) -> String {
        if key.is_empty() {
            format!("/{}", self.vault)
        } else {
            format!("/{}/{key}", self.vault)
        }
    }

    fn rpc(&self, endpoint: &str, arg: serde_json::Value) -> io::Result<Reply> {
        self.api.call(Method::POST, &format!("{}/{endpoint}", self.rpc), &[], &Body::Json(arg))
    }

    /// Everything in a folder (all the way down when `recursive`).
    fn entries(&self, path: &str, recursive: bool) -> io::Result<Vec<serde_json::Value>> {
        let mut out = vec![];
        let mut r = self.rpc("files/list_folder", json!({ "path": path, "recursive": recursive, "include_deleted": false, "limit": 2000 }))?;
        loop {
            if !r.ok() {
                return Err(r.error("Dropbox"));
            }
            let v = r.json();
            out.extend(v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default());
            if v.get("has_more").and_then(|h| h.as_bool()) != Some(true) {
                return Ok(out);
            }
            let cursor = v.get("cursor").and_then(|c| c.as_str()).unwrap_or_default().to_string();
            r = self.rpc("files/list_folder/continue", json!({ "cursor": cursor }))?;
        }
    }

    fn make_vault(&self) -> io::Result<()> {
        let r = self.rpc("files/get_metadata", json!({ "path": self.path("") }))?;
        if r.ok() {
            return Ok(());
        }
        if !not_found(&r) {
            return Err(r.error("Dropbox"));
        }
        let r = self.rpc("files/create_folder_v2", json!({ "path": self.path(""), "autorename": false }))?;
        // 409 path/conflict: made a moment ago on another device.
        if r.ok() || r.status == 409 {
            Ok(())
        } else {
            Err(r.error("Dropbox"))
        }
    }

    /// The vault folders in Marrow's app folder.
    pub fn folders(&self) -> io::Result<Vec<String>> {
        Ok(self
            .entries("", false)?
            .iter()
            .filter(|e| e.get(".tag").and_then(|t| t.as_str()) == Some("folder"))
            .filter_map(|e| e.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect())
    }

    /// The signed-in person's email address.
    pub fn account(&self) -> io::Result<String> {
        let r = self.api.call(Method::POST, &format!("{}/users/get_current_account", self.rpc), &[], &Body::Bytes(b"null".to_vec(), "application/json".into()))?;
        if !r.ok() {
            return Err(r.error("Dropbox"));
        }
        let v = r.json();
        Ok(v.get("email").or_else(|| v.pointer("/name/display_name")).and_then(|e| e.as_str()).unwrap_or("Dropbox").to_string())
    }

    /// Deletes now-empty folders above `key`.
    fn tidy(&self, key: &str) {
        let mut dir = key.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        while !dir.is_empty() {
            match self.entries(&self.path(&dir), false) {
                Ok(e) if e.is_empty() => {}
                _ => return,
            }
            if !self.rpc("files/delete_v2", json!({ "path": self.path(&dir) })).map(|r| r.ok()).unwrap_or(false) {
                return;
            }
            dir = dir.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        }
    }
}

impl Remote for Dropbox {
    fn id(&self) -> String {
        format!("dropbox:{}/{}", self.account, self.vault)
    }

    fn check(&self) -> Result<(), SyncStop> {
        self.make_vault().map_err(stop)
    }

    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        let root = format!("{}/", self.path("").to_lowercase());
        let mut out = BTreeMap::new();
        for e in self.entries(&self.path(""), true)? {
            if e.get(".tag").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let (Some(lower), Some(display)) = (e.get("path_lower").and_then(|p| p.as_str()), e.get("path_display").and_then(|p| p.as_str())) else { continue };
            if !lower.starts_with(&root) {
                continue;
            }
            // Dropbox ignores case; keep the name as it was written.
            let key = display.chars().skip(root.chars().count()).collect::<String>();
            if key.is_empty() || hidden(&key) {
                continue;
            }
            let modified_ms = time::ms(e.get("server_modified").and_then(|m| m.as_str()).unwrap_or_default());
            let tag = e.get("content_hash").or_else(|| e.get("rev")).and_then(|h| h.as_str()).unwrap_or_default().to_string();
            out.insert(key, RemoteFile { version: Version::Tag(tag), modified_ms });
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        let arg = header_json(&json!({ "path": self.path(key) }));
        let r = self.api.call(Method::POST, &format!("{}/files/download", self.content), &[("Dropbox-API-Arg", arg)], &Body::None)?;
        if r.ok() {
            Ok(r.body)
        } else if not_found(&r) {
            Err(io::Error::new(io::ErrorKind::NotFound, format!("{key}: not found")))
        } else {
            Err(r.error(key))
        }
    }

    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let arg = header_json(&json!({ "path": self.path(key), "mode": "overwrite", "mute": true, "strict_conflict": false }));
        let r = self.api.call(
            Method::POST,
            &format!("{}/files/upload", self.content),
            &[("Dropbox-API-Arg", arg)],
            &Body::Bytes(bytes.to_vec(), "application/octet-stream".into()),
        )?;
        if !r.ok() {
            return Err(r.error(key));
        }
        let v = r.json();
        Ok(Version::Tag(v.get("content_hash").or_else(|| v.get("rev")).and_then(|h| h.as_str()).unwrap_or_default().to_string()))
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let r = self.rpc("files/delete_v2", json!({ "path": self.path(key) }))?;
        if !r.ok() && !not_found(&r) {
            return Err(r.error(key));
        }
        self.tidy(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_ascii() {
        assert_eq!(header_json(&json!({"path": "/v/বা.md"})), r#"{"path":"/v/\u09ac\u09be.md"}"#);
        assert_eq!(header_json(&json!({"path": "/v/😀"})), r#"{"path":"/v/\ud83d\ude00"}"#);
    }
}
