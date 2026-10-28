//! Whole syncs between two "devices" through each pretend service.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use marrow_core::mirror::{Mirror, Remote};
use marrow_core::sealed::Sealed;

use crate::dropbox::Dropbox;
use crate::fake;
use crate::gdrive::GoogleDrive;
use crate::oauth::{Fixed, TokenError, TokenSource};
use crate::onedrive::OneDrive;

fn good() -> Arc<dyn TokenSource> {
    Arc::new(Fixed("good".into()))
}

/// Hands out a stale token first, then a good one when asked to renew.
struct Stale(AtomicUsize);

impl TokenSource for Stale {
    fn token(&self, renew: bool) -> Result<String, TokenError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(if renew { "good".into() } else { "stale".into() })
    }
}

fn put(root: &Path, key: &str, text: &str) {
    let p = root.join(key);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn get(root: &Path, key: &str) -> Option<String> {
    fs::read_to_string(root.join(key)).ok()
}

/// Runs the same story against any service: `make` connects one device.
type Connect<'a> = &'a dyn Fn(Arc<dyn TokenSource>, &str) -> Box<dyn Remote>;

fn story(make: Connect) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    put(&a, "Welcome.md", "one\ntwo\nthree\n");
    put(&a, "Projects/Garden plan.md", "tomatoes");
    put(&a, "বাংলা/কবিতা #1.md", "আকাশ");
    put(&a, ".obsidian/app.json", "{}");
    put(&a, "photo.png", "\u{89}PNG");

    let ma = Mirror::with_remote(&a, make(good(), "My notes"));
    let mb = Mirror::with_remote(&b, make(good(), "My notes"));

    let r = ma.sync(false).unwrap();
    assert_eq!(r.to_remote.len(), 4, "{r:?}");
    assert!(r.errors.is_empty(), "{:?}", r.errors);

    // The second device gets everything (but nothing hidden).
    let r = mb.sync(false).unwrap();
    assert_eq!(r.to_local.len(), 4, "{r:?}");
    assert_eq!(get(&b, "বাংলা/কবিতা #1.md").as_deref(), Some("আকাশ"));
    assert_eq!(get(&b, "Projects/Garden plan.md").as_deref(), Some("tomatoes"));
    assert!(!b.join(".obsidian").exists());

    // Edits, a new note and a delete on the second device…
    put(&b, "Welcome.md", "one\ntwo (from b)\nthree\n");
    put(&b, "Ideas.md", "an idea");
    fs::remove_file(b.join("Projects/Garden plan.md")).unwrap();
    let r = mb.sync(false).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.deleted_remote, vec!["Projects/Garden plan.md".to_string()]);

    // …reach the first.
    let r = ma.sync(false).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(get(&a, "Ideas.md").as_deref(), Some("an idea"));
    assert_eq!(get(&a, "Welcome.md").as_deref(), Some("one\ntwo (from b)\nthree\n"));
    assert!(!a.join("Projects/Garden plan.md").exists());

    // Both change the same note at once: merged, nothing lost.
    put(&a, "Welcome.md", "one (from a)\ntwo (from b)\nthree\n");
    put(&b, "Welcome.md", "one\ntwo (from b)\nthree (from b)\n");
    ma.sync(false).unwrap();
    let r = mb.sync(false).unwrap();
    assert_eq!(r.merged, vec!["Welcome.md".to_string()]);
    ma.sync(false).unwrap();
    let want = "one (from a)\ntwo (from b)\nthree (from b)\n";
    assert_eq!(get(&a, "Welcome.md").as_deref(), Some(want));
    assert_eq!(get(&b, "Welcome.md").as_deref(), Some(want));

    // A quiet run changes nothing.
    assert!(!ma.sync(false).unwrap().changed());
    assert!(!mb.sync(false).unwrap().changed());

    // A token that ran out is renewed once, by itself.
    let stale = Arc::new(Stale(AtomicUsize::new(0)));
    let mc = Mirror::with_remote(dir.path().join("c"), make(stale.clone(), "My notes"));
    fs::create_dir_all(dir.path().join("c")).unwrap();
    mc.sync(false).unwrap();
    assert!(stale.0.load(Ordering::Relaxed) >= 2);
    assert_eq!(get(&dir.path().join("c"), "Ideas.md").as_deref(), Some("an idea"));

    // End-to-end encrypted, in another vault folder.
    let (c, d) = (dir.path().join("enc-a"), dir.path().join("enc-b"));
    fs::create_dir_all(&c).unwrap();
    fs::create_dir_all(&d).unwrap();
    put(&c, "Diary/Today.md", "a secret line");
    let mc = Mirror::with_remote(&c, Box::new(Sealed::new(make(good(), "Private"), "correct horse battery")));
    mc.sync(false).unwrap();
    let md = Mirror::with_remote(&d, Box::new(Sealed::new(make(good(), "Private"), "correct horse battery")));
    md.sync(false).unwrap();
    assert_eq!(get(&d, "Diary/Today.md").as_deref(), Some("a secret line"));
    // The wrong passphrase can't read it.
    let wrong = Mirror::with_remote(dir.path().join("x"), Box::new(Sealed::new(make(good(), "Private"), "something else")));
    fs::create_dir_all(dir.path().join("x")).unwrap();
    assert!(wrong.sync(false).is_err());
}

