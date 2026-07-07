use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("zstd: {0}")]
    Zstd(std::io::Error),
    #[error("unsupported codec: {0}")]
    UnsupportedCodec(String),
    #[error("bundle missing required file: {0}")]
    MissingFile(PathBuf),
    #[error("corrupt frame {0}: {1}")]
    CorruptFrame(u32, String),
    #[error("internal sqlite connection lock poisoned by a prior panic")]
    LockPoisoned,
}

pub type Result<T> = std::result::Result<T, Error>;
