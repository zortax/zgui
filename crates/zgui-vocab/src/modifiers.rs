//! The modifier keys held down while an event happened.

use core::fmt::{self, Debug};
use core::ops::{BitAnd, BitOr, BitOrAssign, Not};

/// The set of modifier keys held when an event was produced.
///
/// Four modifiers, one bit each, because that is the set every desktop and every browser agrees
/// on. The physical keys behind them differ per platform — the meta bit is Super on Linux, Command
/// on macOS and Windows on Windows — so a shortcut is written against the *modifier* and the
/// platform decides which key produces it.
///
/// ```
/// use zgui_vocab::Modifiers;
///
/// let chord = Modifiers::CONTROL | Modifiers::SHIFT;
/// assert!(chord.control());
/// assert!(chord.contains(Modifiers::SHIFT));
/// assert!(!chord.alt());
/// // A chord test is exact: control-shift is not control.
/// assert_ne!(chord, Modifiers::CONTROL);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifier held.
    pub const NONE: Self = Self(0);
    /// Either shift key.
    pub const SHIFT: Self = Self(1 << 0);
    /// Either control key.
    pub const CONTROL: Self = Self(1 << 1);
    /// Either alt key, called option on macOS.
    pub const ALT: Self = Self(1 << 2);
    /// The platform's command modifier: Super, Command or the Windows key.
    pub const META: Self = Self(1 << 3);

    /// Every modifier at once, which is also the mask of the bits that are defined.
    pub const ALL: Self = Self(0b1111);

    /// The raw bits.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The set with exactly these bits, ignoring any bit outside [`Modifiers::ALL`].
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::ALL.0)
    }

    /// Whether every modifier in `other` is held.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no modifier at all is held.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether shift is held.
    pub const fn shift(self) -> bool {
        self.contains(Self::SHIFT)
    }

    /// Whether control is held.
    pub const fn control(self) -> bool {
        self.contains(Self::CONTROL)
    }

    /// Whether alt is held.
    pub const fn alt(self) -> bool {
        self.contains(Self::ALT)
    }

    /// Whether the platform's command modifier is held.
    pub const fn meta(self) -> bool {
        self.contains(Self::META)
    }

    /// The same set with `other` added or removed.
    pub const fn with(self, other: Self, on: bool) -> Self {
        if on {
            Self(self.0 | other.0)
        } else {
            Self(self.0 & !other.0)
        }
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitAnd for Modifiers {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Not for Modifiers {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0 & Self::ALL.0)
    }
}

impl Debug for Modifiers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (Self::CONTROL, "control"),
            (Self::ALT, "alt"),
            (Self::SHIFT, "shift"),
            (Self::META, "meta"),
        ];
        formatter.write_str("Modifiers(")?;
        let mut written = false;
        for (bit, name) in names {
            if self.contains(bit) {
                if written {
                    formatter.write_str("+")?;
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
    use super::Modifiers;

    #[test]
    fn undefined_bits_never_enter_the_set() {
        assert_eq!(Modifiers::from_bits_truncate(0xff), Modifiers::ALL);
        assert_eq!((!Modifiers::NONE).bits(), Modifiers::ALL.bits());
    }

    #[test]
    fn containment_is_a_subset_test_not_an_equality_test() {
        let chord = Modifiers::CONTROL | Modifiers::SHIFT;
        assert!(chord.contains(Modifiers::CONTROL));
        assert!(chord.contains(chord));
        assert!(!Modifiers::CONTROL.contains(chord));
    }

    #[test]
    fn toggling_is_reversible() {
        let held = Modifiers::NONE.with(Modifiers::ALT, true);
        assert!(held.alt());
        assert!(held.with(Modifiers::ALT, false).is_empty());
    }

    #[test]
    fn debug_names_the_held_modifiers() {
        assert_eq!(format!("{:?}", Modifiers::NONE), "Modifiers(none)");
        assert_eq!(
            format!("{:?}", Modifiers::CONTROL | Modifiers::SHIFT),
            "Modifiers(control+shift)"
        );
    }
}
