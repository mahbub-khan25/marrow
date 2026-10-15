//! Vault-relative paths.
//!
//! Every file inside a vault is addressed by a [`VaultPath`]: a `/`-separated,
//! Unicode-NFC-normalised path relative to the vault root, with no leading or
//! trailing slash. The vault root itself is the empty path.
//!
//! Keeping one canonical spelling means the same note is never indexed twice
//! because one tool wrote `Café.md` decomposed and another composed, or
//! because Windows handed us a backslash.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, Result};

/// A normalised path relative to the vault root.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VaultPath(String);

impl VaultPath {
    /// The vault root.
    pub fn root() -> Self {
        VaultPath(String::new())
    }

    /// Normalises a user- or OS-supplied relative path.
    ///
    /// Accepts `/` or `\` separators, drops `.` segments and empty segments,
    /// and rejects `..` or absolute paths so nothing can escape the vault.
    pub fn new(raw: &str) -> Result<Self> {
        let mut parts: Vec<String> = Vec::new();
        for seg in raw.split(['/', '\\']) {
            match seg {
                "" | "." => continue,
                ".." => return Err(Error::InvalidPath(raw.to_string())),
                s => parts.push(s.nfc().collect()),
            }
        }
        if raw.starts_with('/') && raw.len() > 1 && parts.is_empty() {
            return Err(Error::InvalidPath(raw.to_string()));
        }
        // A Windows drive prefix ("C:") as the first segment means an absolute path.
        if let Some(first) = parts.first() {
            if first.len() == 2 && first.ends_with(':') {
                return Err(Error::InvalidPath(raw.to_string()));
            }
        }
        Ok(VaultPath(parts.join("/")))
    }

    /// Builds a vault path from an absolute OS path under `root`.
    pub fn from_os(root: &Path, full: &Path) -> Result<Self> {
        let rel = full
            .strip_prefix(root)
            .map_err(|_| Error::OutsideVault(full.to_path_buf()))?;
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(s) => parts.push(
                    s.to_str()
                        .ok_or_else(|| Error::InvalidPath(full.display().to_string()))?
                        .nfc()
                        .collect::<String>(),
                ),
                Component::CurDir => {}
                _ => return Err(Error::OutsideVault(full.to_path_buf())),
            }
        }
        Ok(VaultPath(parts.join("/")))
    }

    /// The absolute OS path for this vault path under `root`.
    pub fn to_os(&self, root: &Path) -> PathBuf {
        let mut p = root.to_path_buf();
        for seg in self.segments() {
            p.push(seg);
        }
        p
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// Last segment, with extension: `notes/Idea.md` → `Idea.md`.
    pub fn name(&self) -> &str {
        match self.0.rfind('/') {
            Some(i) => &self.0[i + 1..],
            None => &self.0,
        }
    }

    /// Name without its final extension: `Idea.md` → `Idea`.
    pub fn stem(&self) -> &str {
        let n = self.name();
        match n.rfind('.') {
            Some(i) if i > 0 => &n[..i],
            _ => n,
        }
    }

    /// Lower-case extension without the dot, or `""`.
    pub fn extension(&self) -> String {
        let n = self.name();
        match n.rfind('.') {
            Some(i) if i > 0 => n[i + 1..].to_lowercase(),
            _ => String::new(),
        }
    }

    pub fn is_markdown(&self) -> bool {
        self.extension() == "md"
    }

    /// Parent folder; the root's parent is the root.
    pub fn parent(&self) -> VaultPath {
        match self.0.rfind('/') {
            Some(i) => VaultPath(self.0[..i].to_string()),
            None => VaultPath::root(),
        }
    }

    pub fn join(&self, child: &str) -> Result<VaultPath> {
        if self.is_root() {
            VaultPath::new(child)
        } else {
            VaultPath::new(&format!("{}/{}", self.0, child))
        }
    }

    /// The path without a trailing `.md`, as links usually spell it.
    pub fn without_md(&self) -> &str {
        if self.is_markdown() {
            &self.0[..self.0.len() - 3]
        } else {
            &self.0
        }
    }

    /// True if this path is `other` or lies inside it.
    pub fn starts_with(&self, other: &VaultPath) -> bool {
        other.is_root()
            || self.0 == other.0
            || (self.0.starts_with(&other.0) && self.0.as_bytes().get(other.0.len()) == Some(&b'/'))
    }

    /// True if any segment starts with a dot (`.git`, `.marrow`, `.obsidian`).
    pub fn is_hidden(&self) -> bool {
        self.segments().any(|s| s.starts_with('.'))
    }
}

impl fmt::Debug for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_separators_and_dots() {
        let p = VaultPath::new("\\notes/./daily//2026.md").unwrap();
        assert_eq!(p.as_str(), "notes/daily/2026.md");
        assert_eq!(p.name(), "2026.md");
        assert_eq!(p.stem(), "2026");
        assert_eq!(p.extension(), "md");
        assert_eq!(p.parent().as_str(), "notes/daily");
        assert_eq!(p.without_md(), "notes/daily/2026");
    }

    #[test]
    fn rejects_escapes() {
        assert!(VaultPath::new("../x.md").is_err());
        assert!(VaultPath::new("a/../../x.md").is_err());
        assert!(VaultPath::new("C:/x.md").is_err());
    }

    #[test]
    fn nfc_normalises() {
        let decomposed = "Cafe\u{301}.md";
        let composed = "Caf\u{e9}.md";
        assert_eq!(
            VaultPath::new(decomposed).unwrap(),
            VaultPath::new(composed).unwrap()
        );
    }

    #[test]
    fn starts_with_respects_segments() {
        let a = VaultPath::new("notes/a.md").unwrap();
        assert!(a.starts_with(&VaultPath::new("notes").unwrap()));
        assert!(!a.starts_with(&VaultPath::new("not").unwrap()));
        assert!(a.starts_with(&VaultPath::root()));
    }

    #[test]
    fn dotfiles_and_hidden() {
        assert_eq!(VaultPath::new(".gitignore").unwrap().stem(), ".gitignore");
        assert!(VaultPath::new(".marrow/app.json").unwrap().is_hidden());
        assert!(!VaultPath::new("a/b.md").unwrap().is_hidden());
    }
}
