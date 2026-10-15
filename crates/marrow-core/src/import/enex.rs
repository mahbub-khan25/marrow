//! Evernote's .enex export (File → Export Notes). Each note becomes a
//! Markdown note with its tags, dates and source in the frontmatter;
//! pictures and attachments are saved beside the notes and linked where they
//! were; checkboxes become to-dos.

use std::collections::HashMap;

use base64::Engine as _;
use md5::{Digest, Md5};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{clean_name, frontmatter, html_to_md, is_image, Output, PropValue};

#[derive(Default, Debug)]
struct Resource {
    data: Vec<u8>,
    mime: String,
    file_name: String,
}

#[derive(Default, Debug)]
struct Note {
    title: String,
    content: String,
    created: String,
    updated: String,
    tags: Vec<String>,
    source_url: String,
    resources: Vec<Resource>,
}

/// "20231005T101500Z" → "2023-10-05".
fn enex_date(s: &str) -> String {
    if s.len() >= 8 && s[..8].chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &s[..4], &s[4..6], &s[6..8])
    } else {
        s.to_string()
    }
}

fn ext_for(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        "audio/mpeg" => "mp3",
        "audio/wav" => "wav",
        "audio/x-m4a" | "audio/mp4" => "m4a",
        "video/mp4" => "mp4",
        "text/plain" => "txt",
        _ => "bin",
    }
}

fn parse(xml: &str) -> Result<Vec<Note>, String> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(false);
    let mut notes = Vec::new();
    let mut cur: Option<Note> = None;
    let mut res: Option<Resource> = None;
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    loop {
        match r.read_event() {
            Err(e) => return Err(format!("This .enex file couldn't be read: {e}")),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "note" => cur = Some(Note::default()),
                    "resource" => res = Some(Resource::default()),
                    _ => {}
                }
                path.push(name);
                text.clear();
            }
            Ok(Event::Text(t)) => text.push_str(&t.unescape().unwrap_or_default()),
            Ok(Event::CData(t)) => text.push_str(&String::from_utf8_lossy(&t)),
            Ok(Event::End(_)) => {
                let name = path.pop().unwrap_or_default();
                let parent = path.last().map(|s| s.as_str()).unwrap_or("");
                if let Some(rs) = res.as_mut() {
                    match name.as_str() {
                        "data" => rs.data = base64::engine::general_purpose::STANDARD.decode(text.split_whitespace().collect::<String>()).unwrap_or_default(),
                        "mime" => rs.mime = text.trim().to_string(),
                        "file-name" => rs.file_name = text.trim().to_string(),
                        "resource" => {
                            if let (Some(n), Some(rs)) = (cur.as_mut(), res.take()) {
                                n.resources.push(rs);
                            }
                        }
                        _ => {}
                    }
                } else if let Some(n) = cur.as_mut() {
                    match (name.as_str(), parent) {
                        ("title", "note") => n.title = text.trim().to_string(),
                        ("content", "note") => n.content = std::mem::take(&mut text),
                        ("created", "note") => n.created = enex_date(text.trim()),
                        ("updated", "note") => n.updated = enex_date(text.trim()),
                        ("tag", "note") => n.tags.push(text.trim().to_string()),
                        ("source-url", _) => n.source_url = text.trim().to_string(),
                        ("note", _) => {
                            if let Some(n) = cur.take() {
                                notes.push(n);
                            }
                        }
                        _ => {}
                    }
                }
                text.clear();
            }
            _ => {}
        }
    }
    Ok(notes)
}

