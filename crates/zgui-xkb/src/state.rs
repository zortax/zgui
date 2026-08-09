//! An `xkb_state`: fed key transitions, asked for text and modifiers.

use std::ffi::{CStr, c_uint};
use std::ptr::NonNull;
use std::sync::Arc;

use crate::keymap::{Keycode, Keysym, Layout};
use crate::library::{Library, XkbState, read_text};

/// `XKB_KEY_UP`.
const UP: c_uint = 0;

/// `XKB_KEY_DOWN`.
const DOWN: c_uint = 1;

/// `XKB_STATE_MODS_LOCKED`.
const MODS_LOCKED: c_uint = 1 << 2;

/// `XKB_STATE_MODS_EFFECTIVE`.
const MODS_EFFECTIVE: c_uint = 1 << 3;

/// Every `XKB_STATE_MODS_*` bit.
const MODS: c_uint = 0b1111;

/// Every `XKB_STATE_LAYOUT_*` bit.
const LAYOUTS: c_uint = 0b1111_0000;

/// `XKB_STATE_LEDS`.
const LEDS: c_uint = 1 << 8;

/// The name a keymap holds one modifier under, and where its answer goes.
type Named = (&'static CStr, fn(&mut Modifiers));

/// The modifiers this crate names, each with the name the keymap holds it under.
///
/// The four numbered ones are xkb's real modifiers. What they mean is the keymap's decision:
/// `Mod1` is alt and `Mod4` is the logo key on every layout `xkeyboard-config` ships, and a keymap
/// written by hand may map them elsewhere.
const NAMED: [Named; 6] = [
    (c"Shift", |mods| mods.shift = true),
    (c"Control", |mods| mods.control = true),
    (c"Mod1", |mods| mods.alt = true),
    (c"Mod4", |mods| mods.logo = true),
    (c"Lock", |mods| mods.caps = true),
    (c"Mod2", |mods| mods.num = true),
];

/// Which modifiers are on.
///
/// `Shift`, `Lock` and `Control` are fixed by the protocol. `Mod1` is Alt, `Mod2` is num lock and
/// `Mod4` is super by convention: libxkbcommon records the usual mapping for those three, in
/// `xkbcommon-names.h`. `Mod3` and `Mod5` carry no fixed meaning. A keymap assigns them, commonly
/// through a virtual modifier such as `LevelThree` or `LevelFive`, and a caller that needs one asks
/// the keymap rather than the number.
///
/// ```
/// use zgui_xkb::Modifiers;
///
/// let none = Modifiers::default();
/// assert!(none.is_empty());
///
/// let shifted = Modifiers {
///     shift: true,
///     ..Modifiers::default()
/// };
/// assert!(shifted.shift);
/// assert!(!shifted.is_empty());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// Alt, which xkb calls `Mod1`.
    pub alt: bool,
    /// The logo or super key, which xkb calls `Mod4`.
    pub logo: bool,
    /// Caps lock, which xkb calls `Lock`.
    pub caps: bool,
    /// Num lock, which xkb calls `Mod2`.
    pub num: bool,
}

impl Modifiers {
    /// Returns `true` if no modifier is on.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// What one key transition changed.
///
/// A caller reads this to decide what to recompute. Most key presses change nothing here, so a
/// caller that redrew a modifier display on every key would redraw it for every letter typed.
///
/// ```
/// use zgui_xkb::Changed;
///
/// assert!(Changed::default().is_empty());
///
/// let shift_went_down = Changed {
///     modifiers: true,
///     ..Changed::default()
/// };
/// assert!(!shift_went_down.is_empty());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changed {
    /// A modifier went down, came up, latched, locked or unlocked.
    pub modifiers: bool,
    /// The active layout changed.
    pub layout: bool,
    /// A keyboard light changed.
    pub leds: bool,
}

