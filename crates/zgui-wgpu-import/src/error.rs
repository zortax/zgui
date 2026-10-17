//! Why a frame could not be imported.

/// Why a frame could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The device is on a backend this importer does not serve.
    Backend,
    /// The device lacks a feature or an extension the import needs.
    Missing(&'static str),
    /// The frame's pixel format is one this importer does not read.
    Format(String),
    /// The platform refused the import.
    Platform(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend => f.write_str("the device is on a backend this importer does not serve"),
            Self::Missing(what) => write!(f, "the device lacks {what}"),
            Self::Format(format) => write!(f, "frames in {format} cannot be imported"),
            Self::Platform(reason) => write!(f, "the platform refused the import: {reason}"),
        }
    }
}

impl std::error::Error for ImportError {}
