//! Receives web pages from the Marrow Clipper browser extension.
//!
//! While Marrow runs it listens on this computer only (127.0.0.1, port
//! 53683) and answers only browser extensions: every request must carry an
//! extension's Origin (`chrome-extension://…`, `moz-extension://…`,
//! `safari-web-extension://…`), which web pages can't fake, and name this
//! computer as the Host (so a web site can't reach it by DNS tricks).
//!
//! - `POST /hello` → `{"app":"Marrow","version":"…","vault":"Notes"}` (vault is
//!   null when none is open)
//! - `POST /clip` `{"url","html","selection"?,"folder"?}` → `{"path","title"}`
//! - `POST /open` `{"path"}` → shows the note in the window

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use marrow_core::clip;
use marrow_core::path::VaultPath;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::{emit_events, err, with_engine, AppState, CmdResult};

pub const PORT: u16 = 53683;
/// Pages bigger than this aren't taken.
const MOST: usize = 25 * 1024 * 1024;

pub fn start(app: &AppHandle) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", PORT)) else {
        log::warn!("clipper: port {PORT} is busy, the browser extension can't reach Marrow");
        return;
    };
    let app = app.clone();
    let _ = std::thread::Builder::new().name("marrow-clipper".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            let _ = std::thread::Builder::new().name("marrow-clip".into()).spawn(move || {
                if let Err(e) = serve(&app, stream) {
                    log::info!("clipper: {e}");
                }
            });
        }
    });
}

struct Request {
    method: String,
    path: String,
    origin: Option<String>,
    host: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    stream.set_read_timeout(Some(Duration::from_secs(30))).map_err(err)?;
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line).map_err(err)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let (mut origin, mut host, mut len) = (None, None, 0usize);
    let mut header_bytes = 0;
    loop {
        let mut h = String::new();
        let n = r.read_line(&mut h).map_err(err)?;
        header_bytes += n;
        if n == 0 || h == "\r\n" || h == "\n" || header_bytes > 32 * 1024 {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim().to_string();
            match k.trim().to_ascii_lowercase().as_str() {
                "origin" => origin = Some(v),
                "host" => host = Some(v),
                "content-length" => len = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if len > MOST {
        return Err("too big".into());
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).map_err(err)?;
    Ok(Request { method, path, origin, host, body })
}

fn is_extension(origin: &str) -> bool {
    ["chrome-extension://", "moz-extension://", "safari-web-extension://", "extension://"]
        .iter()
        .any(|p| origin.starts_with(p))
}

fn is_this_computer(host: &str) -> bool {
    [format!("127.0.0.1:{PORT}"), format!("localhost:{PORT}")].iter().any(|h| h == host)
}

fn respond(stream: &mut TcpStream, status: &str, origin: Option<&str>, body: &Value) -> std::io::Result<()> {
    let text = body.to_string();
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n", text.len());
    if let Some(o) = origin {
        head.push_str(&format!("Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Private-Network: true\r\nVary: Origin\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(text.as_bytes())
}

fn serve(app: &AppHandle, mut stream: TcpStream) -> Result<(), String> {
    let req = read_request(&mut stream)?;
    let origin = req.origin.as_deref().filter(|o| is_extension(o));
    if origin.is_none() || !req.host.as_deref().is_some_and(is_this_computer) {
        let _ = respond(&mut stream, "403 Forbidden", None, &json!({ "error": "Only the Marrow Clipper can use this." }));
        return Ok(());
    }
    if req.method == "OPTIONS" {
        let _ = respond(&mut stream, "204 No Content", origin, &json!({}));
        return Ok(());
    }
    if req.method != "POST" {
        let _ = respond(&mut stream, "405 Method Not Allowed", origin, &json!({ "error": "POST only" }));
        return Ok(());
    }
    let result = match req.path.as_str() {
        "/hello" => Ok(hello(app)),
        "/clip" => serde_json::from_slice(&req.body).map_err(err).and_then(|c| save(app, c)),
        "/open" => serde_json::from_slice::<OpenReq>(&req.body).map_err(err).map(|o| open(app, &o.path)),
        _ => Err("Not found".into()),
    };
    let _ = match result {
        Ok(v) => respond(&mut stream, "200 OK", origin, &v),
        Err(e) => respond(&mut stream, "400 Bad Request", origin, &json!({ "error": e })),
    };
    Ok(())
}

fn hello(app: &AppHandle) -> Value {
    let state: State<AppState> = app.state();
    let vault = with_engine(&state, |e| Ok(e.vault().root().file_name().map(|n| n.to_string_lossy().to_string()))).ok().flatten();
    json!({ "app": "Marrow", "version": env!("CARGO_PKG_VERSION"), "vault": vault })
}

#[derive(Deserialize)]
struct ClipReq {
    url: String,
    html: String,
    #[serde(default)]
    selection: Option<String>,
    #[serde(default)]
    folder: Option<String>,
}

#[derive(Deserialize)]
struct OpenReq {
    path: String,
}

fn save(app: &AppHandle, c: ClipReq) -> CmdResult<Value> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let made = clip::clip_html(&c.html, &c.url, c.selection.as_deref(), &today);
    let folder = c.folder.as_deref().map(str::trim).filter(|f| !f.is_empty()).unwrap_or(clip::FOLDER).trim_matches('/');
    let folder = VaultPath::new(folder).map_err(err)?;
    let state: State<AppState> = app.state();
    let (path, events) = with_engine(&state, |e| {
        let p = crate::commands::unique_child(e, &folder, &clip::file_name(&made.title), "md")?;
        let ev = e.create_note(&p, &made.text).map_err(err)?;
        Ok((p, ev))
    })
    .map_err(|e| if e.contains("No vault") { "Open a vault in Marrow first.".to_string() } else { e })?;
    emit_events(app, events, None);
    let _ = app.emit("clipped", json!({ "path": path.as_str(), "title": made.title }));
    Ok(json!({ "path": path.as_str(), "title": made.title }))
}

fn open(app: &AppHandle, path: &str) -> Value {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("open-note", path);
    json!({ "ok": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_may_ask() {
        assert!(is_extension("chrome-extension://abcdef"));
        assert!(is_extension("moz-extension://1234-5678"));
        assert!(!is_extension("https://evil.example"));
        assert!(!is_extension("null"));
        assert!(is_this_computer("127.0.0.1:53683"));
        assert!(!is_this_computer("evil.example:53683"));
    }
}
