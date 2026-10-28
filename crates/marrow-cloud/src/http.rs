//! Calls to a cloud service's web API: the access token, trying again when
//! the service is busy, and errors in words.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use marrow_core::mirror::SyncStop;
use reqwest::blocking::Client;
use reqwest::Method;

use crate::oauth::{TokenError, TokenSource};

pub(crate) enum Body {
    None,
    Json(serde_json::Value),
    Bytes(Vec<u8>, String),
}

pub(crate) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_default()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The reply as an error about `what`.
    pub fn error(&self, what: &str) -> io::Error {
        let text = self.text();
        let lower = text.to_lowercase();
        if self.status == 507 || lower.contains("quotaexceeded") || lower.contains("insufficient_space") || lower.contains("insufficientstorage") {
            return io::Error::other(format!("{what}: your cloud storage is full"));
        }
        match self.status {
            401 => io::Error::new(io::ErrorKind::PermissionDenied, "the cloud service turned down the sign-in"),
            404 => io::Error::new(io::ErrorKind::NotFound, format!("{what}: not found")),
            _ => {
                let v = self.json();
                let why = v
                    .pointer("/error/message")
                    .or_else(|| v.get("error_summary"))
                    .or_else(|| v.get("error_description"))
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("the service answered {}", self.status));
                io::Error::other(format!("{what}: {why}"))
            }
        }
    }
}

pub(crate) fn short(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the service took too long to answer".into()
    } else if e.is_connect() {
        "couldn't connect (are you online?)".into()
    } else {
        let s = e.to_string();
        s.split(": ").next().unwrap_or(&s).to_string()
    }
}

fn token_error(e: TokenError) -> io::Error {
    match e {
        TokenError::SignIn(m) => io::Error::new(io::ErrorKind::PermissionDenied, m),
        TokenError::Offline(m) => io::Error::new(io::ErrorKind::ConnectionAborted, m),
    }
}

/// Why a sync can't run, from an error reaching the service.
pub(crate) fn stop(e: io::Error) -> SyncStop {
    match e.kind() {
        io::ErrorKind::PermissionDenied => SyncStop::SignIn { message: e.to_string() },
        io::ErrorKind::ConnectionAborted | io::ErrorKind::TimedOut => SyncStop::Offline { message: e.to_string() },
        _ => SyncStop::Io { message: e.to_string() },
    }
}

pub(crate) struct Api {
    client: Client,
    token: Arc<dyn TokenSource>,
}

impl Api {
    pub fn new(token: Arc<dyn TokenSource>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("Marrow/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Api { client, token }
    }

    /// Sends a request with the access token, trying again when the service
    /// is busy or the token has just run out.
    pub fn call(&self, method: Method, url: &str, headers: &[(&str, String)], body: &Body) -> io::Result<Reply> {
        let mut renew = false;
        let mut tries = 0u32;
        loop {
            tries += 1;
            let token = self.token.token(renew).map_err(token_error)?;
            let mut req = self.client.request(method.clone(), url).bearer_auth(&token);
            for (k, v) in headers {
                req = req.header(*k, v);
            }
            req = match body {
                Body::None => req,
                Body::Json(v) => req.header("Content-Type", "application/json").body(v.to_string()),
                Body::Bytes(b, ty) => req.header("Content-Type", ty.as_str()).body(b.clone()),
            };
            let r = match req.send() {
                Ok(r) => r,
                Err(e) if tries < 3 && (e.is_timeout() || e.is_connect() || e.is_request()) => {
                    std::thread::sleep(Duration::from_secs(tries as u64));
                    continue;
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::ConnectionAborted, short(&e))),
            };
            let status = r.status().as_u16();
            let retry_after = r.headers().get("Retry-After").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
            let bytes = r.bytes().map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, short(&e)))?.to_vec();
            let reply = Reply { status, body: bytes };
            if status == 401 && !renew {
                renew = true;
                continue;
            }
            let busy = matches!(status, 429 | 500 | 502 | 503 | 504) || (status == 403 && reply.text().contains("ateLimitExceeded"));
            if busy && tries < 5 {
                let wait = retry_after.unwrap_or(1 << (tries - 1)).min(20);
                std::thread::sleep(Duration::from_secs(wait));
                continue;
            }
            if busy {
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, format!("the service is busy (it answered {status})")));
            }
            return Ok(reply);
        }
    }

    pub fn get(&self, url: &str) -> io::Result<Reply> {
        self.call(Method::GET, url, &[], &Body::None)
    }
}

/// `%`-encodes one part of a path or query.
pub(crate) fn enc(s: &str) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');
    percent_encoding::utf8_percent_encode(s, SET).to_string()
}

/// Whether any part of `key` is hidden or a temporary file (never synced).
pub(crate) fn hidden(key: &str) -> bool {
    key.split('/').any(marrow_core::mirror::skip_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_names() {
        assert_eq!(enc("a b/c#d.md"), "a%20b%2Fc%23d.md");
        assert_eq!(enc("বাং"), "%E0%A6%AC%E0%A6%BE%E0%A6%82");
    }

    #[test]
    fn hidden_paths() {
        assert!(hidden(".marrow-encryption.json"));
        assert!(hidden("a/.obsidian/x.json"));
        assert!(!hidden("Projects/Garden plan.md"));
    }

    #[test]
    fn errors_in_words() {
        let r = Reply { status: 403, body: br#"{"error":{"message":"The user's Drive storage quota has been exceeded.","errors":[{"reason":"storageQuotaExceeded"}]}}"#.to_vec() };
        assert_eq!(r.error("a.md").to_string(), "a.md: your cloud storage is full");
        let r = Reply { status: 409, body: br#"{"error_summary":"path/conflict/folder/.."}"#.to_vec() };
        assert_eq!(r.error("a.md").to_string(), "a.md: path/conflict/folder/..");
        assert_eq!(Reply { status: 404, body: vec![] }.error("x").kind(), io::ErrorKind::NotFound);
    }
}
