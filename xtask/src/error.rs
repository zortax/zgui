//! The one error type every xtask entry point returns.

use std::path::{Path, PathBuf};

/// Anything that can go wrong while running a gate.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// A gate reported violations, or a required condition was not met.
    #[error("{0}")]
    Failed(String),

    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file the operation was attempted on.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// A manifest or lockfile could not be parsed.
    #[error("{path}: {source}")]
    Toml {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying failure.
        source: toml::de::Error,
    },
}

impl Error {
    /// Builds a [`Error::Failed`] from anything printable.
    pub(crate) fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }

    /// Attaches a path to an I/O failure.
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

/// The result type used throughout xtask.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Reads a file, attaching its path to any failure.
pub(crate) fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|source| Error::io(path, source))
}

/// Writes a file, creating parent directories, attaching its path to any failure.
pub(crate) fn write(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    std::fs::write(path, contents).map_err(|source| Error::io(path, source))
}
