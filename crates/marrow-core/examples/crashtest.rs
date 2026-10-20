//! Crash test for saving.
//!
//! `crashtest run [n]` starts a copy of itself that saves one 4 MB note over
//! and over, kills it at a random moment, and checks the note: it must hold
//! one whole version, never a cut-off or mixed one. It does this `n` times
//! (default 200) for plain `fs::write` and for Marrow's crash-safe save.
//!
//!     cargo run --release --example crashtest -- run 200

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SIZE: usize = 4 << 20;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
            let dir = std::env::temp_dir().join(format!("marrow-crashtest-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            println!("Crash test: {n} forced crashes per method, 4 MB note, {}", std::env::consts::OS);
            let mut failed = false;
            for mode in ["plain", "atomic"] {
                let bad = run(&dir, mode, n);
                println!("  {mode:>6} save: {bad} damaged notes out of {n} crashes");
                failed |= mode == "atomic" && bad > 0;
            }
            let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
            println!("  files left in the test folder: {left:?}");
            let _ = std::fs::remove_dir_all(&dir);
            println!("{}", if failed { "RESULT: FAIL" } else { "RESULT: PASS" });
            std::process::exit(failed as i32);
        }
        Some(path) => save_forever(Path::new(path), args.get(1).map(String::as_str) == Some("plain")),
        None => eprintln!("usage: crashtest run [n]"),
    }
}

fn save_forever(path: &Path, plain: bool) {
    let a = "a".repeat(SIZE);
    let b = "b".repeat(SIZE);
    // Runs until the test kills it.
    let mut even = true;
    loop {
        let text = if even { &a } else { &b };
        even = !even;
        if plain {
            std::fs::write(path, text).unwrap();
        } else {
            marrow_core::atomic::write(path, text).unwrap();
        }
    }
}

fn run(dir: &Path, mode: &str, n: usize) -> usize {
    let note: PathBuf = dir.join(format!("Note-{mode}.md"));
    std::fs::write(&note, "a".repeat(SIZE)).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut bad = 0;
    for _ in 0..n {
        let mut child = Command::new(&exe).arg(&note).arg(mode).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(20 + random() % 230));
        let _ = child.kill();
        let _ = child.wait();
        let data = std::fs::read(&note).unwrap_or_default();
        let whole = data.len() == SIZE && (data.iter().all(|&c| c == b'a') || data.iter().all(|&c| c == b'b'));
        if !whole {
            bad += 1;
            // Start the next round from a good note.
            std::fs::write(&note, "a".repeat(SIZE)).unwrap();
        }
    }
    // Temporary files a killed save left behind are tidied by atomic::clean_up.
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if marrow_core::atomic::is_temp_name(&e.file_name().to_string_lossy()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    bad
}

fn random() -> u64 {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    // xorshift on the clock is plenty for picking a moment to crash.
    let mut x = t ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}
