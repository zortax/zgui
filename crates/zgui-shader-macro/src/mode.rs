//! The shading modes and reads a declaration can name.

use iceblink::ShaderMode;

/// What a declared shader does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Paint,
    Coverage,
    Filter,
}

impl Mode {
    pub(crate) const ALL: [Self; 3] = [Self::Paint, Self::Coverage, Self::Filter];

    /// Returns the name a declaration writes the mode as.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Paint => "Paint",
            Self::Coverage => "Coverage",
            Self::Filter => "Filter",
        }
    }

    /// Returns the mode written as `name`, or `None` for no mode.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }

    /// Returns the compositor's shader mode.
    pub(crate) fn compositor(self) -> ShaderMode {
        match self {
            Self::Paint => ShaderMode::PAINT,
            Self::Coverage => ShaderMode::COVERAGE,
            Self::Filter => ShaderMode::FILTER,
        }
    }
}

/// What a declared shader reads that changes on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reads {
    pub(crate) time: bool,
    pub(crate) pointer: bool,
}

impl Reads {
    /// The names a declaration writes the reads as.
    pub(crate) const NAMES: [&'static str; 2] = ["Time", "Pointer"];

    /// Returns these reads and the one written as `name`, or `None` for no read.
    pub(crate) fn with(mut self, name: &str) -> Option<Self> {
        match name {
            "Time" => self.time = true,
            "Pointer" => self.pointer = true,
            _ => return None,
        }
        Some(self)
    }
}
