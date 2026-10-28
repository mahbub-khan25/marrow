//! Signing in with OAuth 2 and PKCE, and keeping an access token fresh.
//!
//! Marrow only ever keeps the long-lived refresh token (in the system's
//! credential store); access tokens live in memory for an hour or so.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine as _;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::blocking::Client as Http;
use sha2::{Digest, Sha256};

use crate::keys::{self, Client};
use crate::Provider;

/// The kind of device signing in (each has its own way back from the browser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Ios,
    Android,
}

/// Why there's no access token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// The service turned the sign-in down (revoked, expired): sign in again.
    SignIn(String),
    /// Couldn't reach the service; try again later.
    Offline(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::SignIn(m) | TokenError::Offline(m) => f.write_str(m),
        }
    }
}

/// Gives the access token for calls to a service.
pub trait TokenSource: Send + Sync {
    /// A current token. `renew`: the last one was turned down, get a new one.
    fn token(&self, renew: bool) -> Result<String, TokenError>;
}

struct Endpoints {
    auth: &'static str,
    token: &'static str,
    scope: &'static str,
}

fn endpoints(p: Provider) -> Endpoints {
    match p {
        Provider::GoogleDrive => Endpoints {
            auth: "https://accounts.google.com/o/oauth2/v2/auth",
            token: "https://oauth2.googleapis.com/token",
            scope: "https://www.googleapis.com/auth/drive.file",
        },
        Provider::OneDrive => Endpoints {
            auth: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            scope: "Files.ReadWrite.AppFolder offline_access openid profile",
        },
        Provider::Dropbox => Endpoints {
            auth: "https://www.dropbox.com/oauth2/authorize",
            token: "https://api.dropboxapi.com/oauth2/token",
            scope: "",
        },
    }
}

/// A sign-in in progress: open `url` in a browser; it comes back to `redirect`.
#[derive(Debug, Clone)]
pub struct AuthStart {
    pub provider: Provider,
    pub client: Client,
    pub url: String,
    /// Sent back unchanged with the answer, to match it to this sign-in.
    pub state: String,
    /// Kept on this device to prove the answer is ours (PKCE).
    pub verifier: String,
}

impl AuthStart {
    /// The link scheme the browser comes back to (phones), e.g. `db-abc`.
    pub fn scheme(&self) -> String {
        self.client.redirect.split(':').next().unwrap_or_default().to_string()
    }
}

/// A finished sign-in.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub provider: Provider,
    pub refresh_token: String,
    pub access_token: String,
    pub expires_in: u64,
    /// The email address or name in the sign-in, when the service said.
    pub account: Option<String>,
}

fn random(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("the system's random numbers");
    B64URL.encode(b)
}