impl Changed {
    /// Reads the mask `xkb_state_update_key` answers with.
    fn from_mask(mask: c_uint) -> Self {
        Self {
            modifiers: mask & MODS != 0,
            layout: mask & LAYOUTS != 0,
            leds: mask & LEDS != 0,
        }
    }

    /// Returns `true` if the transition changed nothing.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// What a key produced when it went down.
///
/// ```no_run
/// use zgui_xkb::{Context, Keycode, RuleNames};
///
/// let keymap = Context::new()?.keymap(&RuleNames::default())?;
/// let mut state = keymap.state()?;
///
/// let press = state.press(Keycode::from_evdev(30));
///
/// println!("{:?} produced {:?}", press.sym, press.text);
/// assert!(press.changed.is_empty(), "a letter changes no modifier");
/// # Ok::<(), zgui_xkb::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Press {
    /// What the key meant, read before the state changed.
    pub sym: Keysym,
    /// The text it produced, read before the state changed. A modifier produces none.
    pub text: Option<String>,
    /// What the press changed in the state.
    pub changed: Changed,
}

/// An `xkb_state`: which modifiers are held, latched and locked, and which layout is active.
///
/// One state stands for one keyboard. Two keyboards on one machine hold two states over one
/// [`crate::Keymap`], because shift held on one of them does not shift the other.
///
/// The state holds a share of the loaded [`Library`] and takes its own reference on the keymap, so
/// the [`crate::Keymap`] may be dropped first.
#[derive(Debug)]
pub struct State {
    /// The library every call goes through.
    library: Arc<Library>,
    /// The state itself.
    handle: NonNull<XkbState>,
}

impl State {
    /// Takes ownership of a new state.
    pub(crate) fn new(library: Arc<Library>, handle: NonNull<XkbState>) -> Self {
        Self { library, handle }
    }

    /// Records a key going down, and reports what it produced.
    ///
    /// The keysym and the text are read before the state is told about the press. libxkbcommon
    /// calls that order the conventional one, and a latched modifier is why: with sticky keys,
    /// shift is latched, `a` is pressed, and the update that records the press clears the latch. A
    /// reading taken after the update therefore answers `a` where the person typed `A`, for the
    /// one key that follows each latch.
    pub fn press(&mut self, key: Keycode) -> Press {
        let sym = self.sym(key);
        let text = self.text(key);
        Press {
            sym,
            text,
            changed: self.update(key, DOWN),
        }
    }

    /// Records a key coming up.
    ///
    /// A release produces no text, so there is nothing to read before it. What comes back is what
    /// the transition changed, which for a modifier is the modifier set.
    pub fn release(&mut self, key: Keycode) -> Changed {
        self.update(key, UP)
    }

    /// Returns what this key means in the state as it stands.
    ///
    /// Reading with no update is what a caller does for a key that arrives already held: a
    /// modifier pressed before this process was listening is in the kernel's own map of held keys
    /// and in no event.
    pub fn sym(&self, key: Keycode) -> Keysym {
        // SAFETY: the symbol is `xkb_state_key_get_one_sym`, which reads the state and the keymap
        // behind it. A code the keymap has no key for answers `XKB_KEY_NoSymbol`.
        let raw = unsafe {
            (self.library.symbols.state_key_get_one_sym)(self.handle.as_ptr(), key.raw())
        };
        Keysym::from_raw(raw)
    }

    /// Returns the text this key produces in the state as it stands.
    ///
    /// A modifier, a function key and a key the layout leaves empty all produce nothing.
    pub fn text(&self, key: Keycode) -> Option<String> {
        read_text(|buffer, size| {
            // SAFETY: the symbol is `xkb_state_key_get_utf8`, which writes into `buffer` up to
            // `size` bytes and answers how many the whole string needs. `read_text` passes the
            // buffer it owns and the length of that buffer.
            unsafe {
                (self.library.symbols.state_key_get_utf8)(
                    self.handle.as_ptr(),
                    key.raw(),
                    buffer,
                    size,
                )
            }
        })
    }

