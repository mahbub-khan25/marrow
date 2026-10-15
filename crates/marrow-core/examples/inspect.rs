//! Point the engine at any folder of Markdown (an Obsidian vault works) and
//! print what it sees. Useful for checking Phases 1–4 on real notes before
//! the UI exists.
//!
//!     cargo run --release --example inspect -- "D:\Notes"
//!     cargo run --release --example inspect -- "D:\Notes" "Projects/Idea.md"
//!     cargo run --release --example inspect -- "D:\Notes" --watch
//!     cargo run --release --example inspect -- "D:\Notes" --search "query"
//!
//! The cache is written to your temp folder, never into the vault.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use marrow_core::cache::Cache;
use marrow_core::{Engine, EngineEvent, VaultPath, VaultService};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: inspect <vault folder> [note path | --watch]");
        std::process::exit(2);
    };
    let second = args.next();
    let cache_path = std::env::temp_dir()
        .join("marrow-inspect")
        .join(cache_name(&root));

    if second.as_deref() == Some("--watch") {
        return watch(&root, &cache_path);
    }
    if second.as_deref() == Some("--search") {
        let query = args.next().unwrap_or_default();
        let mut engine =
            Engine::open(&root, Cache::open(&cache_path).expect("open cache")).expect("open vault");
        for round in ["first search (loads note text)", "second search"] {
            let t = Instant::now();
            let r = engine.search(&query, 20);
            println!("{round}: {} matching notes in {:.1?}", r.total, t.elapsed());
            if round == "second search" {
                for h in r.hits.iter().take(5) {
                    println!("  {} ({} matches)", h.path, h.matches);
                    if let Some(s) = h.snippets.first() {
                        println!(
                            "    line {}: {}",
                            s.line + 1,
                            s.text.chars().take(100).collect::<String>()
                        );
                    }
                }
            }
        }
        return;
    }

    let t = Instant::now();
    let engine =
        Engine::open(&root, Cache::open(&cache_path).expect("open cache")).expect("open vault");
    let took = t.elapsed();
    let s = engine.stats();
    let notes = engine.vault().markdown_files().count();
    let files = engine.vault().files().count();
    println!("Opened {} in {:.0?}", root.display(), took);
    println!("  {files} files, {notes} notes");
    println!(
        "  index: {} unchanged (not read), {} hash reused, {} parsed",
        s.unchanged, s.reused, s.parsed
    );
    println!("  run it again: the second open should parse 0 notes.\n");

    let unresolved = engine.graph().all_unresolved();
    println!("Unresolved links: {} distinct", unresolved.len());
    let mut top: Vec<_> = unresolved.into_iter().collect();
    top.sort_by_key(|b| std::cmp::Reverse(b.1));
    for (link, n) in top.iter().take(10) {
        println!("  {n:>4}  [[{link}]]");
    }

    let tags = engine.tags();
    let mut top_tags: Vec<_> = tags.into_iter().collect();
    top_tags.sort_by_key(|b| std::cmp::Reverse(b.1));
    println!("\nTop tags:");
    for (t, n) in top_tags.iter().take(10) {
        println!("  {n:>4}  {t}");
    }

    let note = match second {
        Some(n) => VaultPath::new(&n).expect("note path"),
        None => match most_linked(&engine) {
            Some(p) => p,
            None => return,
        },
    };
    println!("\nNote: {note}");
    if let Some(m) = engine.meta(&note) {
        println!(
            "  {} links, {} embeds, {} tags, {} headings, {} blocks, {} tasks",
            m.links.len(),
            m.embeds.len(),
            m.tags.len(),
            m.headings.len(),
            m.blocks.len(),
            m.list_items.iter().filter(|l| l.task.is_some()).count()
        );
    }
    println!("  links out to:");
    for (p, n) in engine.outgoing(&note).iter().take(15) {
        println!("    {n}× {p}");
    }
    let bl = engine.backlinks(&note);
    println!("  backlinks from {} notes:", bl.len());
    for b in bl.iter().take(15) {
        let r = &b.refs[0];
        println!(
            "    {} (line {}): {}",
            b.source,
            r.pos.start.line + 1,
            r.original
        );
    }
}

fn most_linked(e: &Engine) -> Option<VaultPath> {
    let mut counts: std::collections::HashMap<&VaultPath, u32> = Default::default();
    for targets in e.graph().resolved.values() {
        for (t, n) in targets {
            *counts.entry(t).or_default() += n;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(p, _)| p.clone())
}

fn watch(root: &std::path::Path, cache: &std::path::Path) {
    let svc = VaultService::start(root, cache, |events| {
        for e in events {
            match e {
                EngineEvent::Vault { event } => println!("vault   {event:?}"),
                EngineEvent::MetadataChanged { path } => println!("reindex {path}"),
                EngineEvent::LinksChanged { paths } => println!("relink  {} notes", paths.len()),
            }
        }
    })
    .expect("start");
    println!(
        "Watching {} ({} notes). Edit, create or delete notes in another app; Ctrl+C to stop.",
        root.display(),
        svc.engine().vault().markdown_files().count()
    );
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn cache_name(root: &std::path::Path) -> String {
    let h = marrow_core::cache::hash_bytes(root.to_string_lossy().as_bytes());
    format!("{}.redb", &h[..16])
}
