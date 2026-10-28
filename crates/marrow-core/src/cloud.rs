//! Finding the folders that cloud apps (Dropbox, OneDrive, Google Drive,
//! iCloud) keep on this computer.
//!
//! Marrow never talks to a cloud service itself. A vault "in Dropbox" is just
//! a folder inside the Dropbox folder, and the Dropbox app does the syncing.
//! This module only finds where those folders are.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLocation {
    /// "dropbox", "onedrive", "gdrive" or "icloud".
    pub provider: String,
    /// What to show, e.g. "Dropbox" or "OneDrive (Work)".
    pub label: String,
    pub path: PathBuf,
}

/// Cloud folders on this computer that exist right now.
pub fn detect() -> Vec<CloudLocation> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("USERPROFILE").or_else(|| env("HOME")).map(PathBuf::from);
    let found = if cfg!(windows) {
        windows_candidates(&env, home.as_deref(), &|l| PathBuf::from(format!("{l}:\\")))
    } else if cfg!(target_os = "macos") {
        mac_candidates(home.as_deref())
    } else {
        unix_candidates(home.as_deref())
    };
    dedupe(found)
}

/// Which provider's folder (if any) `path` is inside, using `locations`.
pub fn provider_of<'a>(path: &Path, locations: &'a [CloudLocation]) -> Option<&'a CloudLocation> {
    let p = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    locations.iter().find(|l| {
        let root = dunce::canonicalize(&l.path).unwrap_or_else(|_| l.path.clone());
        p.starts_with(&root)
    })
}

fn loc(provider: &str, label: impl Into<String>, path: PathBuf) -> CloudLocation {
    CloudLocation {
        provider: provider.into(),
        label: label.into(),
        path,
    }
}

/// Keeps folders that exist, once each (the same folder can be found two ways).
fn dedupe(found: Vec<CloudLocation>) -> Vec<CloudLocation> {
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|l| l.path.is_dir())
        .filter(|l| {
            let key = dunce::canonicalize(&l.path)
                .unwrap_or_else(|_| l.path.clone())
                .to_string_lossy()
                .to_lowercase();
            seen.insert(key)
        })
        .collect()
}

/// Dropbox writes where its folders are to `info.json`.
fn dropbox_info(file: &Path) -> Vec<CloudLocation> {
    let Ok(bytes) = std::fs::read(file) else {
        return vec![];
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return vec![];
    };
    let mut out = vec![];
    for (key, label) in [("personal", "Dropbox"), ("business", "Dropbox (Business)")] {
        if let Some(p) = v.get(key).and_then(|a| a.get("path")).and_then(|p| p.as_str()) {
            out.push(loc("dropbox", label, PathBuf::from(p)));
        }
    }
    out
}

/// Windows. Kept free of `cfg` so the tests can run anywhere.
/// `drive` turns a drive letter into its root (Google Drive streams files as
/// a drive, usually G:).
pub(crate) fn windows_candidates(
    env: &dyn Fn(&str) -> Option<String>,
    home: Option<&Path>,
    drive: &dyn Fn(char) -> PathBuf,
) -> Vec<CloudLocation> {
    let mut out = vec![];
    // Dropbox
    for var in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(dir) = env(var) {
            out.extend(dropbox_info(&Path::new(&dir).join("Dropbox").join("info.json")));
        }
    }
    if let Some(h) = home {
        out.push(loc("dropbox", "Dropbox", h.join("Dropbox")));
    }
    // OneDrive sets these for each signed-in account.
    if let Some(p) = env("OneDriveConsumer") {
        out.push(loc("onedrive", "OneDrive", PathBuf::from(p)));
    }
    if let Some(p) = env("OneDriveCommercial") {
        out.push(loc("onedrive", "OneDrive (Work or school)", PathBuf::from(p)));
    }
    if let Some(p) = env("OneDrive") {
        out.push(loc("onedrive", "OneDrive", PathBuf::from(p)));
    }
    if let Some(h) = home {
        out.push(loc("onedrive", "OneDrive", h.join("OneDrive")));
    }
    // Google Drive for desktop: "My Drive" on its own drive letter, or in the
    // user folder when set to mirror files. Older versions used "Google Drive".
    for letter in 'D'..='Z' {
        out.push(loc("gdrive", "Google Drive", drive(letter).join("My Drive")));
    }
    if let Some(h) = home {
        out.push(loc("gdrive", "Google Drive", h.join("My Drive")));
        out.push(loc("gdrive", "Google Drive", h.join("Google Drive")));
        // The Marrow app's own iCloud folder (iPhone and iPad), then the rest.
        out.push(loc("icloud", "iCloud (Marrow on iPhone and iPad)", h.join("iCloudDrive").join("iCloud~app~marrow~notes")));
        out.push(loc("icloud", "iCloud Drive", h.join("iCloudDrive")));
    }
    out
}

