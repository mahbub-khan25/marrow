//! Runs an [`Engine`] with a live file watcher.
//!
//! The engine sits behind a mutex. A watcher thread turns OS events into
//! vault paths (debounced), locks the engine, reconciles those paths and
//! hands the resulting [`EngineEvent`]s to a callback (the Tauri shell will
//! forward them to the window). UI commands lock the same mutex, so a save
//! and a watcher event can never interleave.
//!
//! The watcher's event kinds are ignored on purpose: every reported path is
//! re-checked against disk by [`crate::vault::Vault::reconcile`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::cache::Cache;
use crate::engine::{Engine, EngineEvent};
use crate::error::{Error, Result};
use crate::path::VaultPath;

/// How long the watcher waits for a burst of events to settle.
pub const DEBOUNCE: Duration = Duration::from_millis(250);

enum Msg {
    Paths(Vec<VaultPath>),
    Rescan,
    Stop,
}

pub struct VaultService {
    engine: Arc<Mutex<Engine>>,
    tx: Sender<Msg>,
    worker: Option<JoinHandle<()>>,
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl VaultService {
    /// Opens the vault at `root`, with its cache at `cache_path` (keep that
    /// outside the vault, in the app's data folder), and starts watching.
    pub fn start(
        root: &Path,
        cache_path: &Path,
        on_events: impl Fn(Vec<EngineEvent>) + Send + 'static,
    ) -> Result<Self> {
        let cache = Cache::open(cache_path)?;
        Self::start_with_cache(root, cache, on_events)
    }

    pub fn start_with_cache(
        root: &Path,
        cache: Cache,
        on_events: impl Fn(Vec<EngineEvent>) + Send + 'static,
    ) -> Result<Self> {
        let engine = Engine::open(root, cache)?;
        let canonical: PathBuf = engine.vault().root().to_path_buf();
        let engine = Arc::new(Mutex::new(engine));
        let (tx, rx) = unbounded::<Msg>();

        let watch_tx = tx.clone();
        let watch_root = canonical.clone();
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |res: DebounceEventResult| {
            match res {
                Ok(events) => {
                    let mut paths: BTreeSet<VaultPath> = BTreeSet::new();
                    let mut rescan = false;
                    for ev in events {
                        if ev.need_rescan() {
                            rescan = true;
                        }
                        for full in &ev.paths {
                            if let Ok(vp) = VaultPath::from_os(&watch_root, full) {
                                if !vp.is_root() && !vp.is_hidden() {
                                    paths.insert(vp);
                                }
                            }
                        }
                    }
                    if rescan {
                        let _ = watch_tx.send(Msg::Rescan);
                    } else if !paths.is_empty() {
                        let _ = watch_tx.send(Msg::Paths(paths.into_iter().collect()));
                    }
                }
                Err(_) => {
                    // The OS dropped events; fall back to a full check.
                    let _ = watch_tx.send(Msg::Rescan);
                }
            }
        })
        .map_err(|e| Error::Watch(e.to_string()))?;
        debouncer
            .watch(&canonical, RecursiveMode::Recursive)
            .map_err(|e| Error::Watch(e.to_string()))?;

        // Load note text for search in the background, so the first search
        // doesn't pause. Commands wait for the lock only while it runs.
        let warm = engine.clone();
        let _ = std::thread::Builder::new()
            .name("marrow-warm".into())
            .spawn(move || {
                if let Ok(mut e) = warm.lock() {
                    e.warm_search();
                }
            });

        let worker_engine = engine.clone();
        let worker = std::thread::Builder::new()
            .name("marrow-watch".into())
            .spawn(move || run(worker_engine, rx, on_events))
            .map_err(|e| Error::Watch(e.to_string()))?;

        Ok(VaultService {
            engine,
            tx,
            worker: Some(worker),
            _debouncer: debouncer,
        })
    }

