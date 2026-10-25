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
| Ekushe Next typing engine (Bangla typing): `ui/public/ekushe/` (ekushe-core built for the web, with its 30,000-word dictionary) and `ui/src/vendor/ekushe.js` | Mozilla Public License 2.0 (text in `ui/public/ekushe/LICENSE-MPL-2.0.txt`); source: https://github.com/mahbub-khan25/ekushenext and https://github.com/mahbub-khan25/ekushenext-website (`engine/`). The dictionary is CC BY-SA 4.0 (`ui/public/ekushe/DICTIONARY-LICENSE.md`). To update: `scripts/update-ekushe.sh`. |

Marrow is not affiliated with Obsidian and contains none of its code.
