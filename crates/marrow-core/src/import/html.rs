//! Web pages (.html) — including what OneNote and many apps save as "web
//! page". Pictures embedded in the page or saved beside it come along.

use base64::Engine as _;

use super::{html_to_md, is_image, Output};

/// Converts a page into `folder/<title>.md`. `load` reads a file the page
/// refers to (relative to it), for pictures saved beside the page.
pub fn import(html: &str, file_stem: &str, folder: &str, out: &mut Output, load: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<(), String> {
    let title = between(html, "<title>", "</title>").map(|t| decode_entities(t.trim())).filter(|t| !t.is_empty()).unwrap_or_else(|| file_stem.to_string());
    let body = between(html, "<body", "</body>").map(|b| format!("<div{b}</div>")).unwrap_or_else(|| html.to_string());
    let note_path = out.reserve(folder, &title, "md");

    // Pictures: data: URLs and files beside the page become attachments.
    let mut page = String::with_capacity(body.len());
    let mut rest = body.as_str();
    let mut n = 0;
    while let Some(i) = rest.find("<img") {
        page.push_str(&rest[..i]);
        let after = &rest[i..];
        let end = after.find('>').map(|e| e + 1).unwrap_or(after.len());
        let tag = &after[..end];
        let src = attr(tag, "src").unwrap_or_default();
        let saved = if let Some(data) = src.strip_prefix("data:") {
            data.split_once(";base64,").and_then(|(mime, b64)| {
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
                let ext = mime.rsplit('/').next().unwrap_or("png").replace("jpeg", "jpg").replace("svg+xml", "svg");
                Some((bytes, ext))
            })
        } else if !src.contains("://") && !src.is_empty() {
            let rel = percent_encoding::percent_decode_str(&src).decode_utf8_lossy().to_string();
            load(&rel).map(|b| (b, rel.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_else(|| "png".into())))
        } else {
            None
        };
        match saved {
            Some((bytes, ext)) => {
                n += 1;
                let path = out.reserve(&format!("{folder}/attachments"), &format!("{title} {n}"), &ext);
                out.attachment(path.clone(), bytes);
                page.push_str(&format!("<p>MARROWEMBED{path}MARROWEND</p>"));
            }
            None => page.push_str(tag),
        }
        rest = &after[end..];
    }
    page.push_str(rest);
    let mut md = html_to_md(&page);
    while let (Some(a), Some(b)) = (md.find("MARROWEMBED"), md.find("MARROWEND")) {
        if b < a {
            break;
        }
        let p = md[a + 11..b].replace('\\', "");
        let link = if is_image(&p) { format!("![[{p}]]") } else { format!("[[{p}]]") };
        md = format!("{}{}{}", &md[..a], link, &md[b + 9..]);
    }
    out.note(note_path, md);
    Ok(())
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let lower = s.to_lowercase();
    let i = lower.find(a)? + a.len();
    let j = lower[i..].find(b)? + i;
    Some(&s[i..j])
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    for q in ['"', '\''] {
        let key = format!("{name}={q}");
        if let Some(i) = lower.find(&key) {
            let start = i + key.len();
            let end = tag[start..].find(q)? + start;
            return Some(decode_entities(&tag[start..end]));
        }
    }
    None
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_page_with_pictures() {
        let html = r#"<html><head><title>Recipe &amp; notes</title></head><body><h2>Steps</h2><p>Mix <em>well</em>.</p><img src="data:image/png;base64,UE5H"><img src="pics/cake%201.jpg"><img src="https://x.org/a.png"></body></html>"#;
        let mut out = Output::default();
        import(html, "page", "Web", &mut out, &|p| (p == "pics/cake 1.jpg").then(|| b"JPG".to_vec())).unwrap();
        let note = out.files.iter().find(|f| f.path == "Web/Recipe & notes.md").unwrap();
        let md = String::from_utf8(note.bytes.clone()).unwrap();
        assert!(md.contains("## Steps"), "{md}");
        assert!(md.contains("*well*") || md.contains("_well_"), "{md}");
        assert!(md.contains("![[Web/attachments/Recipe & notes 1.png]]"), "{md}");
        assert!(md.contains("![[Web/attachments/Recipe & notes 2.jpg]]"), "{md}");
        assert!(md.contains("https://x.org/a.png"), "{md}");
        assert_eq!(out.summary.attachments, 2);
    }
}
