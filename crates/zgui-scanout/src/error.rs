//! What can refuse, and which step refused.

use std::fmt::{self, Display};

/// Why a copier could not be made, or could not copy.
///
/// Every variant names the step, because the steps fail for unrelated reasons on unrelated
/// machines and a caller's log line is the only place that difference is ever seen.
#[derive(Debug)]
pub enum Error {
    /// A library could not be opened at run time.
    Library {
        /// What was looked for.
        soname: &'static str,
        /// What the loader said.
        reason: String,
    },
    /// A symbol the crate calls is missing from a library that did open.
    Symbol {
        /// The name that did not resolve.
        name: &'static str,
    },
    /// The driver refused a step, and this is which one.
    Driver {
        /// What was being attempted, in words a log line can carry.
        step: &'static str,
        /// What the driver said, where it said anything.
        reason: String,
    },
    /// The device offers no way to copy a rectangle between two textures.
    ///
    /// The three ways are tried in turn — see [`egl`](crate::egl) — so this means a driver with
    /// none of them, which is a driver that cannot service this crate at all.
    NoCopy,
}

impl Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library { soname, reason } => {
                write!(formatter, "{soname} would not open: {reason}")
            }
            Self::Symbol { name } => write!(formatter, "the library has no {name}"),
            Self::Driver { step, reason } => write!(formatter, "{step}: {reason}"),
            Self::NoCopy => formatter.write_str(
                "this device can copy no rectangle between two textures, by any of the three ways \
                 this crate knows",
            ),
        }
    }
}

impl std::error::Error for Error {}
