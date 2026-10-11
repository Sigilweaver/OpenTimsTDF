use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by this crate.
///
/// New variants may be added in minor releases; match with a `_` arm.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// An `analysis.tdf` SQLite query failed.
    #[error("sqlite: {0}")]
    Sqlite(SqliteError),
    #[error("zstd: {0}")]
    Zstd(std::io::Error),
    #[error("unsupported codec: {0}")]
    UnsupportedCodec(String),
    #[error("bundle missing required file: {0}")]
    MissingFile(PathBuf),
    #[error("corrupt frame {0}: {1}")]
    CorruptFrame(u32, String),
    /// A `GlobalMetadata` value (or another bundle-level table) is missing
    /// or out of range where the reader needs it, for example the 1/K0 or
    /// m/z acquisition range used by the calibration.
    #[error("invalid metadata: {0}")]
    InvalidMetadata(String),
    #[error("internal sqlite connection lock poisoned by a prior panic")]
    LockPoisoned,
    /// Error from `openmassspec-core` (for example from the mzML writer).
    #[error(transparent)]
    Core(#[from] openmassspec_core::Error),
}

/// An `analysis.tdf` SQLite error.
///
/// Opaque so the SQLite binding this crate uses is not part of its public
/// API. `Display` gives the underlying message.
pub struct SqliteError(rusqlite::Error);

impl fmt::Debug for SqliteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for SqliteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for SqliteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// Convert `rusqlite` results into this crate's `Result` without a public
/// `From<rusqlite::Error>` impl.
pub(crate) trait SqlResultExt<T> {
    fn sql(self) -> Result<T>;
}

impl<T> SqlResultExt<T> for rusqlite::Result<T> {
    fn sql(self) -> Result<T> {
        self.map_err(|e| Error::Sqlite(SqliteError(e)))
    }
}

/// Reader errors become [`openmassspec_core::Error::Vendor`], except
/// [`Error::Core`], which is unwrapped.
impl From<Error> for openmassspec_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Core(inner) => inner,
            other => openmassspec_core::Error::vendor(other),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
