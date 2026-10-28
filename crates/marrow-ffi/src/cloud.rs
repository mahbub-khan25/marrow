//! Sync with Google Drive, OneDrive and Dropbox: signing in through the
//! phone's browser, picking the vault folder, and turning sync on.

use std::sync::{Arc, Mutex};

use marrow_cloud::oauth::{self, Refreshing};
use marrow_cloud::{keys, Provider, TokenError, TokenSource};
use marrow_core::mirror::Remote;
use marrow_core::sealed;

use crate::sync::{check_sealed, SyncPlace, Target};
use crate::{fail, Marrow, Result, SecretStore};

/// The phone's own sign-in, for services it handles itself (Google Drive on
/// Android, through Google Play services).
#[uniffi::export(with_foreign)]
pub trait PlatformTokens: Send + Sync {
    /// A current access token for Google Drive, or None when the person
    /// isn't signed in (or said no). `renew`: the last one was turned down.
    /// Called from a background thread; it may wait for the network.
    fn google_token(&self, renew: bool) -> Option<String>;
}

pub(crate) type Platform = Arc<Mutex<Option<Arc<dyn PlatformTokens>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CloudKind {
    GoogleDrive,
    OneDrive,
    Dropbox,
}

impl From<CloudKind> for Provider {
    fn from(k: CloudKind) -> Self {
        match k {
            CloudKind::GoogleDrive => Provider::GoogleDrive,
            CloudKind::OneDrive => Provider::OneDrive,
            CloudKind::Dropbox => Provider::Dropbox,
        }
    }
}

impl From<Provider> for CloudKind {
    fn from(p: Provider) -> Self {
        match p {
            Provider::GoogleDrive => CloudKind::GoogleDrive,
            Provider::OneDrive => CloudKind::OneDrive,
            Provider::Dropbox => CloudKind::Dropbox,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CloudService {
    pub kind: CloudKind,
    pub name: String,
    /// Where the notes go, as the person sees it there (e.g. "Apps › Marrow").
    pub place: String,
    /// This copy of the app can sign in to it.
    pub available: bool,
    /// The phone signs in itself (no browser): call `cloud_platform_account`.
    pub signs_in_on_device: bool,
}

/// Open `url` in the system's sign-in browser (ASWebAuthenticationSession,
/// Custom Tabs); it comes back to a link starting with `callback_scheme`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CloudSignInStart {
    pub url: String,
    pub callback_scheme: String,
}

/// A signed-in account and the vault folders already there.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CloudAccount {
    pub kind: CloudKind,
    pub account: String,
    pub folders: Vec<String>,
    /// A folder name for this vault, if it starts a new one.
    pub suggested: String,
}

fn platform() -> oauth::Platform {
    if cfg!(target_os = "ios") {
        oauth::Platform::Ios
    } else {
        oauth::Platform::Android
    }
}

pub(crate) fn secret_id(provider: &str, account: &str) -> String {
    format!("sync:cloud:{provider}:{account}")
}

/// Asks the phone for Google tokens.
struct OnDevice(Arc<dyn PlatformTokens>);

impl TokenSource for OnDevice {
    fn token(&self, renew: bool) -> std::result::Result<String, TokenError> {
        self.0.google_token(renew).ok_or_else(|| TokenError::SignIn("Google Drive isn't signed in on this phone".into()))
    }
}

/// Access tokens for a signed-in account.
pub(crate) fn tokens(p: Provider, account: &str, secrets: &Arc<dyn SecretStore>, platform_tokens: &Platform) -> std::result::Result<Arc<dyn TokenSource>, String> {
    if p == Provider::GoogleDrive && platform() == oauth::Platform::Android {
        let t = platform_tokens.lock().ok().and_then(|t| t.clone()).ok_or("Google sign-in isn't ready yet.")?;
        return Ok(Arc::new(OnDevice(t)));
    }
    let client = keys::client(p, platform()).ok_or_else(|| format!("This copy of Marrow isn't set up to sign in to {}.", p.name()))?;
    let id = secret_id(p.id(), account);
    let refresh = secrets.get(id.clone()).ok_or_else(|| format!("Sign in to {} again in Settings › Sync.", p.name()))?;
    let store = secrets.clone();
    Ok(Arc::new(Refreshing::new(client, refresh, move |new| store.set(id.clone(), Some(new.to_string())))))
}

impl Marrow {
    fn suggested_folder(&self) -> String {
        let name = self.root().ok().and_then(|r| r.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
        marrow_cloud::folder_name(&name)
    }

    fn account_with(&self, p: Provider, account: String, token: Arc<dyn TokenSource>) -> Result<CloudAccount> {
        let folders = marrow_cloud::vault_folders(p, token).map_err(fail)?;
        Ok(CloudAccount { kind: p.into(), account, folders, suggested: self.suggested_folder() })
    }

    fn cloud_remote(&self, kind: CloudKind, account: &str, folder: &str) -> Result<Box<dyn Remote>> {
        let p: Provider = kind.into();
        let t = tokens(p, account, &self.secrets, &self.platform_tokens).map_err(fail)?;
        Ok(marrow_cloud::connect(p, t, account, &marrow_cloud::folder_name(folder)))
    }
}

#[uniffi::export]
impl Marrow {
    /// Tells Marrow how this phone signs in by itself (Google on Android).
    pub fn set_platform_tokens(&self, tokens: Arc<dyn PlatformTokens>) {
        if let Ok(mut t) = self.platform_tokens.lock() {
            *t = Some(tokens);
        }
    }