fn mac_candidates(home: Option<&Path>) -> Vec<CloudLocation> {
    let Some(h) = home else { return vec![] };
    let mut out = vec![];
    // Newer versions of every provider live under ~/Library/CloudStorage.
    if let Ok(rd) = std::fs::read_dir(h.join("Library/CloudStorage")) {
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        for name in names {
            let p = h.join("Library/CloudStorage").join(&name);
            if name.starts_with("Dropbox") {
                out.push(loc("dropbox", "Dropbox", p));
            } else if let Some(rest) = name.strip_prefix("OneDrive-") {
                let label = if rest == "Personal" { "OneDrive".to_string() } else { format!("OneDrive ({rest})") };
                out.push(loc("onedrive", label, p));
            } else if let Some(account) = name.strip_prefix("GoogleDrive-") {
                out.push(loc("gdrive", format!("Google Drive ({account})"), p.join("My Drive")));
            }
        }
    }
    out.push(loc("dropbox", "Dropbox", h.join("Dropbox")));
    // The Marrow app's own iCloud folder (iPhone and iPad), then the rest.
    out.push(loc("icloud", "iCloud (Marrow on iPhone and iPad)", h.join("Library/Mobile Documents/iCloud~app~marrow~notes/Documents")));
    out.push(loc("icloud", "iCloud Drive", h.join("Library/Mobile Documents/com~apple~CloudDocs")));
    out
}

fn unix_candidates(home: Option<&Path>) -> Vec<CloudLocation> {
    let Some(h) = home else { return vec![] };
    vec![
        loc("dropbox", "Dropbox", h.join("Dropbox")),
        loc("onedrive", "OneDrive", h.join("OneDrive")),
        loc("gdrive", "Google Drive", h.join("Google Drive")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_windows_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let home = t.join("home");
        let appdata = t.join("appdata");
        let dbx = t.join("D Box");
        let od = home.join("OneDrive");
        let g = t.join("drive-g");
        for d in [&home, &appdata.join("Dropbox"), &dbx, &od, &g.join("My Drive")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(
            appdata.join("Dropbox/info.json"),
            serde_json::json!({ "personal": { "path": dbx } }).to_string(),
        )
        .unwrap();
        let env_od = od.to_string_lossy().into_owned();
        let env_app = appdata.to_string_lossy().into_owned();
        let env = move |k: &str| match k {
            "APPDATA" => Some(env_app.clone()),
            "OneDrive" => Some(env_od.clone()),
            _ => None,
        };
        let g2 = g.clone();
        let drive = move |l: char| if l == 'G' { g2.clone() } else { PathBuf::from("/nonexistent").join(l.to_string()) };
        let found = dedupe(windows_candidates(&env, Some(&home), &drive));
        let got: Vec<(&str, &Path)> = found.iter().map(|l| (l.provider.as_str(), l.path.as_path())).collect();
        assert_eq!(
            got,
            vec![
                ("dropbox", dbx.as_path()),
                ("onedrive", od.as_path()),
                ("gdrive", g.join("My Drive").as_path()),
            ]
        );
        std::fs::create_dir_all(od.join("Notes")).unwrap();
        assert_eq!(provider_of(&od.join("Notes"), &found).unwrap().provider, "onedrive");
        assert!(provider_of(&home, &found).is_none());
    }
}