    /// Returns which layout this key is read in.
    ///
    /// A keymap can hold several, and which one a key uses is state. This is the layout a shortcut
    /// looks the key up in through [`crate::Keymap::syms_at_level`].
    pub fn layout(&self, key: Keycode) -> Layout {
        // SAFETY: the symbol is `xkb_state_key_get_layout`, which reads the state and answers an
        // index into the keymap's layouts.
        let raw =
            unsafe { (self.library.symbols.state_key_get_layout)(self.handle.as_ptr(), key.raw()) };
        Layout::from_raw(raw)
    }

    /// Returns which modifiers are on right now.
    ///
    /// This is the effective set: held, latched and locked together. That set decides the level a
    /// key is read at. Caps lock appears here while it is on.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers_in(MODS_EFFECTIVE)
    }

    /// Returns which modifiers are locked right now.
    ///
    /// Locked is what a caps lock light shows and what [`State::modifiers`] cannot say on its own:
    /// caps held and caps locked look the same in the effective set, and only one of them survives
    /// the key coming up.
    pub fn locked(&self) -> Modifiers {
        self.modifiers_in(MODS_LOCKED)
    }

    /// Reads every named modifier out of one component of the state.
    fn modifiers_in(&self, component: c_uint) -> Modifiers {
        let mut modifiers = Modifiers::default();
        for (name, set) in NAMED {
            // SAFETY: the symbol is `xkb_state_mod_name_is_active`. The name is a static C string,
            // and the state is live. It answers -1 for a name the keymap does not hold, so only 1
            // counts as on: a keymap without `Mod2` has no num lock, which reads here the same way
            // num lock being off reads.
            let active = unsafe {
                (self.library.symbols.state_mod_name_is_active)(
                    self.handle.as_ptr(),
                    name.as_ptr(),
                    component,
                )
            };
            if active == 1 {
                set(&mut modifiers);
            }
        }
        modifiers
    }

    /// Tells the state that a key moved.
    fn update(&mut self, key: Keycode, direction: c_uint) -> Changed {
        // SAFETY: the symbol is `xkb_state_update_key`, which changes the state and answers a mask
        // of what it changed. The direction is one of the two values `xkb_key_direction` holds.
        let mask = unsafe {
            (self.library.symbols.state_update_key)(self.handle.as_ptr(), key.raw(), direction)
        };
        Changed::from_mask(mask)
    }
}

/// Gives the state back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_state_unref`, and this is the reference taken by
        // `xkb_state_new`. Nothing here holds another, so it is dropped exactly once.
        unsafe { (self.library.symbols.state_unref)(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    //! The mask, over no library at all.

    use super::*;

    #[test]
    fn a_mask_of_nothing_is_a_transition_that_changed_nothing() {
        assert!(Changed::from_mask(0).is_empty());
        assert!(Modifiers::default().is_empty());
    }

    #[test]
    fn each_group_of_bits_reads_as_its_own_answer() {
        // The three groups are read apart because a caller acts on them apart: a modifier display
        // follows the first, a shortcut table follows the second, and a keyboard light the third.
        assert_eq!(
            Changed::from_mask(MODS_EFFECTIVE),
            Changed {
                modifiers: true,
                ..Changed::default()
            }
        );
        assert_eq!(
            Changed::from_mask(1 << 7),
            Changed {
                layout: true,
                ..Changed::default()
            }
        );
        assert_eq!(
            Changed::from_mask(LEDS),
            Changed {
                leds: true,
                ..Changed::default()
            }
        );
    }

    #[test]
    fn a_mask_with_every_bit_reads_as_every_answer() {
        let changed = Changed::from_mask(MODS | LAYOUTS | LEDS);

        assert!(changed.modifiers && changed.layout && changed.leds);
        assert!(!changed.is_empty());
    }
}