    /// The cloud services Marrow can sync with, and whether this copy can sign in to each.
    pub fn cloud_services(&self) -> Vec<CloudService> {
        Provider::ALL
            .into_iter()
            .map(|p| CloudService {
                kind: p.into(),
                name: p.name().into(),
                place: p.place().into(),
                available: keys::available(p, platform()),
                signs_in_on_device: p == Provider::GoogleDrive && platform() == oauth::Platform::Android,
            })
            .collect()
    }

    /// Starts signing in: open the link in the sign-in browser, then pass
    /// the address it comes back to to `cloud_sign_in_finish`.
    pub fn cloud_sign_in_start(&self, kind: CloudKind) -> Result<CloudSignInStart> {
        let p: Provider = kind.into();
        let start = oauth::start(p, platform()).ok_or_else(|| fail(format!("This copy of Marrow isn't set up to sign in to {}.", p.name())))?;
        let out = CloudSignInStart { url: start.url.clone(), callback_scheme: start.scheme() };
        if let Ok(mut pending) = self.cloud_pending.lock() {
            pending.retain(|_, s| s.provider != p);
            pending.insert(start.state.clone(), start);
        }
        Ok(out)
    }

    /// Finishes signing in with the address the browser came back to, keeps
    /// the sign-in, and lists the vault folders already there. Slow: call it
    /// off the main thread.
    pub fn cloud_sign_in_finish(&self, callback_url: String) -> Result<CloudAccount> {
        let state = callback_url
            .split(['?', '&', '#'])
            .find_map(|p| p.strip_prefix("state="))
            .map(percent_decode)
            .ok_or_else(|| fail("That sign-in answer is incomplete. Try again."))?;
        let start = self.cloud_pending.lock().ok().and_then(|mut p| p.remove(&state)).ok_or_else(|| fail("That sign-in has expired. Try again."))?;
        let done = oauth::finish(&start, &callback_url).map_err(fail)?;
        let p = start.provider;
        let token: Arc<dyn TokenSource> = Arc::new(Refreshing::new(start.client.clone(), done.refresh_token.clone(), |_| {}).with_access(done.access_token.clone(), done.expires_in));
        let account = match done.account.clone() {
            Some(a) => a,
            None => marrow_cloud::account_name(p, token.clone()).map_err(fail)?,
        };
        self.secrets.set(secret_id(p.id(), &account), Some(done.refresh_token));
        self.account_with(p, account, token)
    }

    /// For a service the phone signs in to itself (Google on Android): the
    /// account and its vault folders. Slow: call it off the main thread.
    pub fn cloud_platform_account(&self, kind: CloudKind) -> Result<CloudAccount> {
        let p: Provider = kind.into();
        let token = tokens(p, "", &self.secrets, &self.platform_tokens).map_err(fail)?;
        let account = marrow_cloud::account_name(p, token.clone()).map_err(fail)?;
        self.account_with(p, account, token)
    }

    /// Makes the vault folder if needed and says what's in it. Slow.
    pub fn check_cloud(&self, kind: CloudKind, account: String, folder: String) -> Result<SyncPlace> {
        let r = self.cloud_remote(kind, &account, &folder)?;
        r.check().map_err(|e| fail(e.to_string()))?;
        Ok(SyncPlace { files: r.list()?.len() as u32, encrypted: sealed::is_encrypted(r.as_ref()) })
    }

    /// Syncs the open vault with `folder` in a signed-in service.
    /// `passphrase` turns on end-to-end encryption. Slow.
    pub fn sync_with_cloud(&self, kind: CloudKind, account: String, folder: String, passphrase: Option<String>) -> Result<()> {
        let passphrase = passphrase.filter(|p| !p.is_empty());
        let folder = marrow_cloud::folder_name(&folder);
        let r = self.cloud_remote(kind, &account, &folder)?;
        r.check().map_err(|e| fail(e.to_string()))?;
        match &passphrase {
            Some(pp) => check_sealed(r, pp)?,
            None if sealed::is_encrypted(r.as_ref()) => return Err(fail("That folder holds an encrypted vault. Turn on encryption and enter its sync passphrase.")),
            None => {}
        }
        let p: Provider = kind.into();
        self.set_target(Some(Target::Cloud { provider: p.id().into(), account, folder }), passphrase)
    }
}

fn percent_decode(s: &str) -> String {
    let s = s.split(['&', '#']).next().unwrap_or_default();
    marrow_webdav::percent_decode(&s.replace('+', " "))
}

#[cfg(test)]
mod tests {
    #[test]
    fn services_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[("A.md", "x")]);
        let s = m.cloud_services();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].name, "Google Drive");
        // Tests build for Android: Google signs in on the phone.
        assert!(s[0].signs_in_on_device && s[0].available);
        assert!(m.cloud_sign_in_finish("app.marrow.notes://oauth?code=c&state=nothing".into()).is_err());
        assert_eq!(m.suggested_folder(), "vault");
    }
}