/// Imports one .enex file into `folder` (usually named after the notebook).
pub fn import(xml: &str, folder: &str, out: &mut Output) -> Result<usize, String> {
    let notes = parse(xml)?;
    if notes.is_empty() {
        return Err("No notes were found in this .enex file.".into());
    }
    let count = notes.len();
    for n in notes {
        let title = if n.title.is_empty() { "Untitled".to_string() } else { n.title.clone() };
        let note_path = out.reserve(folder, &title, "md");
        // Resources by the md5 of their data (how ENML refers to them).
        let mut by_hash: HashMap<String, String> = HashMap::new();
        for (i, r) in n.resources.iter().enumerate() {
            if r.data.is_empty() {
                continue;
            }
            let hash = format!("{:x}", Md5::digest(&r.data));
            let (stem, ext) = if r.file_name.is_empty() {
                (format!("{} {}", clean_name(&title), i + 1), ext_for(&r.mime).to_string())
            } else {
                match r.file_name.rsplit_once('.') {
                    Some((s, e)) => (s.to_string(), e.to_lowercase()),
                    None => (r.file_name.clone(), ext_for(&r.mime).to_string()),
                }
            };
            let p = out.reserve(&format!("{folder}/attachments"), &stem, &ext);
            out.attachment(p.clone(), r.data.clone());
            by_hash.insert(hash, p);
        }
        let mut html = n.content.clone();
        // <en-media hash="…"/> → a placeholder the Markdown keeps.
        html = replace_tags(&html, "en-media", |attrs| {
            let hash = attr(attrs, "hash").unwrap_or_default();
            match by_hash.get(&hash) {
                Some(p) => format!(" MARROWLINK{}MARROWEND ", p),
                None => String::new(),
            }
        });
        html = replace_tags(&html, "en-todo", |attrs| {
            if attr(attrs, "checked").as_deref() == Some("true") {
                "MARROWDONE ".into()
            } else {
                "MARROWTODO ".into()
            }
        });
        let mut md = html_to_md(&html);
        // Placeholders back to Markdown.
        let mut fixed = String::new();
        for line in md.lines() {
            let mut l = line.to_string();
            while let (Some(a), Some(b)) = (l.find("MARROWLINK"), l.find("MARROWEND")) {
                if b < a {
                    break;
                }
                let p = l[a + 10..b].replace('\\', "");
                let link = if is_image(&p) { format!("![[{p}]]") } else { format!("[[{p}]]") };
                l = format!("{}{}{}", &l[..a], link, &l[b + 9..]);
            }
            let t = l.trim_start();
            let indent = &l[..l.len() - t.len()];
            let l = if let Some(rest) = t.strip_prefix("MARROWDONE ") {
                format!("{indent}- [x] {rest}")
            } else if let Some(rest) = t.strip_prefix("MARROWTODO ") {
                format!("{indent}- [ ] {rest}")
            } else {
                l.replace("MARROWDONE ", "[x] ").replace("MARROWTODO ", "[ ] ").replace("- - [", "- [")
            };
            fixed.push_str(l.trim_end());
            fixed.push('\n');
        }
        md = fixed;
        let mut props: Vec<(String, PropValue)> = Vec::new();
        if !n.tags.is_empty() {
            props.push(("tags".into(), PropValue::List(n.tags.iter().map(|t| t.replace(' ', "-")).collect())));
        }
        if !n.created.is_empty() {
            props.push(("created".into(), PropValue::Text(n.created.clone())));
        }
        if !n.updated.is_empty() && n.updated != n.created {
            props.push(("updated".into(), PropValue::Text(n.updated.clone())));
        }
        if !n.source_url.is_empty() {
            props.push(("source".into(), PropValue::Text(n.source_url.clone())));
        }
        out.note(note_path, format!("{}{}", frontmatter(&props), super::tidy(&md)));
    }
    Ok(count)
}

/// Replaces every `<tag …/>` or `<tag …></tag>` with what `f` makes of its attributes.
fn replace_tags(html: &str, tag: &str, f: impl Fn(&str) -> String) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        let after = &rest[i + open.len()..];
        let Some(end) = after.find('>') else {
            rest = after;
            break;
        };
        let attrs = after[..end].trim_end_matches('/');
        out.push_str(&f(attrs));
        rest = &after[end + 1..];
        if let Some(stripped) = rest.strip_prefix(&close) {
            rest = stripped;
        }
    }
    out.push_str(rest);
    out
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = attrs.find(&key)? + key.len();
    let j = attrs[i..].find('"')?;
    Some(attrs[i..i + j].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_notes_tags_images_and_todos() {
        let png = b"\x89PNG fake image";
        let hash = format!("{:x}", Md5::digest(png));
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE en-export SYSTEM "http://xml.evernote.com/pub/evernote-export3.dtd">
<en-export>
<note><title>Trip &amp; plans</title>
<content><![CDATA[<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE en-note SYSTEM "http://xml.evernote.com/pub/enml2.dtd"><en-note><div><b>Pack</b> list</div><div><en-todo checked="true"/>Passport</div><div><en-todo/>Charger</div><div><en-media hash="{hash}" type="image/png"/></div></en-note>]]></content>
<created>20231005T101500Z</created><updated>20231006T080000Z</updated>
<tag>travel</tag><tag>to do</tag>
<note-attributes><source-url>https://example.com</source-url></note-attributes>
<resource><data encoding="base64">{data}</data><mime>image/png</mime><resource-attributes><file-name>map.png</file-name></resource-attributes></resource>
</note>
<note><title></title><content><![CDATA[<en-note>Second</en-note>]]></content></note>
</en-export>"#
        );
        let mut out = Output::default();
        assert_eq!(import(&xml, "Evernote/Travel", &mut out).unwrap(), 2);
        let note = out.files.iter().find(|f| f.path == "Evernote/Travel/Trip & plans.md").expect("note");
        let text = String::from_utf8(note.bytes.clone()).unwrap();
        assert!(text.contains("tags:\n- travel\n- to-do"), "{text}");
        assert!(text.contains("created: 2023-10-05"), "{text}");
        assert!(text.contains("**Pack** list"), "{text}");
        assert!(text.contains("- [x] Passport"), "{text}");
        assert!(text.contains("- [ ] Charger"), "{text}");
        assert!(text.contains("![[Evernote/Travel/attachments/map.png]]"), "{text}");
        assert!(out.files.iter().any(|f| f.path == "Evernote/Travel/attachments/map.png" && f.bytes == png));
        assert!(out.files.iter().any(|f| f.path == "Evernote/Travel/Untitled.md"));
    }
}
