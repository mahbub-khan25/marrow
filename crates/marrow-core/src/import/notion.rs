//! Notion's "Markdown & CSV" export (a .zip, sometimes with more zips
//! inside). Page and folder names lose Notion's 32-character ids, links
//! between pages become [[wikilinks]], properties become frontmatter, and
//! each database (a .csv) becomes a note with a ```database block over its
//! pages.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use percent_encoding::percent_decode_str;

use super::{clean_name, frontmatter, guess_value, is_image, tidy, Output, PropValue};

/// Removes Notion's " 0123…(32 hex)" from a file or folder name.
pub fn strip_id(name: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !e.contains(' ') && e.len() <= 5 => (s, Some(e)),
        _ => (name, None),
    };
    let stem = match stem.rsplit_once(' ') {
        Some((a, id)) if id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()) => a,
        _ => match stem.len() > 32 {
            // "Page 0123…" without the space (older exports)
            true if stem[stem.len() - 32..].chars().all(|c| c.is_ascii_hexdigit()) && stem.is_char_boundary(stem.len() - 32) => stem[..stem.len() - 32].trim_end(),
            _ => stem,
        },
    };
    match ext {
        Some(e) => format!("{stem}.{e}"),
        None => stem.to_string(),
    }
}

/// Every file in the export, nested zips opened: (path inside, bytes).
fn entries(bytes: &[u8], prefix: &str, out: &mut Vec<(String, Vec<u8>)>, depth: u32) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("This isn't a zip file Marrow can read: {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().replace('\\', "/");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if name.to_lowercase().ends_with(".zip") && depth < 3 {
            entries(&buf, prefix, out, depth + 1)?;
        } else {
            out.push((format!("{prefix}{name}"), buf));
        }
    }
    Ok(())
}

/// Normalises "a/./b/../c" style paths.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(a, _)| a).unwrap_or("")
}

