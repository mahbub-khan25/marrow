//! Persistent, content-addressed metadata cache.
//!
//! Two tables, as in the design doc:
//!
//! | table   | key             | value                         |
//! |---------|-----------------|-------------------------------|
//! | `files` | vault path      | mtime, size, content hash     |
//! | `meta`  | content hash    | parsed [`NoteMeta`]           |
//!
//! A note whose mtime and size are unchanged is skipped without reading it.
//! A note whose bytes hash to something already in `meta` (a copy, a revert)
//! is never parsed again. The database lives in the app's data folder, not
//! in the vault.

use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::parse::NoteMeta;

const FILES: TableDefinition<&str, &[u8]> = TableDefinition::new("files");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

/// Bump when [`NoteMeta`] or the parser's output changes shape, so old
/// entries are discarded instead of misread.
pub const SCHEMA_VERSION: u32 = 1;
const VERSION: TableDefinition<&str, u32> = TableDefinition::new("version");

/// Hex BLAKE3 of a file's bytes.
pub type ContentHash = String;

pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    blake3::hash(bytes).to_hex().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub mtime_ms: i64,
    pub size: u64,
    pub hash: ContentHash,
}

/// A write to apply in one transaction.
pub enum CacheOp {
    PutFile(String, FileRecord),
    DeleteFile(String),
    PutMeta(ContentHash, NoteMeta),
    DeleteMeta(ContentHash),
}

pub struct Cache {
    db: Database,
}

fn ce<E: std::fmt::Display>(e: E) -> Error {
    Error::Cache(e.to_string())
}

