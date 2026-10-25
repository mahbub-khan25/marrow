# Third-party software

Marrow is built on open-source software. Each keeps its own license; the full
license texts ship with the packages (see `Cargo.lock` and
`package-lock.json` for exact versions).

| Component | License |
|---|---|
| Tauri, Rust crates (redb, serde, notify and others) | MIT and/or Apache-2.0 |
| CodeMirror, Lezer, markdown-it, KaTeX, Mermaid | MIT |
| DOMPurify | MPL-2.0 or Apache-2.0 |
| d3-force, yaml | ISC |
| Phosphor Icons | MIT |
| EB Garamond and Noto Serif Bengali fonts (via Fontsource) | SIL Open Font License 1.1 |
| jsAvroPhonetic (Bangla phonetic typing), in `ui/src/vendor/avro-phonetic.js`, unchanged apart from an added export line | Mozilla Public License 1.1 (text in `ui/src/vendor/MPL-1.1.txt`); source: https://github.com/dipu-bd/jsAvroPhonetic |

Marrow is not affiliated with Obsidian and contains none of its code.
