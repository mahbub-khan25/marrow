//! Conflict copies made by sync apps.
//!
//! When a note is changed on two computers before they sync, the sync app
//! keeps both by saving one as a copy beside the original:
//!
//! | app          | copy is named                                        |
//! |--------------|------------------------------------------------------|
//! | Dropbox      | `Note (Khan's conflicted copy 2026-10-15).md`        |
//! | OneDrive     | `Note-DESKTOP-AB12.md` (the computer's name)         |
//! | Google Drive | `Note [Conflict].md`                                 |
//! | Syncthing    | `Note.sync-conflict-20261015-101500-ABCDEFG.md`      |
//! | Marrow       | `Note (Marrow conflicted copy 2026-10-15).md`        |
//!
//! Only notes are reported, and only when the original is still there, so
//! the copy can be merged back into it.

use std::collections::HashSet;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictCopy {
    /// Vault path of the copy.
    pub copy: String,
    /// Vault path of the note it's a copy of.
    pub original: String,
    /// "dropbox", "onedrive", "gdrive", "syncthing" or "marrow".
    pub source: String,
}

/// Finds conflict copies among `paths` (vault paths with `/`).
pub fn find<S: AsRef<str>>(paths: &[S]) -> Vec<ConflictCopy> {
    let all: HashSet<&str> = paths.iter().map(|p| p.as_ref()).collect();
    let mut out = vec![];
    for p in paths {
        let p = p.as_ref();
        let (dir, file) = match p.rfind('/') {
            Some(i) => (&p[..=i], &p[i + 1..]),
            None => ("", p),
        };
        let Some(stem) = file.strip_suffix(".md") else { continue };
        let hit = original_stems(stem)
            .into_iter()
            .map(|(base, source)| (format!("{dir}{base}.md"), source))
            .find(|(original, _)| original != p && all.contains(original.as_str()));
        if let Some((original, source)) = hit {
            out.push(ConflictCopy {
                copy: p.to_string(),
                original,
                source: source.into(),
            });
        }
    }
    out.sort_by(|a, b| a.copy.cmp(&b.copy));
    out
}

/// Possible original names for a copy's name, and who made the copy.
fn original_stems(stem: &str) -> Vec<(&str, &'static str)> {
    // Dropbox, and Marrow's own copies.
    if let Some(i) = stem.find("conflicted copy") {
        if stem.ends_with(')') {
            if let Some(open) = stem[..i].rfind(" (") {
                let source = if stem[open..i].contains("Marrow") { "marrow" } else { "dropbox" };
                return vec![(stem[..open].trim_end(), source)];
            }
        }
    }
    // Syncthing.
    if let Some(i) = stem.find(".sync-conflict-") {
        return vec![(&stem[..i], "syncthing")];
    }
    // Google Drive.
    let lower = stem.to_lowercase();
    if lower.ends_with("[conflict]") {
        return vec![(stem[..stem.len() - "[conflict]".len()].trim_end(), "gdrive")];
    }
    // OneDrive adds "-COMPUTERNAME". Computer names are at most 15 characters,
    // and OneDrive shows them in capitals; requiring that keeps names like
    // "plan-v2" from being mistaken for copies.
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = stem[from..].find('-').map(|i| i + from) {
        let suffix = &stem[i + 1..];
        let ok = (3..=15).contains(&suffix.len())
            && suffix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
            && suffix.chars().any(|c| c.is_ascii_uppercase())
            && i > 0;
        if ok {
            out.push((&stem[..i], "onedrive"));
        }
        from = i + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_copies_with_their_originals() {
        let paths = [
            "Note.md",
            "Note (Khan's conflicted copy 2026-10-15).md",
            "Daily/2026-10-15.md",
            "Daily/2026-10-15-DESKTOP-AB12.md",
            "Plan.md",
            "Plan [Conflict].md",
            "Ideas.md",
            "Ideas.sync-conflict-20261015-101500-ABCDEFG.md",
            "Orphan (Khan's conflicted copy 2026-10-15).md",
            "plan-v2.md",
            "plan.md",
            "Read me.md",
            "Read me (Marrow conflicted copy 2026-10-15).md",
            "pic (Khan's conflicted copy 2026-10-15).png",
            "pic.png",
        ];
        let found: Vec<(String, String, String)> =
            find(&paths).into_iter().map(|c| (c.copy, c.original, c.source)).collect();
        let want = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(
            found,
            vec![
                want("Daily/2026-10-15-DESKTOP-AB12.md", "Daily/2026-10-15.md", "onedrive"),
                want("Ideas.sync-conflict-20261015-101500-ABCDEFG.md", "Ideas.md", "syncthing"),
                want("Note (Khan's conflicted copy 2026-10-15).md", "Note.md", "dropbox"),
                want("Plan [Conflict].md", "Plan.md", "gdrive"),
                want("Read me (Marrow conflicted copy 2026-10-15).md", "Read me.md", "marrow"),
            ]
        );
    }
}