impl Cache {
    /// Opens (or creates) the cache file at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(ce)?;
        }
        let db = Database::create(path).map_err(ce)?;
        let c = Cache { db };
        c.check_version()?;
        Ok(c)
    }

    /// A cache that lives only in memory (tests, read-only vaults).
    pub fn in_memory() -> Result<Self> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(ce)?;
        let c = Cache { db };
        c.check_version()?;
        Ok(c)
    }

    fn check_version(&self) -> Result<()> {
        let txn = self.db.begin_write().map_err(ce)?;
        {
            let v = txn.open_table(VERSION).map_err(ce)?;
            let current = v.get("schema").map_err(ce)?.map(|g| g.value());
            if current != Some(SCHEMA_VERSION) {
                drop(v);
                // Old or missing schema: start clean.
                let _ = txn.delete_table(FILES);
                let _ = txn.delete_table(META);
                let mut v = txn.open_table(VERSION).map_err(ce)?;
                v.insert("schema", SCHEMA_VERSION).map_err(ce)?;
            }
            txn.open_table(FILES).map_err(ce)?;
            txn.open_table(META).map_err(ce)?;
        }
        txn.commit().map_err(ce)
    }

    pub fn file(&self, path: &str) -> Result<Option<FileRecord>> {
        let txn = self.db.begin_read().map_err(ce)?;
        let t = txn.open_table(FILES).map_err(ce)?;
        match t.get(path).map_err(ce)? {
            Some(v) => Ok(serde_json::from_slice(v.value()).ok()),
            None => Ok(None),
        }
    }

    pub fn meta(&self, hash: &str) -> Result<Option<NoteMeta>> {
        let txn = self.db.begin_read().map_err(ce)?;
        let t = txn.open_table(META).map_err(ce)?;
        match t.get(hash).map_err(ce)? {
            Some(v) => Ok(serde_json::from_slice(v.value()).ok()),
            None => Ok(None),
        }
    }

    pub fn has_meta(&self, hash: &str) -> Result<bool> {
        let txn = self.db.begin_read().map_err(ce)?;
        let t = txn.open_table(META).map_err(ce)?;
        Ok(t.get(hash).map_err(ce)?.is_some())
    }

    /// Every stored file record, for start-up comparison against the vault.
    pub fn all_files(&self) -> Result<Vec<(String, FileRecord)>> {
        let txn = self.db.begin_read().map_err(ce)?;
        let t = txn.open_table(FILES).map_err(ce)?;
        let mut out = Vec::new();
        for item in t.iter().map_err(ce)? {
            let (k, v) = item.map_err(ce)?;
            if let Ok(rec) = serde_json::from_slice(v.value()) {
                out.push((k.value().to_string(), rec));
            }
        }
        Ok(out)
    }

    /// Applies writes in one transaction (one fsync for a whole batch).
    pub fn apply(&self, ops: Vec<CacheOp>) -> Result<()> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut txn = self.db.begin_write().map_err(ce)?;
        // The cache can always be rebuilt from the notes, and every entry is
        // checked against the file before use, so it needn't wait for the
        // disk on each save. redb still keeps it consistent after a crash.
        txn.set_durability(redb::Durability::Eventual);
        {
            let mut files = txn.open_table(FILES).map_err(ce)?;
            let mut meta = txn.open_table(META).map_err(ce)?;
            for op in ops {
                match op {
                    CacheOp::PutFile(p, rec) => {
                        let bytes = serde_json::to_vec(&rec).map_err(ce)?;
                        files.insert(p.as_str(), bytes.as_slice()).map_err(ce)?;
                    }
                    CacheOp::DeleteFile(p) => {
                        files.remove(p.as_str()).map_err(ce)?;
                    }
                    CacheOp::PutMeta(h, m) => {
                        let bytes = serde_json::to_vec(&m).map_err(ce)?;
                        meta.insert(h.as_str(), bytes.as_slice()).map_err(ce)?;
                    }
                    CacheOp::DeleteMeta(h) => {
                        meta.remove(h.as_str()).map_err(ce)?;
                    }
                }
            }
        }
        txn.commit().map_err(ce)
    }

    /// Drops metadata no file points to any more. Returns how many went.
    pub fn collect_garbage(&self) -> Result<usize> {
        let live: std::collections::HashSet<String> =
            self.all_files()?.into_iter().map(|(_, r)| r.hash).collect();
        let txn = self.db.begin_read().map_err(ce)?;
        let t = txn.open_table(META).map_err(ce)?;
        let mut dead = Vec::new();
        for item in t.iter().map_err(ce)? {
            let (k, _) = item.map_err(ce)?;
            if !live.contains(k.value()) {
                dead.push(k.value().to_string());
            }
        }
        drop(t);
        drop(txn);
        let n = dead.len();
        self.apply(dead.into_iter().map(CacheOp::DeleteMeta).collect())?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn roundtrip_and_gc() {
        let c = Cache::in_memory().unwrap();
        let m = parse("[[A]] #t\n");
        let h = hash_bytes(b"[[A]] #t\n");
        c.apply(vec![
            CacheOp::PutFile(
                "a.md".into(),
                FileRecord {
                    mtime_ms: 1,
                    size: 9,
                    hash: h.clone(),
                },
            ),
            CacheOp::PutMeta(h.clone(), m.clone()),
            CacheOp::PutMeta("orphan".into(), NoteMeta::default()),
        ])
        .unwrap();
        assert_eq!(c.file("a.md").unwrap().unwrap().hash, h);
        assert_eq!(c.meta(&h).unwrap().unwrap(), m);
        assert_eq!(c.collect_garbage().unwrap(), 1);
        assert!(c.has_meta(&h).unwrap());
        assert!(!c.has_meta("orphan").unwrap());
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.redb");
        {
            let c = Cache::open(&path).unwrap();
            c.apply(vec![CacheOp::PutFile(
                "x.md".into(),
                FileRecord {
                    mtime_ms: 5,
                    size: 1,
                    hash: "h".into(),
                },
            )])
            .unwrap();
        }
        let c = Cache::open(&path).unwrap();
        assert_eq!(c.file("x.md").unwrap().unwrap().mtime_ms, 5);
    }
}
