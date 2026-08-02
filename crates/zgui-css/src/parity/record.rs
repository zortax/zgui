//! One declaration: a longhand, how it is treated, and whether that is still true.

use core::fmt;

use crate::parity::engine::status_of;
use crate::parity::support::{Expectation, Support};

/// One longhand's treatment, declared beside the code that reads it.
///
/// The property is named by its Rust spelling — underscores where a style sheet writes hyphens —
/// because that is the spelling the declaring macro is handed and the one the engine's own
/// accessors use. [`Registration::css_name`] converts, so the two spellings never have to be
/// written down twice.
///
/// ```
/// use zgui_css::parity::{Registration, Support};
///
/// let row = Registration::new("border_top_left_radius", Support::Implemented("zgui-paint"));
/// assert_eq!(row.css_name(), "border-top-left-radius");
/// row.check().expect("a painted corner radius is a live longhand in this build");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Registration {
    /// The longhand's Rust spelling.
    ident: &'static str,
    /// How it is treated.
    support: Support,
}

impl Registration {
    /// Declares one longhand's treatment.
    pub const fn new(ident: &'static str, support: Support) -> Self {
        Self { ident, support }
    }

    /// The longhand's Rust spelling, which is how a declaration is keyed.
    pub const fn ident(&self) -> &'static str {
        self.ident
    }

    /// The longhand's spelling in a style sheet.
    ///
    /// Every underscore becomes a hyphen, which also gives vendor-prefixed properties their leading
    /// hyphen back:
    ///
    /// ```
    /// use zgui_css::parity::{Registration, Support, AbsentReason};
    ///
    /// let row = Registration::new("_moz_context_properties", Support::Absent(AbsentReason::GeckoOnly));
    /// assert_eq!(row.css_name(), "-moz-context-properties");
    /// ```
    pub fn css_name(&self) -> String {
        self.ident.replace('_', "-")
    }

    /// How the property is treated.
    pub const fn support(&self) -> Support {
        self.support
    }

    /// What this declaration implies the engine will say about the property.
    pub const fn expectation(&self) -> Expectation {
        match self.support {
            Support::Implemented(_) | Support::Ignored(_) => Expectation::Enabled,
            Support::Absent(reason) => reason.expected(),
        }
    }

    /// Checks the declaration against the engine as it is built and configured right now.
    ///
    /// A declaration is prose until something can prove it wrong. This is that something: a
    /// property claimed to be unavailable that the parser accepts, or one claimed to be consumed
    /// whose name the parser has never heard of, is a stale declaration and an error here.
    ///
    /// # Errors
    ///
    /// Returns [`ParityError::NotALonghand`] when the name is a shorthand or is not a property at
    /// all, and [`ParityError::Stale`] when the engine's answer contradicts the declaration.
    pub fn check(&self) -> Result<(), ParityError> {
        let css_name = self.css_name();
        let status = status_of(&css_name);
        let expected = self.expectation();
        match status.expectation() {
            None => Err(ParityError::NotALonghand { css_name }),
            Some(found) if found == expected => Ok(()),
            Some(found) => Err(ParityError::Stale {
                css_name,
                expected,
                found,
            }),
        }
    }
}

/// A declaration that does not match the engine it describes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParityError {
    /// The declared name is a shorthand, an alias or not a property, and the register holds
    /// longhands.
    NotALonghand {
        /// The name as a style sheet would write it.
        css_name: String,
    },
    /// The engine's answer contradicts the declaration.
    Stale {
        /// The name as a style sheet would write it.
        css_name: String,
        /// What the declaration implies.
        expected: Expectation,
        /// What the engine actually says.
        found: Expectation,
    },
}

impl fmt::Display for ParityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALonghand { css_name } => {
                write!(f, "`{css_name}` is not a longhand in this build")
            }
            Self::Stale {
                css_name,
                expected,
                found,
            } => write!(
                f,
                "`{css_name}` is declared as {expected:?} but this build reports {found:?}"
            ),
        }
    }
}

impl core::error::Error for ParityError {}