/// Imports a Notion export zip into `dest` (a vault folder).
pub fn import(zip_bytes: &[u8], dest: &str) -> Result<Output, String> {
    let mut files = Vec::new();
    entries(zip_bytes, "", &mut files, 0)?;
    // Notion sometimes wraps everything in one top folder; keep it (it's the
    // workspace or page name) but drop the id.
    let mut out = Output::default();

    // Original path → new vault path, for every file.
    let mut map: HashMap<String, String> = HashMap::new();
    // Folder path (original) → cleaned folder path (under dest).
    let mut folders: HashMap<String, String> = HashMap::new();
    let clean_folder = |orig: &str, folders: &mut HashMap<String, String>| -> String {
        if orig.is_empty() {
            return dest.to_string();
        }
        if let Some(f) = folders.get(orig) {
            return f.clone();
        }
        let mut cur = dest.to_string();
        let mut acc = String::new();
        for part in orig.split('/') {
            acc = if acc.is_empty() { part.to_string() } else { format!("{acc}/{part}") };
            cur = match folders.get(&acc) {
                Some(f) => f.clone(),
                None => {
                    let c = format!("{cur}/{}", clean_name(&strip_id(part)));
                    folders.insert(acc.clone(), c.clone());
                    c
                }
            };
        }
        cur
    };
    let mut skipped_all = 0;
    for (p, _) in &files {
        if p.ends_with("_all.csv") {
            skipped_all += 1;
            continue;
        }
        let folder = clean_folder(parent(p), &mut folders);
        let file = p.rsplit('/').next().unwrap_or(p);
        let stripped = strip_id(file);
        let (stem, ext) = match stripped.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), e.to_lowercase()),
            None => (stripped.clone(), String::new()),
        };
        let ext_out = if ext == "csv" { "md".to_string() } else { ext.clone() };
        let path = out.reserve(&folder, &stem, &ext_out);
        map.insert(p.clone(), path);
    }
    let _ = skipped_all;

    // Note names that are unique, so links can be short.
    let mut name_count: HashMap<String, usize> = HashMap::new();
    for v in map.values() {
        if v.ends_with(".md") {
            let n = v.rsplit('/').next().unwrap().trim_end_matches(".md").to_lowercase();
            *name_count.entry(n).or_default() += 1;
        }
    }
    let link_name = |vault_path: &str| -> String {
        let base = vault_path.rsplit('/').next().unwrap().trim_end_matches(".md");
        if name_count.get(&base.to_lowercase()).copied().unwrap_or(0) <= 1 {
            base.to_string()
        } else {
            vault_path.trim_end_matches(".md").to_string()
        }
    };

    // Databases first: their rows' properties go into the row pages.
    let mut row_props: HashMap<String, Vec<(String, PropValue)>> = HashMap::new();
    let mut db_notes: Vec<(String, String)> = Vec::new();
    for (p, bytes) in &files {
        if !p.to_lowercase().ends_with(".csv") || p.ends_with("_all.csv") {
            continue;
        }
        let Some(note_path) = map.get(p).cloned() else { continue };
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(strip_bom(bytes));
        let headers: Vec<String> = match rdr.headers() {
            Ok(h) => h.iter().map(|s| s.trim().to_string()).collect(),
            Err(_) => {
                out.warn(format!("Couldn't read the database {}", p));
                continue;
            }
        };
        if headers.is_empty() {
            continue;
        }
        // The database's pages live in a folder of the same name.
        let orig_folder = p.trim_end_matches(".csv").to_string();
        let pages_folder = folders.get(&orig_folder).cloned();
        let mut rows = 0;
        for rec in rdr.records().flatten() {
            rows += 1;
            let title = rec.get(0).unwrap_or("").trim().to_string();
            if title.is_empty() {
                continue;
            }
            let props: Vec<(String, PropValue)> = headers
                .iter()
                .enumerate()
                .skip(1)
                .filter_map(|(i, h)| {
                    let v = rec.get(i).unwrap_or("").trim();
                    if v.is_empty() || h.is_empty() {
                        return None;
                    }
                    let val = prop_value(v);
                    Some((clean_prop(h), val))
                })
                .collect();
            let key = match &pages_folder {
                Some(f) => format!("{f}/{}", clean_name(&title)).to_lowercase(),
                None => clean_name(&title).to_lowercase(),
            };
            row_props.insert(key, props);
        }
        let db_from = pages_folder.clone().unwrap_or_else(|| parent(&note_path).to_string());
        let cols: Vec<String> = headers.iter().skip(1).filter(|h| !h.is_empty()).map(|h| clean_prop(h)).collect();
        let title = note_path.rsplit('/').next().unwrap().trim_end_matches(".md").to_string();
        let block = format!(
            "# {title}\n\n```database\nfrom: {db_from}\nview: table\ncolumns: [{}]\n```\n",
            cols.iter().map(|c| if c.contains(',') || c.contains(':') { format!("\"{c}\"") } else { c.clone() }).collect::<Vec<_>>().join(", ")
        );
        let _ = rows;
        db_notes.push((note_path, block));
    }

    // Pages.
    let mut pages_written: HashSet<String> = HashSet::new();
    for (p, bytes) in &files {
        let Some(new_path) = map.get(p).cloned() else { continue };
        let lower = p.to_lowercase();
        if lower.ends_with(".csv") {
            continue;
        }
        if lower.ends_with(".md") {
            let text = String::from_utf8_lossy(strip_bom(bytes)).to_string();
            let dir = parent(p).to_string();
            let mut body = rewrite_links(&text, &dir, &map, &link_name);
            // "Key: value" lines right under the title are properties.
            let (mut props, rest) = take_properties(&body);
            body = rest;
            let key = new_path.trim_end_matches(".md").to_lowercase();
            if let Some(extra) = row_props.remove(&key) {
                for (k, v) in extra {
                    if !props.iter().any(|(pk, _)| pk.eq_ignore_ascii_case(&k)) {
                        props.push((k, v));
                    }
                }
            }
            pages_written.insert(key);
            out.note(new_path, format!("{}{}", frontmatter(&props), tidy(&body)));
        } else {
            out.attachment(new_path, bytes.clone());
        }
    }
    // Rows without a page of their own.
    for (key, props) in row_props {
        if pages_written.contains(&key) {
            continue;
        }
        let path = format!("{}.md", key_to_path(&key, &folders, dest));
        out.note(path, frontmatter(&props));
    }
    for (path, text) in db_notes {
        out.note(path, text);
    }
    if out.summary.notes == 0 {
        return Err("No pages were found in this file. In Notion, use Export → “Markdown & CSV”, then pick the .zip it saves.".into());
    }
    Ok(out)
}

