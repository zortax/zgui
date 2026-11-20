//! Why the factory gave no implementation.

use core::fmt;

/// Why the factory gave no implementation, as a message a test prints when it skips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable(String);

impl Unavailable {
    /// An implementation that `variable=value` selects and that is not written yet.
    pub(crate) fn not_written(variable: &str, value: &str) -> Self {
        Self(format!(
            "{variable}={value} selects an implementation that does not exist yet; \
             unset {variable} or choose another value"
        ))
    }

    /// A value of `variable` that names no implementation.
    pub(crate) fn unknown(variable: &str, value: &str, known: &str) -> Self {
        Self(format!(
            "{variable}={value} names no implementation; the known values are {known}"
        ))
    }

    /// A device that could not be opened.
    pub(crate) fn device(error: &dyn fmt::Display) -> Self {
        Self(format!(
            "ZGUI_RENDERER=wgpu found no usable device: {error}"
        ))
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unavailable {}
