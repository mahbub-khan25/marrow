//! Web pages as notes, from the phones: a link shared to Marrow is fetched
//! and its article saved in the Clippings folder (`marrow_core::clip`).
//! Safari's share sheet can hand over the page itself, as seen (signed in,
//! scrolled), so it isn't fetched again.

use std::io::Read;
use std::time::Duration;

use marrow_core::clip;

use crate::files::unique_child;
use crate::{fail, vp, Marrow, Result};

/// Pages bigger than this aren't saved.
const MOST: u64 = 15 * 1024 * 1024;

/// Downloads a web page's HTML.
fn fetch(url: &str) -> Result<String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(fail("Only web pages (http or https) can be clipped."));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(25))
        // Some sites only send the article to browsers.
        .user_agent("Mozilla/5.0 (Linux; Android 15; Mobile) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Mobile Safari/537.36")
        .build()
        .map_err(|e| fail(e.to_string()))?;
    let r = client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.5")
        .send()
        .map_err(|_| fail("Couldn't reach that page. Check the connection and try again."))?;
    if !r.status().is_success() {
        return Err(fail(format!("The site answered “{}”.", r.status())));
    }
    let kind = r.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("text/html").to_ascii_lowercase();
    if !(kind.contains("html") || kind.contains("text/plain")) {
        return Err(fail("That link isn't a web page (it may be a file or a picture)."));
    }
    let charset_utf8 = !kind.contains("charset=") || kind.contains("utf-8");
    let mut bytes = Vec::new();
    r.take(MOST).read_to_end(&mut bytes).map_err(|e| fail(e.to_string()))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if !charset_utf8 {
        // Old pages in other encodings: Latin-1 is the usual one.
        return Ok(bytes.iter().map(|&b| b as char).collect());
    }
    Ok(text)
}

#[uniffi::export]
impl Marrow {
    /// Saves a web page as a note in `folder` (Clippings when empty) and
    /// returns its path. Without `html` the page is downloaded. `selection`
    /// (HTML) saves just that part. `today` is `YYYY-MM-DD`.
    pub fn clip_page(&self, url: String, html: Option<String>, selection: Option<String>, folder: Option<String>, today: String) -> Result<String> {
        let url = url.trim().to_string();
        let html = match html.filter(|h| !h.trim().is_empty()) {
            Some(h) => h,
            None => fetch(&url)?,
        };
        let made = clip::clip_html(&html, &url, selection.as_deref(), &today);
        let folder = folder.map(|f| f.trim().trim_matches('/').to_string()).filter(|f| !f.is_empty()).unwrap_or_else(|| clip::FOLDER.into());
        let folder = vp(&folder)?;
        let (p, events) = self.with(|e| {
            let p = unique_child(e, &folder, &clip::file_name(&made.title), "md")?;
            let ev = e.create_note(&p, &made.text)?;
            Ok((p, ev))
        })?;
        self.tell(events);
        self.poke_sync();
        Ok(p.to_string())
    }
}

/// The web address in shared text ("Look at this https://…"), if it holds one.
#[uniffi::export]
pub fn link_in(text: String) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, '<' | '>' | '(' | ')' | '"' | '\'' | ',')))
        .find(|w| (w.starts_with("https://") || w.starts_with("http://")) && w.len() > 10)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_in_shared_text() {
        assert_eq!(link_in("Look at this https://garden.example/potatoes!".into()).as_deref(), Some("https://garden.example/potatoes!"));
        assert_eq!(link_in("How to grow potatoes\n<https://garden.example/p>".into()).as_deref(), Some("https://garden.example/p"));
        assert_eq!(link_in("no link here".into()), None);
    }

    #[test]
    fn clips_given_html() {
        let dir = tempfile::tempdir().unwrap();
        let m = crate::tests::vault(dir.path(), &[]);
        let html = "<html><head><title>Seeds</title></head><body><article><p>Plant seeds in spring, in good soil, and keep them watered until they come up. Thin them out when they have two true leaves.</p></article></body></html>";
        let p = m.clip_page("https://garden.example/seeds".into(), Some(html.into()), None, None, "2026-10-30".into()).unwrap();
        assert_eq!(p, "Clippings/Seeds.md");
        let text = m.read_note(p).unwrap();
        assert!(text.starts_with("---\nsource: https://garden.example/seeds\nclipped: 2026-10-30\n---\n\n# Seeds\n"), "{text}");
        let again = m.clip_page("https://garden.example/seeds".into(), Some(html.into()), None, None, "2026-10-30".into()).unwrap();
        assert_eq!(again, "Clippings/Seeds 1.md");
        assert!(m.clip_page("file:///etc/passwd".into(), None, None, None, "2026-10-30".into()).is_err());
    }
}
