//! Interaction state that participates in selector matching.

mod assert;
mod bits;
mod pairs;

#[cfg(test)]
mod tests;

use core::fmt::{self, Debug};
use core::ops::{BitAnd, BitOr, BitOrAssign, Not, Sub};

/// The set of interaction states an element is in.
///
/// These are the states a selector can ask about: `:hover`, `:checked`, `:disabled`, `:open` and
/// the rest. They are a set of bits rather than a struct of booleans because the style engine
/// tests them by mask — a rule that depends on `:hover` records one bit, and a state write
/// invalidates exactly the rules whose mask intersects the bits that changed.
///
/// ```
/// use zgui_vocab::UiState;
///
/// let state = UiState::HOVER | UiState::ENABLED;
/// assert!(state.contains(UiState::HOVER));
/// assert!(!state.contains(UiState::CHECKED));
/// assert!(state.intersects(UiState::HOVER | UiState::ACTIVE));
/// ```
///
/// # Who may write which bit
///
/// The set splits in two, and the split is a rule rather than a convention.
///
/// [`UiState::AUTHOR_SETTABLE`] holds the states a view may assert about its own element:
/// checked, disabled, open, indeterminate, placeholder-shown, read-only, required and invalid.
/// Each one mirrors a control property the author is the authority on.
///
/// Every other bit is computed by the framework. `:hover`, `:active`, `:focus`, `:focus-visible`
/// and `:focus-within` are written by input routing, and directionality, validity and media
/// states by the systems that own them. A view that could assert `:hover` would be lying to the
/// input system, so it cannot.
///
/// # Complementary pairs
///
/// Four pairs of bits are logical negations of one another — enabled/disabled,
/// read-write/read-only, valid/invalid, optional/required — and a set holding both, or neither, is
/// malformed. [`UiState::apply`] is the write that maintains them; a direct `|` does not.
///
/// ```
/// use zgui_vocab::UiState;
///
/// let enabled = UiState::ENABLED;
/// let disabled = enabled.apply(UiState::DISABLED, true);
/// assert!(disabled.contains(UiState::DISABLED));
/// assert!(!disabled.contains(UiState::ENABLED));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct UiState(u64);

impl UiState {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// The raw bits.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// The set with exactly these bits.
    ///
    /// Bits with no meaning here are kept rather than masked away, because the layout is shared
    /// with a style engine that may know states this vocabulary does not name.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Whether every state in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any state in `other` is present.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether no state at all is present.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The states present in this set and not in `other`.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The states present in exactly one of the two sets.
    ///
    /// This is what a state write hands the style engine: the bits that actually changed, which
    /// is what selects the rules to re-match.
    pub const fn symmetric_difference(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }
}

impl BitOr for UiState {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for UiState {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitAnd for UiState {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Sub for UiState {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self.difference(other)
    }
}

impl Not for UiState {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl Debug for UiState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UiState(")?;
        let mut written = false;
        for (bit, name) in bits::NAMED {
            if self.contains(*bit) {
                if written {
                    formatter.write_str("|")?;
                }
                formatter.write_str(name)?;
                written = true;
            }
        }
        if !written {
            formatter.write_str("empty")?;
        }
        formatter.write_str(")")
    }
}
