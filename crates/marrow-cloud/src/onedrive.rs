//! OneDrive: the vault is the folder `Apps/Marrow/<vault>`, Marrow's own
//! app folder (it can't see anything else in the OneDrive).

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use marrow_core::mirror::{Remote, RemoteFile, SyncStop, Version};
use reqwest::Method;
use serde_json::json;

use crate::http::{enc, hidden, stop, Api, Body};
use crate::oauth::TokenSource;
use crate::time;

const SELECT: &str = "name,size,file,folder,lastModifiedDateTime,cTag,eTag";

pub struct OneDrive {
    api: Api,
    /// `…/me/drive`
    drive: String,
    account: String,
    vault: String,
}

fn path_of(parts: &[&str]) -> String {
    parts.iter().flat_map(|p| p.split('/')).filter(|p| !p.is_empty()).map(enc).collect::<Vec<_>>().join("/")
}

fn version(v: &serde_json::Value) -> (Version, u64) {
    let modified = time::ms(v.get("lastModifiedDateTime").and_then(|m| m.as_str()).unwrap_or_default());
    let tag = v
        .get("cTag")
        .or_else(|| v.get("eTag"))
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{modified}-{}", v.get("size").and_then(|s| s.as_u64()).unwrap_or(0)));
    (Version::Tag(tag), modified)
}

impl OneDrive {
    pub fn new(token: Arc<dyn TokenSource>, account: &str, vault: &str) -> Self {
        Self::with_base(token, account, vault, "https://graph.microsoft.com/v1.0")
    }

    pub(crate) fn with_base(token: Arc<dyn TokenSource>, account: &str, vault: &str, base: &str) -> Self {
        OneDrive { api: Api::new(token), drive: format!("{}/me/drive", base.trim_end_matches('/')), account: account.into(), vault: vault.into() }
    }

    fn approot(&self) -> String {
        format!("{}/special/approot", self.drive)
    }

    /// The address of `key` in the vault (`""` is the vault folder).
    fn item(&self, key: &str) -> String {
        format!("{}:/{}:", self.approot(), path_of(&[&self.vault, key]))
    }

    /// Makes folder `name` inside the folder at `parent` (an item address);
    /// fine if it's there already.
    fn make_folder(&self, parent: &str, name: &str) -> io::Result<()> {
        let r = self.api.call(
            Method::POST,
            &format!("{parent}/children"),
            &[],
            &Body::Json(json!({ "name": name, "folder": {}, "@microsoft.graph.conflictBehavior": "fail" })),
        )?;
        if r.ok() || r.status == 409 {
            Ok(())
        } else {
            Err(r.error(name))
        }
    }

    fn make_vault(&self) -> io::Result<()> {
        let r = self.api.get(&format!("{}?$select=id", self.item("")))?;
        match r.status {
            200..=299 => Ok(()),
            404 => self.make_folder(&self.approot(), &self.vault),
            _ => Err(r.error("OneDrive")),
        }
    }

    /// Makes the folders `key` sits in, from the top.
    fn make_parents(&self, key: &str) -> io::Result<()> {
        let parts: Vec<&str> = key.split('/').collect();
        for i in 1..parts.len() {
            let parent = if i == 1 { self.item("") } else { self.item(&parts[..i - 1].join("/")) };
            self.make_folder(&parent, parts[i - 1])?;
        }
        Ok(())
    }

    fn children(&self, folder: &str) -> io::Result<Vec<serde_json::Value>> {
        let mut out = vec![];
        let mut url = format!("{folder}/children?$top=1000&$select={SELECT}");
        loop {
            let r = self.api.get(&url)?;
            if !r.ok() {
                return Err(r.error("OneDrive"));
            }
            let v = r.json();
            out.extend(v.get("value").and_then(|x| x.as_array()).cloned().unwrap_or_default());
            match v.get("@odata.nextLink").and_then(|n| n.as_str()) {
                Some(next) => url = next.to_string(),
                None => return Ok(out),
            }
        }
    }

    /// The vault folders in Marrow's app folder.
    pub fn folders(&self) -> io::Result<Vec<String>> {
        Ok(self
            .children(&self.approot())?
            .iter()
            .filter(|c| c.get("folder").is_some())
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect())
    }

    /// The signed-in person's name.
    pub fn account(&self) -> io::Result<String> {
        let r = self.api.get(&format!("{}?$select=owner", self.drive))?;
        if !r.ok() {
            return Err(r.error("OneDrive"));
        }
        let v = r.json();
        Ok(v.pointer("/owner/user/email")
            .or_else(|| v.pointer("/owner/user/displayName"))
            .and_then(|n| n.as_str())
            .unwrap_or("OneDrive")
            .to_string())
    }

    /// Moves now-empty folders above `key` to the recycle bin.
    fn tidy(&self, key: &str) {
        let mut dir = key.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        while !dir.is_empty() {
            let Ok(r) = self.api.get(&format!("{}?$select=folder", self.item(&dir))) else { return };
            if r.json().pointer("/folder/childCount").and_then(|c| c.as_u64()) != Some(0) {
                return;
            }
            if !self.api.call(Method::DELETE, &self.item(&dir), &[], &Body::None).map(|r| r.ok()).unwrap_or(false) {
                return;
            }
            dir = dir.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        }
    }
}

impl Remote for OneDrive {
    fn id(&self) -> String {
        format!("onedrive:{}/{}", self.account, self.vault)
    }

    fn check(&self) -> Result<(), SyncStop> {
        self.make_vault().map_err(stop)
    }

    fn list(&self) -> io::Result<BTreeMap<String, RemoteFile>> {
        let mut out = BTreeMap::new();
        let mut todo = vec![String::new()];
        while let Some(dir) = todo.pop() {
            for c in self.children(&self.item(&dir))? {
                let Some(name) = c.get("name").and_then(|n| n.as_str()) else { continue };
                let key = if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
                if hidden(&key) {
                    continue;
                }
                if c.get("folder").is_some() {
                    todo.push(key);
                } else if c.get("file").is_some() {
                    let (version, modified_ms) = version(&c);
                    out.insert(key, RemoteFile { version, modified_ms });
                }
            }
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> io::Result<Vec<u8>> {
        // Answers with a short-lived download link, which reqwest follows
        // (without the sign-in, which that host doesn't need).
        let r = self.api.get(&format!("{}/content", self.item(key)))?;
        if r.ok() {
            Ok(r.body)
        } else {
            Err(r.error(key))
        }
    }

    fn write(&self, key: &str, bytes: &[u8]) -> io::Result<Version> {
        let put = || self.api.call(Method::PUT, &format!("{}/content", self.item(key)), &[], &Body::Bytes(bytes.to_vec(), "application/octet-stream".into()));
        let mut r = put()?;
        if matches!(r.status, 404 | 409) && key.contains('/') {
            self.make_parents(key)?;
            r = put()?;
        }
        if !r.ok() {
            return Err(r.error(key));
        }
        Ok(version(&r.json()).0)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        let r = self.api.call(Method::DELETE, &self.item(key), &[], &Body::None)?;
        if !r.ok() && r.status != 404 {
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
    fn item_addresses() {
        let d = OneDrive::with_base(Arc::new(crate::oauth::Fixed("t".into())), "me", "My notes", "https://g/v1.0");
        assert_eq!(d.item(""), "https://g/v1.0/me/drive/special/approot:/My%20notes:");
        assert_eq!(d.item("Daily/2026 #1.md"), "https://g/v1.0/me/drive/special/approot:/My%20notes/Daily/2026%20%231.md:");
    }
}
