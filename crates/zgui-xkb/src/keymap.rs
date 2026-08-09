//! A compiled `xkb_keymap`, and the numbers a keyboard is described in.

use std::fmt;
use std::ptr::NonNull;
use std::slice;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::library::{Library, XkbKeymap};
use crate::state::State;

/// How far an xkb key code sits above the kernel's.
///
/// X11 numbered its key codes from eight, xkb kept that numbering, and evdev started again from
/// zero. Every code that crosses between them moves by this much. libxkbcommon writes the relation
/// as `keycode_A = KEY_A + 8`.
///
/// ```
/// use zgui_xkb::EVDEV_OFFSET;
///
/// assert_eq!(EVDEV_OFFSET, 8);
/// ```
pub const EVDEV_OFFSET: u32 = 8;

/// Which key moved, in the numbering libxkbcommon uses.
///
/// The offset lives in the constructors. A code from the kernel goes through
/// [`Keycode::from_evdev`], which applies it; a code that already carries it goes through
/// [`Keycode::from_raw`], which does not. Both are named for what they take, so a call site says
/// which numbering it holds. Neither checks: a kernel code handed to [`Keycode::from_raw`] names
/// the key eight positions earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Keycode(u32);

impl Keycode {
    /// Returns the xkb code for a kernel key code, such as `KEY_A`, which is 30.
    pub const fn from_evdev(code: u16) -> Self {
        Self(code as u32 + EVDEV_OFFSET)
    }

    /// Returns the code as libxkbcommon already numbers it.
    ///
    /// This is for a caller that reads codes out of a keymap or out of a protocol that carries xkb
    /// numbering. A code from the kernel goes through [`Keycode::from_evdev`].
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the kernel code this stands for.
    ///
    /// Answers nothing for the eight codes below the offset, which no kernel code reaches, and for
    /// a code above what the kernel numbers.
    pub const fn to_evdev(self) -> Option<u16> {
        match self.0.checked_sub(EVDEV_OFFSET) {
            Some(code) if code <= u16::MAX as u32 => Some(code as u16),
            _ => None,
        }
    }

    /// Returns the number libxkbcommon is given.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// What a key means: a character, a dead key, a modifier, a function key.
///
/// A keysym is a number in xkb's own table. A Latin-1 character is the code point itself, so `a` is
/// `0x0061`. A Unicode character from U+0100 to U+10FFFF is the code point plus `0x0100_0000`. The
/// rest are named symbols such as `Shift_L`, and [`crate::Context::keysym_name`] answers the name.
///
/// ```
/// use zgui_xkb::Keysym;
///
/// // Latin-1 is numbered by its own code points.
/// assert_eq!(Keysym::from_raw('a' as u32).raw(), 0x0061);
///
/// // Above U+00FF the offset is added, up to U+10FFFF.
/// let a_macron = Keysym::from_raw(0x0100_0000 + 'ā' as u32);
/// assert_eq!(a_macron.raw(), 0x0100_0101);
///
/// assert!(Keysym::NONE.is_none());
/// assert!(!a_macron.is_none());
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Keysym(u32);

impl Keysym {
    /// The keysym a key with nothing on it carries: `XKB_KEY_NoSymbol`.
    pub const NONE: Self = Self(0);

    /// Returns the keysym with this number.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the number libxkbcommon uses.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns `true` if this key carries nothing at all.
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }
}

/// Prints the number in hexadecimal, which is how the xkb tables are written.
impl fmt::Debug for Keysym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keysym({:#06x})", self.0)
    }
}

/// Which of a keymap's layouts a key is read in.
///
/// A keymap can hold several — `de,us` is two — and which one is active is state rather than
/// layout, so [`crate::State::layout`] is what answers it for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Layout(u32);

impl Layout {
    /// The first layout, which is the only one on a keyboard set to one.
    pub const FIRST: Self = Self(0);

    /// Returns the layout at this index.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the index libxkbcommon uses.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Which shift level of a key is read.
///
/// Level zero is the key with nothing held. Shift reaches level one on most keys, and the level
/// keys reach further. A level is a separate type from a [`Layout`] because the two are adjacent
/// arguments of the same call, and swapping them compiles.
///
/// ```
/// use zgui_xkb::Level;
///
/// assert_eq!(Level::UNMODIFIED.raw(), 0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(u32);

impl Level {
    /// The key with nothing held: the letter printed on it.
    pub const UNMODIFIED: Self = Self(0);

    /// Returns the level at this index.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the index libxkbcommon uses.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A compiled keymap: every key, every level, and every layout of one keyboard.
///
/// A keymap is read-only once it is compiled. What changes while somebody types is the [`State`]
/// beside it, and one keymap serves as many states as there are keyboards on the same layout.
///
/// The keymap holds a share of the loaded [`Library`] and takes its own reference on the context
/// it was compiled through, so the [`crate::Context`] may be dropped first.
///
/// ```no_run
/// use zgui_xkb::{Context, Keycode, Layout, RuleNames};
///
/// let keymap = Context::new()?.keymap(&RuleNames::default())?;
/// let key = Keycode::from_evdev(30);
///
/// println!("{:?}", keymap.unmodified_sym(key, Layout::FIRST));
/// println!("{}", keymap.key_repeats(key));
/// # Ok::<(), zgui_xkb::Error>(())
/// ```
#[derive(Debug)]
pub struct Keymap {
    /// The library every call goes through.
    library: Arc<Library>,
    /// The keymap itself.
    handle: NonNull<XkbKeymap>,
}

impl Keymap {
    /// Takes ownership of a compiled keymap.
    pub(crate) fn new(library: Arc<Library>, handle: NonNull<XkbKeymap>) -> Self {
        Self { library, handle }
    }

