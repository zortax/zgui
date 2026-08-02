//! The semantic properties that are simply on or off.

use core::fmt::{self, Debug};
use core::ops::{BitAnd, BitOr, BitOrAssign, Not};

/// The semantic properties of an element that are either true or false.
///
/// These are the properties whose absence and whose falsehood mean the same thing, which is what
/// separates them from the three-valued ones: an element that has not said whether it is expanded
/// is *not expandable*, and that is a different statement from "collapsed".
///
/// ```
/// use zgui_vocab::SemanticFlags;
///
/// let flags = SemanticFlags::DISABLED | SemanticFlags::REQUIRED;
/// assert!(flags.contains(SemanticFlags::DISABLED));
/// assert!(!flags.contains(SemanticFlags::MODAL));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SemanticFlags(u16);

/// Declares one flag per property, and builds the table debug formatting reads.
macro_rules! flags {
    ($( $(#[$meta:meta])* $name:ident = $index:literal; )+) => {
        impl SemanticFlags {
            $(
                $(#[$meta])*
                pub const $name: Self = Self(1u16 << $index);
            )+
        }

        /// Every flag paired with the name it is written with.
        const NAMED: &[(SemanticFlags, &str)] = &[
            $( (SemanticFlags::$name, stringify!($name)), )+
        ];
    };
}

flags! {
    /// This element and everything inside it is absent from the presented tree.
    ///
    /// This is for content that is visually hidden and has no meaning to convey, not for content
    /// that is merely off screen.
    HIDDEN = 0;
    /// This control cannot be operated.
    DISABLED = 1;
    /// This control's value can be read but not changed.
    READ_ONLY = 2;
    /// This control must have a value before its form can be submitted.
    REQUIRED = 3;
    /// More than one of this container's items may be selected at once.
    MULTISELECTABLE = 4;
    /// While this element is shown, nothing outside it can be interacted with.
    MODAL = 5;
    /// This element's content is still being produced, so a consumer should wait before
    /// announcing it.
    BUSY = 6;
    /// A change anywhere inside this live region should be announced as a whole.
    LIVE_ATOMIC = 7;
    /// This element clips its children, so a consumer may skip the ones scrolled out of sight.
    ///
    /// Set this on every element whose overflow is not visible; without it a consumer walks the
    /// entire content of every scrolling region.
    CLIPS_CHILDREN = 8;
    /// Touching this element passes the touch through to what is behind it.
    TOUCH_TRANSPARENT = 9;
    /// This link has been followed before.
    VISITED = 10;
}

impl SemanticFlags {
    /// No property set.
    pub const NONE: Self = Self(0);

    /// The raw bits.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether every property in `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no property at all is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The same set with `other` set or cleared.
    pub const fn with(self, other: Self, on: bool) -> Self {
        if on {
            Self(self.0 | other.0)
        } else {
            Self(self.0 & !other.0)
        }
    }
}

impl BitOr for SemanticFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for SemanticFlags {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitAnd for SemanticFlags {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Not for SemanticFlags {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl Debug for SemanticFlags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SemanticFlags(")?;
        let mut written = false;
        for (flag, name) in NAMED {
            if self.contains(*flag) {
                if written {
                    formatter.write_str("|")?;
                }
                formatter.write_str(name)?;
                written = true;
            }
        }
        if !written {
            formatter.write_str("none")?;
        }
        formatter.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::{NAMED, SemanticFlags};

    #[test]
    fn every_flag_is_a_distinct_single_bit() {
        let mut seen = SemanticFlags::NONE;
        for (flag, name) in NAMED {
            assert_eq!(flag.bits().count_ones(), 1, "{name} is not a single bit");
            assert!(!seen.contains(*flag), "{name} repeats a bit");
            seen |= *flag;
        }
    }

    #[test]
    fn toggling_is_reversible() {
        let flags = SemanticFlags::NONE.with(SemanticFlags::BUSY, true);
        assert!(flags.contains(SemanticFlags::BUSY));
        assert!(flags.with(SemanticFlags::BUSY, false).is_empty());
    }

    #[test]
    fn debug_names_the_set_flags() {
        assert_eq!(format!("{:?}", SemanticFlags::NONE), "SemanticFlags(none)");
        assert_eq!(
            format!("{:?}", SemanticFlags::HIDDEN | SemanticFlags::MODAL),
            "SemanticFlags(HIDDEN|MODAL)"
        );
    }
}
