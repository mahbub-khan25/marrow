//! Saves one file over and over, alternating two versions, until killed.
//! Used by scripts/crash-test.py to check that a save interrupted at any
//! moment leaves either the old version or the new one, never a mix.
//!
//!     cargo run --release --example crashtest -- <file> atomic|plain

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("file"));
    let plain = args.next().as_deref() == Some("plain");
    let a = "a".repeat(4 << 20);
    let b = "b".repeat(4 << 20);
    for i in 0u64.. {
        let text = if i % 2 == 0 { &a } else { &b };
        if plain {
            std::fs::write(&path, text).unwrap();
        } else {
            marrow_core::atomic::write(&path, text).unwrap();
        }
    }
}