    /// Makes the state that is fed key transitions.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`] when the state cannot be built, which is an allocation failure
    /// and nothing else.
    pub fn state(&self) -> Result<State> {
        // SAFETY: the symbol is `xkb_state_new`. The keymap is live, and the state that comes back
        // is owned by the caller and takes its own reference on the keymap.
        let handle = unsafe { (self.library.symbols.state_new)(self.handle.as_ptr()) };
        let handle = NonNull::new(handle).ok_or(Error::Refused {
            what: "xkb_state_new",
        })?;
        Ok(State::new(Arc::clone(&self.library), handle))
    }

    /// Returns `true` if holding this key repeats it.
    ///
    /// Which keys repeat is the keymap's decision. A letter repeats and a modifier does not, so a
    /// caller that asks this keeps a held shift from filling a document.
    pub fn key_repeats(&self, key: Keycode) -> bool {
        // SAFETY: the symbol is `xkb_keymap_key_repeats`, which reads the keymap and answers one
        // or zero. A code the keymap has no key for answers zero.
        unsafe { (self.library.symbols.keymap_key_repeats)(self.handle.as_ptr(), key.raw()) == 1 }
    }

    /// Returns the keysyms a key carries at one level of one layout, with no state involved.
    ///
    /// This is what a shortcut is matched against. `Ctrl` and `A` held together produce a control
    /// character, so the state's own answer names no letter to match; the level the key is printed
    /// with does. [`Keymap::unmodified_sym`] is the one-symbol case.
    ///
    /// The slice belongs to the keymap. A key with nothing at that level answers an empty one.
    pub fn syms_at_level(&self, key: Keycode, layout: Layout, level: Level) -> &[Keysym] {
        let mut syms: *const u32 = std::ptr::null();
        // SAFETY: the symbol is `xkb_keymap_key_get_syms_by_level`. It writes a pointer to an array
        // the keymap owns into `syms` and answers how long the array is. A layout past the last one
        // is brought back into range by the library rather than refused.
        let count = unsafe {
            (self.library.symbols.keymap_key_get_syms_by_level)(
                self.handle.as_ptr(),
                key.raw(),
                layout.raw(),
                level.raw(),
                &raw mut syms,
            )
        };
        let Ok(count) = usize::try_from(count) else {
            return &[];
        };
        if count == 0 || syms.is_null() {
            // The library sets the pointer to null when it answers zero, and a slice may not be
            // built over null even with no elements in it.
            return &[];
        }
        // SAFETY: `syms` points at `count` keysyms held by the keymap, which lives at least as long
        // as the borrow this answer carries. `Keysym` is `#[repr(transparent)]` over the `u32` the
        // array holds, so the two have one layout.
        unsafe { slice::from_raw_parts(syms.cast::<Keysym>(), count) }
    }

    /// Returns the one keysym a key carries with nothing held.
    ///
    /// This is the symbol printed on the key: `XKB_KEY_a` for the key marked `A` on a Latin
    /// layout, whatever is held while it is pressed. A shortcut is written against it, so that
    /// `Ctrl+A` and `Shift+A` both find the same entry.
    pub fn unmodified_sym(&self, key: Keycode, layout: Layout) -> Option<Keysym> {
        self.syms_at_level(key, layout, Level::UNMODIFIED)
            .first()
            .copied()
    }
}

/// Gives the keymap back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for Keymap {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_keymap_unref`, and this is the reference taken by
        // `xkb_keymap_new_from_names`. Nothing here holds another, so it is dropped exactly once.
        unsafe { (self.library.symbols.keymap_unref)(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    //! The offset and the plain value types, over no library at all.

    use super::*;

    #[test]
    fn a_kernel_code_moves_up_by_eight() {
        // `KEY_A` is 30 in `input-event-codes.h`, and 38 in every xkb keymap.
        assert_eq!(Keycode::from_evdev(30).raw(), 38);
        assert_eq!(Keycode::from_evdev(0).raw(), 8);
    }

    #[test]
    fn a_code_that_moved_up_moves_back_down() {
        for code in [1_u16, 30, 42, 58, 700] {
            assert_eq!(Keycode::from_evdev(code).to_evdev(), Some(code));
        }
    }

    #[test]
    fn the_eight_codes_below_the_offset_belong_to_no_key() {
        // xkb numbers from eight, so codes zero to seven stand for nothing the kernel reports.
        // Answering `Some(0)` for them would put a key press on `KEY_RESERVED`.
        for raw in 0..EVDEV_OFFSET {
            assert_eq!(Keycode::from_raw(raw).to_evdev(), None);
        }
        assert_eq!(Keycode::from_raw(8).to_evdev(), Some(0));
    }

    #[test]
    fn a_code_above_what_the_kernel_numbers_belongs_to_no_key() {
        assert_eq!(Keycode::from_raw(u32::MAX).to_evdev(), None);
    }

    #[test]
    fn a_keysym_of_zero_is_no_symbol() {
        assert!(Keysym::NONE.is_none());
        assert!(Keysym::from_raw(0).is_none());
        assert!(!Keysym::from_raw(0x0061).is_none());
        assert_eq!(format!("{:?}", Keysym::from_raw(0x0061)), "Keysym(0x0061)");
    }
}
