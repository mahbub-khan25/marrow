//! Word documents (.docx) — also how OneNote pages come across (in OneNote:
//! File → Export → Word Document). Headings, bold/italic/strikethrough,
//! links, bulleted and numbered lists, tables and pictures are kept.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::{tidy, Output};

fn read_file(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key.as_bytes()).map(|a| String::from_utf8_lossy(&a.value).to_string())
}

/// rId → target, from a .rels file.
fn rels(xml: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut r = Reader::from_reader(xml);
    loop {
        match r.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(t)) = (attr(&e, "Id"), attr(&e, "Target")) {
                    out.insert(id, t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Heading level by style id (from styles.xml names like "heading 2" or "Title").
fn heading_styles(xml: &[u8]) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    let mut r = Reader::from_reader(xml);
    let mut cur: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"style" => cur = attr(&e, "w:styleId"),
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"name" => {
                if let (Some(id), Some(name)) = (&cur, attr(&e, "w:val")) {
                    let n = name.to_lowercase();
                    let level = if n == "title" {
                        Some(1)
                    } else if let Some(d) = n.strip_prefix("heading ") {
                        d.trim().parse::<usize>().ok().map(|l| l.clamp(1, 6))
                    } else {
                        None
                    };
                    if let Some(l) = level {
                        out.insert(id.clone(), l);
                    }
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"style" => cur = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// numId → whether level 0 is numbered (vs bullets).
fn numbering(xml: &[u8]) -> HashMap<String, bool> {
    let mut abstract_fmt: HashMap<String, bool> = HashMap::new();
    let mut num_to_abs: HashMap<String, String> = HashMap::new();
    let mut r = Reader::from_reader(xml);
    let mut cur_abs: Option<String> = None;
    let mut cur_lvl: Option<String> = None;
    let mut cur_num: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"abstractNum" => cur_abs = attr(&e, "w:abstractNumId"),
                b"lvl" => cur_lvl = attr(&e, "w:ilvl"),
                b"num" => cur_num = attr(&e, "w:numId"),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"numFmt" if cur_lvl.as_deref() == Some("0") => {
                    if let Some(a) = &cur_abs {
                        let v = attr(&e, "w:val").unwrap_or_default();
                        abstract_fmt.insert(a.clone(), v != "bullet" && v != "none");
                    }
                }
                b"abstractNumId" => {
                    if let (Some(n), Some(v)) = (&cur_num, attr(&e, "w:val")) {
                        num_to_abs.insert(n.clone(), v);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"abstractNum" => cur_abs = None,
                b"lvl" => cur_lvl = None,
                b"num" => cur_num = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    num_to_abs.into_iter().map(|(n, a)| (n, abstract_fmt.get(&a).copied().unwrap_or(false))).collect()
}

#[derive(Clone, Default, PartialEq)]
struct Style {
    b: bool,
    i: bool,
    s: bool,
    link: Option<String>,
}

#[derive(Default)]
struct Para {
    style: String,
    list: Option<(String, usize)>,
    runs: Vec<(String, Style)>,
}

fn esc(t: &str) -> String {
    let mut out = String::new();
    for c in t.chars() {
        if matches!(c, '*' | '_' | '`' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl Para {
    fn text(&self) -> String {
        // Merge runs with the same look, then mark them up.
        let mut merged: Vec<(String, Style)> = Vec::new();
        for (t, s) in &self.runs {
            match merged.last_mut() {
                Some((lt, ls)) if ls == s => lt.push_str(t),
                _ => merged.push((t.clone(), s.clone())),
            }
        }
        let mut out = String::new();
        for (t, s) in merged {
            if t.is_empty() {
                continue;
            }
            if s.link.as_deref().is_some_and(|l| l.starts_with("![[")) {
                out.push_str(s.link.as_deref().unwrap());
                continue;
            }
            // Marks go inside the spaces: "**word** " not "**word **".
            let lead = t.len() - t.trim_start().len();
            let trail = t.len() - t.trim_end().len();
            let core = t.trim();
            if core.is_empty() {
                out.push_str(&t);
                continue;
            }
            let mut m = esc(core);
            if s.s {
                m = format!("~~{m}~~");
            }
            if s.i {
                m = format!("*{m}*");
            }
            if s.b {
                m = format!("**{m}**");
            }
            if let Some(l) = &s.link {
                m = format!("[{m}]({l})");
            }
            out.push_str(&t[..lead]);
            out.push_str(&m);
            out.push_str(&t[t.len() - trail..]);
        }
        out
    }
}

/// Converts a .docx into a note at `folder/<name>.md`, pictures beside it.
pub fn import(bytes: &[u8], name: &str, folder: &str, out: &mut Output) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| format!("“{name}” isn't a Word document Marrow can read."))?;
    let doc = read_file(&mut zip, "word/document.xml").ok_or_else(|| format!("“{name}” has no document in it."))?;
    let rel = read_file(&mut zip, "word/_rels/document.xml.rels").map(|b| rels(&b)).unwrap_or_default();
    let heads = read_file(&mut zip, "word/styles.xml").map(|b| heading_styles(&b)).unwrap_or_default();
    let nums = read_file(&mut zip, "word/numbering.xml").map(|b| numbering(&b)).unwrap_or_default();
    let note_path = out.reserve(folder, name, "md");

    let mut r = Reader::from_reader(&doc[..]);
    let mut blocks: Vec<String> = Vec::new();
    let mut para: Option<Para> = None;
    let mut style = Style::default();
    let mut in_rpr = false;
    let mut in_text = false;
    let mut link: Option<String> = None;
    // Tables: rows of cells of paragraphs.
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut cell: Option<Vec<String>> = None;
    let mut table_depth = 0;
    let mut list_counters: HashMap<(String, usize), usize> = HashMap::new();
    let mut images = 0;

    let flush_para = |p: Para, cell: &mut Option<Vec<String>>, blocks: &mut Vec<String>, counters: &mut HashMap<(String, usize), usize>| {
        let text = p.text();
        if let Some(c) = cell.as_mut() {
            c.push(text.replace('|', "\\|"));
            return;
        }
        if text.trim().is_empty() {
            blocks.push(String::new());
            return;
        }
        if let Some(level) = heads.get(&p.style) {
            blocks.push(format!("{} {}", "#".repeat(*level), text.trim()));
        } else if let Some((num, lvl)) = &p.list {
            let ordered = nums.get(num).copied().unwrap_or(false);
            let marker = if ordered {
                let n = counters.entry((num.clone(), *lvl)).or_insert(0);
                *n += 1;
                format!("{}.", n)
            } else {
                "-".into()
            };
            blocks.push(format!("{}{} {}", "   ".repeat(*lvl), marker, text.trim()));
        } else if p.style.to_lowercase().contains("quote") {
            blocks.push(format!("> {}", text.trim()));
        } else {
            blocks.push(text);
        }
    };

    loop {
        let ev = r.read_event().map_err(|e| e.to_string())?;
        match ev {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == b"p" => para = Some(Para::default()),
            Event::Empty(e) if e.local_name().as_ref() == b"p" => {
                if cell.is_none() {
                    blocks.push(String::new());
                }
            }
            Event::Start(e) => match e.local_name().as_ref() {
                b"tbl" => {
                    table_depth += 1;
                    if table_depth == 1 {
                        table.clear();
                    }
                }
                b"tr" if table_depth == 1 => table.push(Vec::new()),
                b"tc" if table_depth == 1 => cell = Some(Vec::new()),
                b"rPr" => {
                    in_rpr = true;
                }
                b"r" => style = Style { link: link.clone(), ..Style::default() },
                b"t" => in_text = true,
                b"hyperlink" => {
                    link = attr(&e, "r:id").and_then(|id| rel.get(&id).cloned()).or_else(|| attr(&e, "w:anchor").map(|a| format!("#{a}")));
                }
                _ => {}
            },
            Event::Empty(e) => {
                let n = e.local_name();
                let val = attr(&e, "w:val");
                let on = !matches!(val.as_deref(), Some("0") | Some("false") | Some("none"));
                match n.as_ref() {
                    b"pStyle" => {
                        if let Some(p) = para.as_mut() {
                            p.style = val.unwrap_or_default();
                        }
                    }
                    b"numId" => {
                        if let Some(p) = para.as_mut() {
                            let lvl = p.list.as_ref().map(|l| l.1).unwrap_or(0);
                            if val.as_deref() != Some("0") {
                                p.list = Some((val.unwrap_or_default(), lvl));
                            }
                        }
                    }
                    b"ilvl" => {
                        if let Some(p) = para.as_mut() {
                            let lvl = val.and_then(|v| v.parse().ok()).unwrap_or(0);
                            p.list = Some((p.list.take().map(|l| l.0).unwrap_or_default(), lvl));
                        }
                    }
                    b"b" if in_rpr => style.b = on,
                    b"i" if in_rpr => style.i = on,
                    b"strike" if in_rpr => style.s = on,
                    b"tab" => {
                        if let Some(p) = para.as_mut() {
                            p.runs.push(("\t".into(), style.clone()));
                        }
                    }
                    b"br" => {
                        if let Some(p) = para.as_mut() {
                            p.runs.push(("  \n".into(), Style::default()));
                        }
                    }
                    b"blip" => {
                        let target = attr(&e, "r:embed").and_then(|id| rel.get(&id).cloned());
                        if let (Some(t), Some(p)) = (target, para.as_mut()) {
                            let media = format!("word/{}", t.trim_start_matches('/').trim_start_matches("word/"));
                            if let Some(bytes) = read_file(&mut zip, &media) {
                                images += 1;
                                let ext = t.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_else(|| "png".into());
                                let path = out.reserve(&format!("{folder}/attachments"), &format!("{name} {images}"), &ext);
                                out.attachment(path.clone(), bytes);
                                p.runs.push((" ".into(), Style { link: Some(format!("![[{path}]]")), ..Style::default() }));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                if let Some(p) = para.as_mut() {
                    p.runs.push((t.unescape().unwrap_or_default().to_string(), style.clone()));
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"rPr" => in_rpr = false,
                b"hyperlink" => link = None,
                b"p" => {
                    if let Some(p) = para.take() {
                        flush_para(p, &mut cell, &mut blocks, &mut list_counters);
                    }
                }
                b"tc" if table_depth == 1 => {
                    if let (Some(c), Some(row)) = (cell.take(), table.last_mut()) {
                        row.push(c.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "));
                    }
                }
                b"tbl" => {
                    table_depth -= 1;
                    if table_depth == 0 && !table.is_empty() {
                        let width = table.iter().map(|r| r.len()).max().unwrap_or(0);
                        let mut lines = Vec::new();
                        for (i, row) in table.iter().enumerate() {
                            let mut cells = row.clone();
                            cells.resize(width, String::new());
                            lines.push(format!("| {} |", cells.join(" | ")));
                            if i == 0 {
                                lines.push(format!("|{}|", vec![" --- "; width].join("|")));
                            }
                        }
                        blocks.push(String::new());
                        blocks.push(lines.join("\n"));
                        blocks.push(String::new());
                        table.clear();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    // Paragraphs are separated by blank lines; list items stay together.
    let mut md = String::new();
    let mut prev_list = false;
    for b in blocks {
        let is_list = b.trim_start().starts_with("- ") || b.trim_start().split_once(". ").is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty());
        if b.is_empty() {
            prev_list = false;
            md.push('\n');
            continue;
        }
        if !(is_list && prev_list) {
            md.push('\n');
        }
        md.push_str(&b);
        md.push('\n');
        prev_list = is_list;
    }
    out.note(note_path, tidy(&md));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn converts_a_document() {
        let document = r#"<?xml version="1.0"?>
<w:document xmlns:w="w" xmlns:r="r" xmlns:a="a"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Meeting notes</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">Plain and </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t xml:space="preserve"> and </w:t></w:r><w:hyperlink r:id="rId9"><w:r><w:t>a link</w:t></w:r></w:hyperlink></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>Bullet</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Hours</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Ana</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>4</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><a:blip r:embed="rId5"/></w:drawing></w:r></w:p>
</w:body></w:document>"#;
        let rels = r#"<Relationships><Relationship Id="rId9" Target="https://example.com" TargetMode="External"/><Relationship Id="rId5" Target="media/image1.png"/></Relationships>"#;
        let styles = r#"<w:styles xmlns:w="w"><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style></w:styles>"#;
        let numbering = r#"<w:numbering xmlns:w="w"><w:abstractNum w:abstractNumId="7"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="8"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="7"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="8"/></w:num></w:numbering>"#;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, b) in [
                ("word/document.xml", document.as_bytes()),
                ("word/_rels/document.xml.rels", rels.as_bytes()),
                ("word/styles.xml", styles.as_bytes()),
                ("word/numbering.xml", numbering.as_bytes()),
                ("word/media/image1.png", b"PNG".as_slice()),
            ] {
                z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let mut out = Output::default();
        import(&bytes, "Weekly", "OneNote", &mut out).unwrap();
        let note = out.files.iter().find(|f| f.path == "OneNote/Weekly.md").unwrap();
        let md = String::from_utf8(note.bytes.clone()).unwrap();
        assert!(md.contains("# Meeting notes"), "{md}");
        assert!(md.contains("Plain and **bold** and [a link](https://example.com)"), "{md}");
        assert!(md.contains("1. First\n2. Second"), "{md}");
        assert!(md.contains("- Bullet"), "{md}");
        assert!(md.contains("| Name | Hours |\n| --- | --- |\n| Ana | 4 |"), "{md}");
        assert!(md.contains("![[OneNote/attachments/Weekly 1.png]]"), "{md}");
        assert!(out.files.iter().any(|f| f.path == "OneNote/attachments/Weekly 1.png"));
    }
}
