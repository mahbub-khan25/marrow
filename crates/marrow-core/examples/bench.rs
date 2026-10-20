//! Speed on a generated vault: opening, searching, a database folder and a
//! rename with many backlinks. Prints a table; nothing is written to your
//! notes (the vault and cache go in a temporary folder).
//!
//!     cargo run --release --example bench -- 10000

use std::fs;
use std::time::Instant;

use marrow_core::cache::Cache;
use marrow_core::{Engine, VaultPath};

const WORDS: &[&str] = &[
    "garden", "soil", "seed", "plan", "river", "stone", "light", "paper", "market", "engine", "letter", "window",
    "summer", "harbour", "meeting", "budget", "design", "review", "travel", "recipe", "chapter", "orbit", "signal",
];

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10_000);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let mut seed = 42u64;
    let mut rnd = move |m: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % m as u64) as usize
    };
    let t = Instant::now();
    for i in 0..n {
        let folder = if i < n / 5 { "Tasks".to_string() } else { format!("Area {}", i % 40) };
        fs::create_dir_all(root.join(&folder)).unwrap();
        let mut body = format!(
            "---\nstatus: {}\ndue: 2026-{:02}-{:02}\ntags: [t{}]\n---\n# Note {i}\n\n",
            ["todo", "doing", "done"][i % 3],
            1 + i % 12,
            1 + i % 28,
            i % 50
        );
        for p in 0..6 {
            for _ in 0..50 {
                body.push_str(WORDS[rnd(WORDS.len())]);
                body.push(' ');
            }
            // A few links per note, plus everyone links to the hub.
            body.push_str(&format!("[[Note {}]] ", rnd(n)));
            if p == 0 {
                body.push_str("[[Hub]] ");
            }
            body.push_str("\n\n");
        }
        fs::write(root.join(folder).join(format!("Note {i}.md")), body).unwrap();
    }
    fs::write(root.join("Hub.md"), "# Hub\n").unwrap();
    let mb = walk_size(&root) as f64 / 1e6;
    println!("Generated {n} notes ({mb:.0} MB) in {:.1?}\n", t.elapsed());

    let cache = dir.path().join("cache.redb");
    let row = |what: &str, t: Instant| println!("  {what:<44} {:>9.1?}", t.elapsed());

    let t = Instant::now();
    let e = Engine::open(&root, Cache::open(&cache).unwrap()).unwrap();
    row("open, first time (reads and indexes all)", t);
    drop(e);
    let t = Instant::now();
    let mut e = Engine::open(&root, Cache::open(&cache).unwrap()).unwrap();
    row("open again (cache)", t);

    let t = Instant::now();
    let r = e.search("harbour signal", 50);
    row(&format!("first search ({} hits, loads text)", r.total), t);
    let t = Instant::now();
    let r = e.search("\"market engine\" -budget", 50);
    row(&format!("next search ({} hits)", r.total), t);

    let t = Instant::now();
    let q = e.query(&VaultPath::new("Tasks").unwrap(), false);
    row(&format!("database folder ({} rows)", q.rows.len()), t);

    let t = Instant::now();
    let b = e.backlinks(&VaultPath::new("Hub.md").unwrap()).len();
    row(&format!("backlinks of the hub ({b} notes)"), t);

    let t = Instant::now();
    let (_, rep) = e.rename_with_links(&VaultPath::new("Hub.md").unwrap(), &VaultPath::new("Home.md").unwrap()).unwrap();
    row(&format!("rename the hub ({} notes rewritten)", rep.files_updated.len()), t);

    let t = Instant::now();
    e.write_note(&VaultPath::new("Tasks/Note 1.md").unwrap(), "# Note 1\n\nedited [[Home]]\n").unwrap();
    row("save one note (crash-safe) and re-index", t);
}

fn walk_size(p: &std::path::Path) -> u64 {
    walkdir_size(p)
}

fn walkdir_size(p: &std::path::Path) -> u64 {
    let mut total = 0;
    for e in fs::read_dir(p).unwrap().flatten() {
        let m = e.metadata().unwrap();
        total += if m.is_dir() { walkdir_size(&e.path()) } else { m.len() };
    }
    total
}