fn enc(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

/// Starts signing in to `provider` on `platform`; None if this build can't.
pub fn start(provider: Provider, platform: Platform) -> Option<AuthStart> {
    let client = keys::client(provider, platform)?;
    Some(start_with(client))
}

pub(crate) fn start_with(client: Client) -> AuthStart {
    let e = endpoints(client.provider);
    let verifier = random(48);
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random(18);
    let mut q = vec![
        ("client_id", client.id.clone()),
        ("redirect_uri", client.redirect.clone()),
        ("response_type", "code".into()),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256".into()),
        ("state", state.clone()),
    ];
    if !e.scope.is_empty() {
        q.push(("scope", e.scope.into()));
    }
    match client.provider {
        // Always hand out a refresh token, even on a second sign-in.
        Provider::GoogleDrive => {
            q.push(("access_type", "offline".into()));
            q.push(("prompt", "consent".into()));
        }
        Provider::OneDrive => q.push(("prompt", "select_account".into())),
        Provider::Dropbox => q.push(("token_access_type", "offline".into())),
    }
    let url = format!("{}?{}", e.auth, q.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&"));
    AuthStart { provider: client.provider, client, url, state, verifier }
}

/// The `code` from the address the browser came back to, after checking `state`.
pub fn code_from(callback: &str, state: &str) -> Result<String, String> {
    let query = callback.split_once('?').map(|(_, q)| q).unwrap_or(callback);
    let query = query.split('#').next().unwrap_or_default();
    let mut code = None;
    let mut got_state = None;
    let mut error = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_encoding::percent_decode_str(&v.replace('+', " ")).decode_utf8_lossy().into_owned();
        match k {
            "code" => code = Some(v),
            "state" => got_state = Some(v),
            "error_description" => error = Some(v),
            "error" if error.is_none() => error = Some(v),
            _ => {}
        }
    }
    if let Some(e) = error {
        return Err(if e == "access_denied" { "Sign-in was cancelled.".into() } else { format!("The sign-in didn't work: {e}") });
    }
    if got_state.as_deref() != Some(state) {
        return Err("That sign-in answer doesn't belong to this sign-in. Try again.".into());
    }
    code.filter(|c| !c.is_empty()).ok_or_else(|| "The sign-in came back without a code. Try again.".into())
}

fn http() -> Http {
    Http::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("Marrow/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client")
}

struct TokenReply {
    access: String,
    refresh: Option<String>,
    expires_in: u64,
    id_token: Option<String>,
}

fn parse_reply(status: u16, body: &str) -> Result<TokenReply, TokenError> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if !(200..300).contains(&status) {
        let why = v
            .get("error_description")
            .or_else(|| v.get("error"))
            .and_then(|e| e.as_str())
            .map(|s| s.lines().next().unwrap_or(s).to_string())
            .unwrap_or_else(|| format!("the service answered {status}"));
        // 400/401: the grant is no good any more.
        return Err(if (400..500).contains(&status) && status != 429 { TokenError::SignIn(why) } else { TokenError::Offline(why) });
    }
    let access = v.get("access_token").and_then(|a| a.as_str()).filter(|a| !a.is_empty()).ok_or_else(|| TokenError::Offline("no access token in the answer".into()))?;
    Ok(TokenReply {
        access: access.to_string(),
        refresh: v.get("refresh_token").and_then(|r| r.as_str()).map(str::to_string),
        expires_in: v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(3600),
        id_token: v.get("id_token").and_then(|t| t.as_str()).map(str::to_string),
    })
}

fn post_token(client: &Client, form: &[(&str, &str)]) -> Result<TokenReply, TokenError> {
    let e = endpoints(client.provider);
    let mut form: Vec<(&str, &str)> = form.to_vec();
    form.push(("client_id", &client.id));
    if let Some(s) = &client.secret {
        form.push(("client_secret", s));
    }
    // Microsoft wants the scopes again on each token request.
    if client.provider == Provider::OneDrive {
        form.push(("scope", e.scope));
    }
    let body = form.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&");
    let r = http()
        .post(e.token)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .map_err(|e| TokenError::Offline(crate::http::short(&e)))?;
    let status = r.status().as_u16();
    let text = r.text().unwrap_or_default();
    parse_reply(status, &text)
}

/// The email or user name inside an OpenID ID token (not verified: it's only
/// shown to the person, never trusted for anything).
fn name_in_id_token(t: &str) -> Option<String> {
    let payload = t.split('.').nth(1)?;
    let bytes = B64URL.decode(payload.trim_end_matches('=')).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    ["email", "preferred_username", "name"].iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string))
}

/// Finishes a sign-in with the address the browser came back to.
pub fn finish(start: &AuthStart, callback: &str) -> Result<SignedIn, String> {
    let code = code_from(callback, &start.state)?;
    let r = post_token(
        &start.client,
        &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &start.client.redirect), ("code_verifier", &start.verifier)],
    )
    .map_err(|e| format!("Couldn't finish signing in: {e}"))?;
    let refresh = r.refresh.clone().filter(|t| !t.is_empty()).ok_or("The service didn't allow Marrow to stay signed in. Try again.")?;
    Ok(SignedIn {
        provider: start.provider,
        refresh_token: refresh,
        access_token: r.access,
        expires_in: r.expires_in,
        account: r.id_token.as_deref().and_then(name_in_id_token),
    })
}

/// Turns a refresh token into access tokens as needed. Microsoft hands out a
/// new refresh token each time; `keep` stores it.
pub struct Refreshing {
    client: Client,
    refresh: Mutex<String>,
    cached: Mutex<Option<(String, Instant)>>,
    keep: Box<dyn Fn(&str) + Send + Sync>,
}

