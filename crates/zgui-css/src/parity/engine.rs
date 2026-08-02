//! What the style engine, as built and configured right now, says about a property name.
//!
//! Every declaration in the register is a claim, and this is where a claim can be checked against
//! the thing it is about. Three answers are possible and they are not interchangeable: a name the
//! parser does not know is a declaration silently discarded, a name it knows but has switched off
//! is the same silence with a different fix, and a name it knows and accepts is a property whose
//! absence from the rendered result is this framework's own doing.

use style::properties::PropertyId;

use crate::parity::support::Expectation;

/// What the engine knows about one property name.
///
/// ```
/// use zgui_css::parity::{EngineStatus, status_of};
///
/// // A longhand every build has.
/// assert!(matches!(status_of("color"), EngineStatus::Longhand { .. }));
/// // A shorthand is not a longhand, and the register is a register of longhands.
/// assert_eq!(status_of("margin"), EngineStatus::Shorthand);
/// // A name this build was not generated with.
/// assert_eq!(status_of("fill-opacity"), EngineStatus::Unknown);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EngineStatus {
    /// A longhand, and whether a style sheet may currently use it.
    ///
    /// `enabled` is read at *parse* time, so a sheet parsed while it is `false` loses those
    /// declarations without reporting anything.
    Longhand {
        /// Whether the property is switched on in the engine's current configuration.
        enabled: bool,
    },
    /// A shorthand, an alias for one, or a custom property — not a longhand.
    Shorthand,
    /// A name the parser does not recognise, so a declaration using it is discarded.
    Unknown,
}

impl EngineStatus {
    /// The claim this status corresponds to, when the name is a longhand.
    ///
    /// A shorthand has no answer here, because a register row naming one is malformed rather than
    /// merely stale.
    pub const fn expectation(self) -> Option<Expectation> {
        Some(match self {
            Self::Longhand { enabled: true } => Expectation::Enabled,
            Self::Longhand { enabled: false } => Expectation::KnownButDisabled,
            Self::Unknown => Expectation::Unknown,
            Self::Shorthand => return None,
        })
    }
}

/// Asks the engine about one CSS property name, spelled as it is written in a style sheet.
///
/// The name is looked up twice: once ignoring the engine's configuration, which answers whether the
/// property was generated at all, and once honouring it, which answers whether a style sheet may
/// use it right now. Those are different questions with different fixes, and collapsing them is
/// what makes a parity report claim a property is missing when a preference is merely off.
pub fn status_of(css_name: &str) -> EngineStatus {
    let Ok(id) = PropertyId::parse_unchecked(css_name, None) else {
        return EngineStatus::Unknown;
    };
    if id.longhand_id().is_none() {
        return EngineStatus::Shorthand;
    }
    EngineStatus::Longhand {
        enabled: PropertyId::parse_enabled_for_all_content(css_name).is_ok(),
    }
}
