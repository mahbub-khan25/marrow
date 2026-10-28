//! Signing in on a computer: Marrow opens the service's sign-in page in the
//! browser and waits on this computer (`http://localhost:53682/`) for the
//! browser to come back with the answer.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::keys::LOOPBACK_PORT;
use crate::oauth::{self, AuthStart, Platform, SignedIn};
use crate::Provider;

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Marrow</title>
<style>body{font:17px/1.5 system-ui,sans-serif;background:#f7f4ee;color:#2b2722;display:grid;place-items:center;height:100vh;margin:0}
main{text-align:center;max-width:28em;padding:2em}h1{font-weight:600;font-size:1.4em}</style></head>
<body><main><h1>{title}</h1><p>{text}</p></main></body></html>"#;

fn reply(mut s: TcpStream, ok: bool, text: &str) {
    let title = if ok { "You're signed in" } else { "That didn't work" };
    let body = PAGE.replace("{title}", title).replace("{text}", &text.replace('<', "&lt;"));
    let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = s.flush();
}

/// The address the browser asked for (`/?code=…`), from the request line.
fn request_target(s: &mut TcpStream) -> Option<String> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = vec![0u8; 8192];
    let mut got = 0;
    while got < buf.len() {
        let n = s.read(&mut buf[got..]).ok()?;
        if n == 0 {
            break;
        }
        got += n;
        if buf[..got].windows(2).any(|w| w == b"\r\n") {
            break;
        }
    }
    let line = String::from_utf8_lossy(&buf[..got]).lines().next()?.to_string();
    let mut parts = line.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_string))?
}

/// Signs in to `provider` in the browser. `open` opens a link there.
/// Gives up after `wait`, or when `cancel` is set.
pub fn sign_in(provider: Provider, open: &dyn Fn(&str) -> Result<(), String>, cancel: &AtomicBool, wait: Duration) -> Result<SignedIn, String> {
    let start = oauth::start(provider, Platform::Desktop).ok_or_else(|| format!("This copy of Marrow isn't set up to sign in to {}.", provider.name()))?;
    // Listen on both addresses "localhost" may mean.
    let listeners: Vec<TcpListener> = ["127.0.0.1", "[::1]"]
        .iter()
        .filter_map(|h| TcpListener::bind(format!("{h}:{LOOPBACK_PORT}")).ok())
        .collect();
    if listeners.is_empty() {
        return Err(format!("Marrow couldn't wait for the browser: port {LOOPBACK_PORT} on this computer is in use. Close other sign-ins and try again."));
    }
    for l in &listeners {
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    open(&start.url)?;
    wait_for(&start, &listeners, cancel, wait)
}

fn wait_for(start: &AuthStart, listeners: &[TcpListener], cancel: &AtomicBool, wait: Duration) -> Result<SignedIn, String> {
    let until = Instant::now() + wait;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Sign-in was cancelled.".into());
        }
        if Instant::now() > until {
            return Err("The sign-in took too long. Try again.".into());
        }
        for l in listeners {
            let Ok((mut s, _)) = l.accept() else { continue };
            let _ = s.set_nonblocking(false);
            let Some(target) = request_target(&mut s) else { continue };
            // Browsers also ask for /favicon.ico and the like.
            if !target.contains("code=") && !target.contains("error=") {
                let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            return match oauth::finish(start, &target) {
                Ok(done) => {
                    reply(s, true, "You can close this tab and go back to Marrow.");
                    Ok(done)
                }
                Err(e) => {
                    reply(s, false, &e);
                    Err(e)
                }
            };
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Client;

    #[test]
    fn a_cancelled_answer_reaches_the_browser_and_marrow() {
        let start = oauth::start_with(Client { provider: Provider::Dropbox, id: "k".into(), secret: None, redirect: "http://localhost:0/".into() });
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        l.set_nonblocking(true).unwrap();
        let state = start.state.clone();
        let browser = std::thread::spawn(move || {
            // A stray request first, then the answer.
            let mut s = TcpStream::connect(addr).unwrap();
            write!(s, "GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            assert!(out.starts_with("HTTP/1.1 404"));
            let mut s = TcpStream::connect(addr).unwrap();
            write!(s, "GET /?error=access_denied&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        });
        let r = wait_for(&start, &[l], &AtomicBool::new(false), Duration::from_secs(10));
        assert_eq!(r.unwrap_err(), "Sign-in was cancelled.");
        assert!(browser.join().unwrap().contains("That didn't work"));
    }
}