#[test]
fn dropbox_story() {
    let (url, state) = fake::dropbox();
    story(&|t, v| Box::new(Dropbox::with_base(t, "me", v, &url, &url)));
    let s = state.lock().unwrap();
    // Plain files, findable in Apps › Marrow › My notes.
    assert!(s.files.contains_key("/my notes/ideas.md"));
    assert_eq!(s.files["/my notes/বাংলা/কবিতা #1.md"].0, "/My notes/বাংলা/কবিতা #1.md");
    assert!(!s.files.keys().any(|k| k.contains(".obsidian")));
    // The emptied folder was tidied away.
    assert!(!s.folders.contains_key("/my notes/projects"));
    // Nothing readable in the encrypted vault.
    assert!(s.files.iter().filter(|(k, _)| k.starts_with("/private/")).all(|(_, f)| !String::from_utf8_lossy(&f.1).contains("secret")));
    drop(s);
    let d = Dropbox::with_base(good(), "", "", &url, &url);
    let mut f = d.folders().unwrap();
    f.sort();
    assert_eq!(f, ["My notes", "Private"]);
    assert_eq!(d.account().unwrap(), "khan@example.com");
}

#[test]
fn onedrive_story() {
    let (url, state) = fake::onedrive();
    story(&|t, v| Box::new(OneDrive::with_base(t, "me", v, &format!("{url}/v1.0"))));
    let s = state.lock().unwrap();
    assert!(s.files.contains_key("My notes/Ideas.md"));
    assert!(s.files.contains_key("My notes/বাংলা/কবিতা #1.md"));
    // Folders were made when an upload needed them.
    assert!(s.missing_parent > 0);
    assert!(!s.folders.contains("My notes/Projects"));
    drop(s);
    let d = OneDrive::with_base(good(), "", "", &format!("{url}/v1.0"));
    let mut f = d.folders().unwrap();
    f.sort();
    assert_eq!(f, ["My notes", "Private"]);
    assert_eq!(d.account().unwrap(), "khan@outlook.com");
}

#[test]
fn google_drive_story() {
    let (url, state) = fake::gdrive();
    story(&|t, v| Box::new(GoogleDrive::with_base(t, "me", v, &url)));
    let s = state.lock().unwrap();
    let mut paths = s.live_paths();
    paths.sort();
    assert!(paths.contains(&"Marrow/My notes/Ideas.md".to_string()), "{paths:?}");
    assert!(paths.contains(&"Marrow/My notes/বাংলা/কবিতা #1.md".to_string()));
    assert!(!paths.iter().any(|p| p.contains("Garden plan")));
    // One Marrow folder and one folder per vault, however many devices.
    let folders: Vec<String> = s.files.iter().filter(|(_, f)| f.folder && !f.trashed).map(|(id, _)| s.path_of(id)).collect();
    assert_eq!(folders.iter().filter(|p| *p == "Marrow").count(), 1, "{folders:?}");
    assert_eq!(folders.iter().filter(|p| *p == "Marrow/My notes").count(), 1);
    // The emptied Projects folder went to the trash.
    assert!(!folders.contains(&"Marrow/My notes/Projects".to_string()));
    drop(s);
    let d = GoogleDrive::with_base(good(), "", "", &url);
    let mut f = d.folders().unwrap();
    f.sort();
    assert_eq!(f, ["My notes", "Private"]);
    assert_eq!(d.account().unwrap(), "khan@gmail.com");
}

#[test]
fn signed_out_stops_sync_with_a_sign_in_problem() {
    let (url, _) = fake::dropbox();
    let dir = tempfile::tempdir().unwrap();
    let m = Mirror::with_remote(dir.path(), Box::new(Dropbox::with_base(Arc::new(Fixed("revoked".into())), "me", "v", &url, &url)));
    assert!(matches!(m.sync(false), Err(marrow_core::mirror::SyncStop::SignIn { .. })));
}