/// Rows' keys are lower-cased paths; find the real casing from the folders.
fn key_to_path(key: &str, folders: &HashMap<String, String>, dest: &str) -> String {
    let (folder_lower, name) = key.rsplit_once('/').unwrap_or(("", key));
    let folder = folders.values().find(|f| f.to_lowercase() == folder_lower).cloned().unwrap_or_else(|| dest.to_string());
    // The name was lower-cased; title case is a fair guess.
    let mut c = name.chars();
    let name = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "Untitled".into(),
    };
    format!("{folder}/{name}")
}

/// A Notion value: a date, a comma-separated list (multi-select, relation)
/// or a single value.
fn prop_value(v: &str) -> PropValue {
    if let Some(d) = super::notion_date(v) {
        return PropValue::Text(d);
    }
    if v.contains(", ") {
        return PropValue::List(v.split(", ").map(strip_notion_link).collect());
    }
    guess_value(&strip_notion_link(v))
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)
}

fn clean_prop(h: &str) -> String {
    let c: String = h.chars().map(|c| if c == ':' || c == '#' || c.is_control() { ' ' } else { c }).collect();
    let c = c.split_whitespace().collect::<Vec<_>>().join(" ");
    if c.is_empty() {
        "property".into()
    } else {
        c
    }
}

/// "Page (Page%20abc….md)" (how Notion writes relations) → "Page".
fn strip_notion_link(v: &str) -> String {
    match v.find(" (") {
        Some(i) if v.ends_with(')') && v[i..].contains(".md") => v[..i].to_string(),
        _ => v.to_string(),
    }
}

/// Lines like "Status: Done" just under the "# Title" line.
fn take_properties(text: &str) -> (Vec<(String, PropValue)>, String) {
    let lines: Vec<&str> = text.lines().collect();
    let Some(h1) = lines.iter().position(|l| !l.trim().is_empty()) else { return (Vec::new(), text.to_string()) };
    if !lines[h1].starts_with("# ") {
        return (Vec::new(), text.to_string());
    }
    let mut i = h1 + 1;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let start = i;
    let mut props = Vec::new();
    while i < lines.len() {
        let l = lines[i];
        let Some((k, v)) = l.split_once(": ") else { break };
        if k.is_empty() || k.len() > 40 || k.starts_with(['#', '-', '*', '>', '|', '!', '[']) || k.contains("](") {
            break;
        }
        let v = v.trim();
        let val = prop_value(v);
        props.push((clean_prop(k), val));
        i += 1;
    }
    // Only a run of two or more counts (one "Note: …" line is just text).
    if props.len() < 2 || (i < lines.len() && !lines[i].trim().is_empty()) {
        return (Vec::new(), text.to_string());
    }
    let mut rest: Vec<&str> = lines[..start].to_vec();
    rest.extend_from_slice(&lines[i..]);
    (props, rest.join("\n"))
}

