//! Sync with Google Drive, OneDrive and Dropbox: each is a [`Remote`] for
//! marrow-core's sync engine, which decides what to copy. Notes stay plain
//! Markdown files in a visible folder in the person's own storage:
//!
//! * Google Drive: `Marrow/<vault>` (Marrow only sees the files it made);
//! * OneDrive: `Apps/Marrow/<vault>`;
//! * Dropbox: `Apps/Marrow/<vault>`.
//!
//! Signing in uses OAuth with PKCE ([`oauth`]); on a computer the browser
//! comes back to Marrow on this computer ([`loopback`]), on a phone through
//! the app's own link.

pub mod dropbox;
pub mod gdrive;
mod http;
pub mod keys;
pub mod loopback;
pub mod oauth;
pub mod onedrive;
mod time;

use std::sync::Arc;

use marrow_core::mirror::Remote;

pub use oauth::{AuthStart, Platform, SignedIn, TokenError, TokenSource};

/// A cloud service Marrow syncs with directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    GoogleDrive,
    OneDrive,
    Dropbox,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::GoogleDrive, Provider::OneDrive, Provider::Dropbox];

    /// Short name kept in settings.
    pub fn id(self) -> &'static str {
        match self {
            Provider::GoogleDrive => "gdrive",
            Provider::OneDrive => "onedrive",
            Provider::Dropbox => "dropbox",
        }
    }

    pub fn from_id(id: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::GoogleDrive => "Google Drive",
            Provider::OneDrive => "OneDrive",
            Provider::Dropbox => "Dropbox",
        }
    }

    /// Where the vaults are, as the person sees it in that service.
    pub fn place(self) -> &'static str {
        match self {
            Provider::GoogleDrive => "My Drive › Marrow",
            Provider::OneDrive | Provider::Dropbox => "Apps › Marrow",
        }
    }
}

/// The sync folder for `vault` in a signed-in service.
pub fn connect(provider: Provider, token: Arc<dyn TokenSource>, account: &str, vault: &str) -> Box<dyn Remote> {
    match provider {
        Provider::GoogleDrive => Box::new(gdrive::GoogleDrive::new(token, account, vault)),
        Provider::OneDrive => Box::new(onedrive::OneDrive::new(token, account, vault)),
        Provider::Dropbox => Box::new(dropbox::Dropbox::new(token, account, vault)),
    }
}

/// The vault folders already in Marrow's place in the service, by name.
pub fn vault_folders(provider: Provider, token: Arc<dyn TokenSource>) -> Result<Vec<String>, String> {
    let mut v = match provider {
        Provider::GoogleDrive => gdrive::GoogleDrive::new(token, "", "").folders(),
        Provider::OneDrive => onedrive::OneDrive::new(token, "", "").folders(),
        Provider::Dropbox => dropbox::Dropbox::new(token, "", "").folders(),
    }
    .map_err(|e| e.to_string())?;
    v.retain(|n| !marrow_core::mirror::skip_name(n));
    v.sort_by_key(|n| n.to_lowercase());
    v.dedup();
    Ok(v)
}

/// Who is signed in (an email address or name), to show in Settings.
pub fn account_name(provider: Provider, token: Arc<dyn TokenSource>) -> Result<String, String> {
    match provider {
        Provider::GoogleDrive => gdrive::GoogleDrive::new(token, "", "").account(),
        Provider::OneDrive => onedrive::OneDrive::new(token, "", "").account(),
        Provider::Dropbox => dropbox::Dropbox::new(token, "", "").account(),
    }
    .map_err(|e| e.to_string())
}

/// A folder name for a vault: no slashes or characters cloud services refuse.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        "Notes".into()
    } else {
        cleaned.chars().take(100).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_safe() {
        assert_eq!(folder_name("  My: notes/2026 "), "My notes 2026");
        assert_eq!(folder_name(".hidden."), "hidden");
        assert_eq!(folder_name("///"), "Notes");
        assert_eq!(folder_name("বাংলা নোট"), "বাংলা নোট");
    }

    #[test]
    fn providers_round_trip() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), Some(p));
        }
    }
}

#[cfg(test)]
mod fake;
#[cfg(test)]
mod sync_tests;
