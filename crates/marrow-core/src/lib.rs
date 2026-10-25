//! Marrow core: the vault, index and link engine.
//!
//! This crate has no UI code. The Tauri shell wraps a
//! [`service::VaultService`] and forwards its [`engine::EngineEvent`]s to the
//! window.
//!
//! | module    | job                                                        |
//! |-----------|------------------------------------------------------------|
//! | `atomic`  | crash-safe file writes (temporary file, then replace)      |
//! | `path`    | canonical vault-relative paths                             |
//! | `vault`   | in-memory file tree, reconcile against disk, file ops      |
//! | `parse`   | Markdown → links, embeds, tags, headings, blocks, YAML      |
//! | `cache`   | redb cache: path → (mtime, size, hash), hash → metadata    |
//! | `links`   | filename index, link resolution, link graph, backlinks     |
//! | `engine`  | ties the above together; one owner for all state           |
//! | `service` | engine + debounced file watcher on a background thread     |
//! | `merge`   | three-way merge for outside edits to an open note          |
//! | `backup`  | text kept when a save fails, offered back later            |
//! | `cloud`   | where Dropbox, OneDrive, Google Drive keep their folders    |
//! | `mirror`  | two-way sync between the vault and another folder          |
//! | `conflicts` | finding the conflict copies sync apps make               |
//! | `history` | earlier versions of each note, thinned out over time       |
//! | `import`  | Notion, Evernote, Word/OneNote and web pages into notes      |
//! | `vectors` | embeddings of note chunks, for search by meaning             |
//! | `lock`    | password-locked notes (Argon2id + XChaCha20-Poly1305)        |

pub mod atomic;
pub mod backup;
pub mod cache;
pub mod cloud;
pub mod conflicts;
pub mod engine;
pub mod error;
pub mod history;
pub mod import;
pub mod links;
pub mod lock;
pub mod merge;
pub mod mirror;
pub mod parse;
pub mod path;
pub mod sealed;
pub mod service;
pub mod vault;
pub mod vectors;

pub use backup::{Backup, BackupStore};
pub use engine::{
    Engine, EngineEvent, GraphData, GraphOptions, IndexStats, NoteInfo, NoteRow, QueryResult, RecentFile, RenameReport, SearchHit,
    SearchResults, Snippet, TreeNode, VaultStats,
};
pub use error::{Error, Result};
pub use links::{Backlink, LinkFormat};
pub use merge::{merge2, merge3, MergeOutcome};
pub use parse::NoteMeta;
pub use path::VaultPath;
pub use service::VaultService;
pub use vault::{Entry, EntryKind, FileStat, Vault, VaultEvent};
