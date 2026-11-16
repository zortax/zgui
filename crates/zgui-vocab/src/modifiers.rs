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
/// Two more bits are the lock keys: [`Modifiers::CAPS_LOCK`] and [`Modifiers::NUM_LOCK`]. A lock
/// is a latched state, and no key is held for it. A platform reports the locks only with the
/// change of the modifier state. An input event carries the four held modifiers alone, so a
/// shortcut matches whatever the locks are. [`Modifiers::held`] takes the locks off a set.
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
    /// Caps lock is on.
    ///
    /// A latched state. A password field reads it to warn that the letters typed are upper case.
    pub const CAPS_LOCK: Self = Self(1 << 4);
    /// Num lock is on.
    pub const NUM_LOCK: Self = Self(1 << 5);
    /// Both lock keys.
    pub const LOCKS: Self = Self(Self::CAPS_LOCK.0 | Self::NUM_LOCK.0);

    /// The modifier this platform's shortcuts use: command on macOS, control elsewhere.
    ///
    /// ```
    /// use zgui_vocab::Modifiers;
    ///
    /// let expected = if cfg!(target_os = "macos") { Modifiers::META } else { Modifiers::CONTROL };
    /// assert_eq!(Modifiers::PRIMARY, expected);
    /// ```
    pub const PRIMARY: Self = if cfg!(target_os = "macos") {
        Self::META
    } else {
        Self::CONTROL
    };

    /// Every held modifier at once: shift, control, alt and meta. The locks are not in it.
    pub const ALL: Self = Self(0b1111);

    /// Every bit that is defined: the held modifiers and the locks.
    const DEFINED: Self = Self(Self::ALL.0 | Self::LOCKS.0);

    /// The raw bits.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The set with exactly these bits, ignoring any bit that is not defined.
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::DEFINED.0)
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

    /// Whether this platform's shortcut modifier is held.
    pub const fn primary(self) -> bool {
        self.contains(Self::PRIMARY)
    }

    /// Whether caps lock is on.
    pub const fn caps_lock(self) -> bool {
        self.contains(Self::CAPS_LOCK)
    }

    /// Whether num lock is on.
    pub const fn num_lock(self) -> bool {
        self.contains(Self::NUM_LOCK)
    }

    /// The held modifiers of this set, without the locks.
    pub const fn held(self) -> Self {
        Self(self.0 & Self::ALL.0)
    }

    /// The locks of this set, without the held modifiers.
    pub const fn locks(self) -> Self {
        Self(self.0 & Self::LOCKS.0)
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
        Self(!self.0 & Self::DEFINED.0)
    }
}

impl Debug for Modifiers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (Self::CONTROL, "control"),
            (Self::ALT, "alt"),
            (Self::SHIFT, "shift"),
            (Self::META, "meta"),
            (Self::CAPS_LOCK, "caps-lock"),
            (Self::NUM_LOCK, "num-lock"),
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
        let every = Modifiers::ALL | Modifiers::LOCKS;
        assert_eq!(Modifiers::from_bits_truncate(0xff), every);
        assert_eq!((!Modifiers::NONE).bits(), every.bits());
    }

    #[test]
    fn the_locks_are_apart_from_the_held_modifiers() {
        let state = Modifiers::CONTROL | Modifiers::CAPS_LOCK;
        assert!(state.caps_lock());
        assert!(!state.num_lock());
        assert_eq!(state.held(), Modifiers::CONTROL);
        assert_eq!(state.locks(), Modifiers::CAPS_LOCK);
        assert!(!Modifiers::ALL.contains(Modifiers::CAPS_LOCK));
        assert_eq!(
            format!("{:?}", Modifiers::SHIFT | Modifiers::NUM_LOCK),
            "Modifiers(shift+num-lock)"
        );
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
