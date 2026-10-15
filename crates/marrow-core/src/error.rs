use std::path::PathBuf;

/// Errors raised by the core engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid vault path: {0}")]
    InvalidPath(String),
    #[error("path is outside the vault: {0}")]
    OutsideVault(PathBuf),
    #[error("not found in vault: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cache error: {0}")]
    Cache(String),
    #[error("watcher error: {0}")]
    Watch(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.into(),
            source,
        })
    }
}