impl Refreshing {
    pub fn new(client: Client, refresh_token: String, keep: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Refreshing { client, refresh: Mutex::new(refresh_token), cached: Mutex::new(None), keep: Box::new(keep) }
    }

    /// Starts with the access token a sign-in just gave.
    pub fn with_access(self, access: String, expires_in: u64) -> Self {
        *self.cached.lock().unwrap_or_else(|p| p.into_inner()) = Some((access, Instant::now() + Duration::from_secs(expires_in.saturating_sub(120))));
        self
    }
}

impl TokenSource for Refreshing {
    fn token(&self, renew: bool) -> Result<String, TokenError> {
        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        if !renew {
            if let Some((t, until)) = cached.as_ref() {
                if Instant::now() < *until {
                    return Ok(t.clone());
                }
            }
        }
        let refresh = self.refresh.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if refresh.is_empty() {
            return Err(TokenError::SignIn("not signed in".into()));
        }
        let r = post_token(&self.client, &[("grant_type", "refresh_token"), ("refresh_token", &refresh)])?;
        if let Some(new) = r.refresh.filter(|n| !n.is_empty() && *n != refresh) {
            (self.keep)(&new);
            *self.refresh.lock().unwrap_or_else(|p| p.into_inner()) = new;
        }
        *cached = Some((r.access.clone(), Instant::now() + Duration::from_secs(r.expires_in.saturating_sub(120))));
        Ok(r.access)
    }
}

/// A fixed token (for tests, and for a phone that signs in itself).
pub struct Fixed(pub String);

impl TokenSource for Fixed {
    fn token(&self, _renew: bool) -> Result<String, TokenError> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(p: Provider) -> Client {
        Client { provider: p, id: "abc".into(), secret: None, redirect: "http://localhost:53682/".into() }
    }

    #[test]
    fn auth_link_has_pkce_and_state() {
        let s = start_with(client(Provider::Dropbox));
        assert!(s.url.starts_with("https://www.dropbox.com/oauth2/authorize?client_id=abc&redirect_uri=http%3A%2F%2Flocalhost%3A53682%2F"));
        assert!(s.url.contains("code_challenge_method=S256"));
        assert!(s.url.contains("token_access_type=offline"));
        assert!(s.url.contains(&format!("state={}", enc(&s.state))));
        assert!(s.verifier.len() >= 43);
        let g = start_with(client(Provider::GoogleDrive));
        assert!(g.url.contains("scope=https%3A%2F%2Fwww%2Egoogleapis%2Ecom%2Fauth%2Fdrive%2Efile"));
        assert!(g.url.contains("access_type=offline"));
        let m = start_with(client(Provider::OneDrive));
        assert!(m.url.contains("Files%2EReadWrite%2EAppFolder"));
        assert_eq!(m.scheme(), "http");
    }

    #[test]
    fn reads_the_answer() {
        assert_eq!(code_from("http://localhost:53682/?code=4%2F0Ab&state=xyz", "xyz").unwrap(), "4/0Ab");
        assert_eq!(code_from("db-abc://2/token?state=s&code=c", "s").unwrap(), "c");
        assert!(code_from("http://localhost/?code=c&state=other", "xyz").is_err());
        assert_eq!(code_from("/?error=access_denied&state=s", "s").unwrap_err(), "Sign-in was cancelled.");
    }

    #[test]
    fn token_replies() {
        let ok = parse_reply(200, r#"{"access_token":"a","refresh_token":"r","expires_in":3599}"#).ok().unwrap();
        assert_eq!((ok.access.as_str(), ok.refresh.as_deref(), ok.expires_in), ("a", Some("r"), 3599));
        assert_eq!(
            parse_reply(400, r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#).err(),
            Some(TokenError::SignIn("Token has been expired or revoked.".into()))
        );
        assert!(matches!(parse_reply(503, "down"), Err(TokenError::Offline(_))));
    }

    #[test]
    fn name_from_id_token() {
        let payload = B64URL.encode(br#"{"preferred_username":"khan@outlook.com","name":"Khan"}"#);
        assert_eq!(name_in_id_token(&format!("h.{payload}.s")).as_deref(), Some("khan@outlook.com"));
    }
}
