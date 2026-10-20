//! Portable mode: run Marrow from any folder (a USB stick, a synced folder,
//! a locked-down work laptop) without installing it.
//!
//! When a file named `portable` (or `portable.txt`) sits next to the program,
//! everything Marrow keeps for itself goes into a `Data` folder beside it
//! instead of the user's AppData folders:
//!
//! | what                          | normally                       | portable              |
//! |-------------------------------|--------------------------------|-----------------------|
//! | settings, last vault          | %APPDATA%\app.marrow.notes     | Data\config           |
//! | search index cache            | %LOCALAPPDATA%\…\cache         | Data\cache            |
//! | backups of failed saves       | %APPDATA%\…                    | Data\backups          |
//! | window state (tabs, prefs)    | %LOCALAPPDATA%\…\EBWebView     | Data\webview          |
//!
//! Vault paths inside the program's folder are stored relative to it, so the
//! stick still works when Windows gives it a different drive letter.
//!
//! If a `WebView2` folder (Microsoft's "fixed version" WebView2 runtime) sits
//! next to the program too, Marrow uses it, so it runs even on a PC without
//! WebView2.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The program's folder, if Marrow runs in portable mode.
pub fn root() -> Option<&'static Path> {
    ROOT.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let dir = dunce::canonicalize(exe.parent()?).ok()?;
        (dir.join("portable").is_file() || dir.join("portable.txt").is_file()).then_some(dir)
    })
    .as_deref()
}

/// Where portable data lives (`<program folder>\Data\<name>`).
pub fn data(name: &str) -> Option<PathBuf> {
    root().map(|r| r.join("Data").join(name))
}

/// Before the window opens: use a bundled WebView2 runtime if there is one.
pub fn prepare() {
    let Some(root) = root() else { return };
    let bundled = root.join("WebView2");
    if bundled.join("msedgewebview2.exe").is_file() && std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_none() {
        // Read by WebView2 when it starts; set before any window exists.
        std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &bundled);
    }
}

/// A path to store: relative to the program folder when it's inside it.
pub fn to_stored(path: &str) -> String {
    stored_in(root(), path)
}

/// A stored path back to a real one.
pub fn from_stored(path: &str) -> String {
    resolved_in(root(), path)
}

fn stored_in(root: Option<&Path>, path: &str) -> String {
    let Some(root) = root else { return path.to_string() };
    let p = Path::new(path);
    match p.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => format!("./{}", rel.to_string_lossy().replace('\\', "/")),
        _ => path.to_string(),
    }
}

fn resolved_in(root: Option<&Path>, path: &str) -> String {
    match (root, path.strip_prefix("./")) {
        (Some(root), Some(rel)) => root.join(rel).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_inside_the_program_folder_are_relative() {
        let root = Path::new("/media/usb/Marrow");
        assert_eq!(stored_in(Some(root), "/media/usb/Marrow/Notes"), "./Notes");
        assert_eq!(stored_in(Some(root), "/home/me/Notes"), "/home/me/Notes");
        assert_eq!(stored_in(None, "/media/usb/Marrow/Notes"), "/media/usb/Marrow/Notes");
        // Joined with the platform's own separator (`\` on Windows).
        let expect = Path::new("/mnt/e/Marrow").join("Notes").to_string_lossy().into_owned();
        assert_eq!(resolved_in(Some(Path::new("/mnt/e/Marrow")), "./Notes"), expect);
        assert_eq!(resolved_in(None, "./Notes"), "./Notes");
    }
}