    /// Locks the engine for a command from the UI.
    pub fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Shared handle, for code that needs to outlive a borrow of the service.
    pub fn engine_handle(&self) -> Arc<Mutex<Engine>> {
        self.engine.clone()
    }

    /// Asks the watcher thread for a full re-check of the vault.
    pub fn request_rescan(&self) {
        let _ = self.tx.send(Msg::Rescan);
    }
}

impl Drop for VaultService {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

fn run(engine: Arc<Mutex<Engine>>, rx: Receiver<Msg>, on_events: impl Fn(Vec<EngineEvent>)) {
    while let Ok(msg) = rx.recv() {
        // Coalesce whatever else is already queued.
        let mut paths: BTreeSet<VaultPath> = BTreeSet::new();
        let mut rescan = false;
        let mut stop = false;
        let mut take = |m: Msg| match m {
            Msg::Paths(p) => paths.extend(p),
            Msg::Rescan => rescan = true,
            Msg::Stop => stop = true,
        };
        take(msg);
        while let Ok(m) = rx.try_recv() {
            take(m);
        }
        if stop {
            break;
        }
        let result = {
            let mut e = engine.lock().unwrap_or_else(|p| p.into_inner());
            if rescan {
                e.rescan()
            } else {
                let list: Vec<VaultPath> = paths.into_iter().collect();
                e.on_paths_changed(&list)
            }
        };
        match result {
            Ok(events) if !events.is_empty() => on_events(events),
            Ok(_) => {}
            Err(err) => eprintln!("marrow: watcher update failed: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    fn p(s: &str) -> VaultPath {
        VaultPath::new(s).unwrap()
    }

    /// Polls until `cond` holds or the deadline passes.
    fn wait_for(svc: &VaultService, cond: impl Fn(&Engine) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if cond(&svc.engine()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn external_changes_flow_into_the_index() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("Home.md"), "[[Alice]]\n").unwrap();
        let seen = Arc::new(Mutex::new(Vec::<EngineEvent>::new()));
        let sink = seen.clone();
        let svc =
            VaultService::start_with_cache(d.path(), Cache::in_memory().unwrap(), move |ev| {
                sink.lock().unwrap().extend(ev)
            })
            .unwrap();
        assert!(svc.engine().unresolved(&p("Home.md")).contains_key("Alice"));

        // Another app creates the target note.
        fs::write(d.path().join("Alice.md"), "hello [[Home]]\n").unwrap();
        assert!(wait_for(&svc, |e| e
            .resolve_link("Alice", &p("Home.md"))
            .is_some()));
        assert!(wait_for(&svc, |e| e.backlinks(&p("Home.md")).len() == 1));

        // Another app edits it.
        std::thread::sleep(Duration::from_millis(30));
        fs::write(d.path().join("Alice.md"), "no links any more, just text\n").unwrap();
        assert!(wait_for(&svc, |e| e.backlinks(&p("Home.md")).is_empty()));

        // And deletes it.
        fs::remove_file(d.path().join("Alice.md")).unwrap();
        assert!(wait_for(&svc, |e| e
            .unresolved(&p("Home.md"))
            .contains_key("Alice")));

        assert!(seen
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, EngineEvent::LinksChanged { .. })));
    }

    #[test]
    fn own_writes_do_not_echo() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("A.md"), "a\n").unwrap();
        let seen = Arc::new(Mutex::new(Vec::<EngineEvent>::new()));
        let sink = seen.clone();
        let svc =
            VaultService::start_with_cache(d.path(), Cache::in_memory().unwrap(), move |ev| {
                sink.lock().unwrap().extend(ev)
            })
            .unwrap();
        svc.engine()
            .write_note(&p("A.md"), "a changed by the app\n")
            .unwrap();
        std::thread::sleep(DEBOUNCE * 4);
        // The watcher saw the write, reconciled it, found nothing new.
        let echoed = seen
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, EngineEvent::MetadataChanged { .. }));
        assert!(!echoed);
    }
}
