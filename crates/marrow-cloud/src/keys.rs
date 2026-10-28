//! The IDs Marrow signs in with (from `keys.toml`, or the same names in
//! capitals with `MARROW_` in front, set when building).

use crate::oauth::Platform;
use crate::Provider;

const FILE: &str = include_str!("../keys.toml");

fn from_file(name: &str) -> String {
    for line in FILE.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == name {
                return v.trim().trim_matches('"').trim().to_string();
            }
        }
    }
    String::new()
}

fn from_build(name: &str) -> Option<&'static str> {
    match name {
        "google_desktop_id" => option_env!("MARROW_GOOGLE_DESKTOP_ID"),
        "google_desktop_secret" => option_env!("MARROW_GOOGLE_DESKTOP_SECRET"),
        "google_ios_id" => option_env!("MARROW_GOOGLE_IOS_ID"),
        "onedrive_id" => option_env!("MARROW_ONEDRIVE_ID"),
        "dropbox_key" => option_env!("MARROW_DROPBOX_KEY"),
        _ => None,
    }
}

/// A key by name, empty when it isn't set.
pub fn get(name: &str) -> String {
    match from_build(name).map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => v.to_string(),
        None => from_file(name),
    }
}

/// How an app signs in to one service on one kind of device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub provider: Provider,
    pub id: String,
    /// Only Google's desktop clients have one (and it isn't secret).
    pub secret: Option<String>,
    /// Where the browser comes back to.
    pub redirect: String,
}

/// The port the browser comes back to on a computer.
pub const LOOPBACK_PORT: u16 = 53682;

/// The app's own link scheme on phones (OneDrive comes back through it).
pub const APP_SCHEME: &str = "app.marrow.notes";

/// The sign-in client for `provider` on `platform`; None when this build has
/// no ID for it (or, for Google on Android, when the phone signs in itself).
pub fn client(provider: Provider, platform: Platform) -> Option<Client> {
    let loopback = format!("http://localhost:{LOOPBACK_PORT}/");
    let (id, secret, redirect) = match (provider, platform) {
        (Provider::GoogleDrive, Platform::Desktop) => (get("google_desktop_id"), Some(get("google_desktop_secret")), loopback),
        (Provider::GoogleDrive, Platform::Ios) => {
            let id = get("google_ios_id");
            // Google's iOS clients come back to the reversed client ID.
            let scheme = id.split('.').rev().collect::<Vec<_>>().join(".");
            (id, None, format!("{scheme}:/oauth2redirect"))
        }
        (Provider::GoogleDrive, Platform::Android) => return None,
        (Provider::OneDrive, Platform::Desktop) => (get("onedrive_id"), None, loopback),
        (Provider::OneDrive, _) => (get("onedrive_id"), None, format!("{APP_SCHEME}://oauth")),
        (Provider::Dropbox, Platform::Desktop) => (get("dropbox_key"), None, loopback),
        (Provider::Dropbox, _) => {
            let key = get("dropbox_key");
            let r = format!("db-{key}://2/token");
            (key, None, r)
        }
    };
    if id.is_empty() || secret.as_deref() == Some("") {
        return None;
    }
    Some(Client { provider, id, secret, redirect })
}

/// Whether this build can sign in to `provider` on `platform` (Google on
/// Android is always offered: the phone signs in, through Google Play services).
pub fn available(provider: Provider, platform: Platform) -> bool {
    matches!((provider, platform), (Provider::GoogleDrive, Platform::Android)) || client(provider, platform).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_keys_file() {
        // Every key is named in the file (even if empty).
        for k in ["google_desktop_id", "google_desktop_secret", "google_ios_id", "onedrive_id", "dropbox_key"] {
            assert!(FILE.lines().any(|l| l.trim_start().starts_with(k)), "{k}");
        }
        assert_eq!(from_file("nothing_like_this"), "");
    }

    #[test]
    fn google_ios_redirect_is_the_reversed_id() {
        let id = "123-abc.apps.googleusercontent.com";
        let scheme = id.split('.').rev().collect::<Vec<_>>().join(".");
        assert_eq!(scheme, "com.googleusercontent.apps.123-abc");
    }
}