/// `[text](Some%20Page%20abc….md)` → `[[Some Page|text]]`; images and files → embeds.
fn rewrite_links(text: &str, dir: &str, map: &HashMap<String, String>, link_name: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        // Find the opening '[' of this link (no nesting across lines).
        let before = &rest[..i];
        let Some(open) = before.rfind('[') else {
            out.push_str(&rest[..i + 2]);
            rest = &rest[i + 2..];
            continue;
        };
        let Some(close) = rest[i + 2..].find(')') else { break };
        let label = &rest[open + 1..i];
        let href = &rest[i + 2..i + 2 + close];
        let bang = open > 0 && rest.as_bytes()[open - 1] == b'!';
        let start = if bang { open - 1 } else { open };
        let replacement = if href.contains("://") || href.starts_with('#') || href.starts_with("mailto:") || label.contains('\n') {
            None
        } else {
            let decoded = percent_decode_str(href.split('#').next().unwrap_or(href)).decode_utf8_lossy().to_string();
            let target = normalize(&if dir.is_empty() { decoded.clone() } else { format!("{dir}/{decoded}") });
            map.get(&target).map(|vp| {
                if vp.ends_with(".md") {
                    let n = link_name(vp);
                    if label.is_empty() || label == n {
                        format!("[[{n}]]")
                    } else {
                        format!("[[{n}|{}]]", label.replace(['[', ']', '|'], ""))
                    }
                } else if bang || is_image(vp) {
                    format!("![[{vp}]]")
                } else {
                    format!("[[{vp}|{}]]", if label.is_empty() { vp.rsplit('/').next().unwrap_or(vp) } else { label })
                }
            })
        };
        match replacement {
            Some(r) => {
                out.push_str(&rest[..start]);
                out.push_str(&r);
            }
            None => out.push_str(&rest[..i + 2 + close + 1]),
        }
        rest = &rest[i + 2 + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, b) in files {
                z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn ids_go() {
        assert_eq!(strip_id("Projects 0123456789abcdef0123456789abcdef.md"), "Projects.md");
        assert_eq!(strip_id("Projects 0123456789abcdef0123456789abcdef"), "Projects");
        assert_eq!(strip_id("photo.png"), "photo.png");
    }

    #[test]
    fn imports_pages_links_and_databases() {
        let id = "0123456789abcdef0123456789abcdef";
        let id2 = "fedcba9876543210fedcba9876543210";
        let home = format!("# Home\n\nSee [Tasks](Tasks%20{id2}.csv) and [Plan](Home%20{id}/Plan%20{id}.md).\n\n![](Home%20{id}/pic.png)\n");
        let plan = "# Plan\n\nStatus: Draft\nOwner: Ana\n\nThe plan text.\n".to_string();
        let csv = "\u{feff}Name,Status,Due\nWrite intro,Done,\"October 20, 2026\"\nNo page,To do,\n";
        let row = "# Write intro\n\nStatus: Done\nDue: October 20, 2026\n\nBody.\n";
        let z = zip_of(&[
            (&format!("Home {id}.md"), home.as_bytes()),
            (&format!("Home {id}/Plan {id}.md"), plan.as_bytes()),
            (&format!("Home {id}/pic.png"), b"PNG"),
            (&format!("Tasks {id2}.csv"), csv.as_bytes()),
            (&format!("Tasks {id2}_all.csv"), csv.as_bytes()),
            (&format!("Tasks {id2}/Write intro {id}.md"), row.as_bytes()),
        ]);
        let outer = zip_of(&[("Export-1.zip", &z)]);
        let out = import(&outer, "Imported").unwrap();
        let get = |p: &str| String::from_utf8(out.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("missing {p}")).bytes.clone()).unwrap();
        let home = get("Imported/Home.md");
        assert!(home.contains("[[Tasks]]"), "{home}");
        assert!(home.contains("[[Plan]]"), "{home}");
        assert!(home.contains("![[Imported/Home/pic.png]]"), "{home}");
        let plan = get("Imported/Home/Plan.md");
        assert!(plan.starts_with("---\n") && plan.contains("Status: Draft") && plan.contains("Owner: Ana"), "{plan}");
        let body = plan.split("# Plan").nth(1).unwrap();
        assert!(!body.contains("Status:") && body.contains("The plan text."), "{plan}");
        let db = get("Imported/Tasks.md");
        assert!(db.contains("```database\nfrom: Imported/Tasks"), "{db}");
        assert!(db.contains("columns: [Status, Due]"), "{db}");
        let row = get("Imported/Tasks/Write intro.md");
        assert!(row.contains("Due: 2026-10-20") || row.contains("Due: '2026-10-20'"), "{row}");
        assert!(out.files.iter().any(|f| f.path == "Imported/Tasks/No page.md"));
        assert!(!out.files.iter().any(|f| f.path.contains("_all")));
    }
}
