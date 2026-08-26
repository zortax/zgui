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

/// How many modifiers this crate names.
pub(crate) const MODIFIERS: usize = 8;

/// The name a keymap holds one modifier under, how to read it, and how to set it.
///
/// Reading and writing go through the same row, so a modifier added or moved reaches the same
/// field in both directions.
type Named = (
    &'static CStr,
    fn(Modifiers) -> bool,
    fn(&mut Modifiers, bool),
);

/// The modifiers this crate names, each with the name the keymap holds it under.
///
/// These eight are xkb's real modifiers, and libxkbcommon holds their names fixed. What a numbered
/// one stands for is the keymap's decision; [`Modifiers`] says how far that decision is settled.
pub(crate) const NAMED: [Named; MODIFIERS] = [
    (c"Shift", |m| m.shift, |m, on| m.shift = on),
    (c"Control", |m| m.control, |m, on| m.control = on),
    (c"Mod1", |m| m.alt, |m, on| m.alt = on),
    (c"Mod2", |m| m.num, |m, on| m.num = on),
    (c"Mod3", |m| m.mod3, |m, on| m.mod3 = on),
    (c"Mod4", |m| m.logo, |m, on| m.logo = on),
    (c"Mod5", |m| m.alt_gr, |m, on| m.alt_gr = on),
    (c"Lock", |m| m.caps, |m, on| m.caps = on),
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
    /// Num lock, which xkb calls `Mod2`.
    pub num: bool,
    /// What the keymap puts on `Mod3`.
    ///
    /// The number stands for nothing on its own. A keymap that uses it says so, and a keymap that
    /// does not leaves this off.
    pub mod3: bool,
    /// The logo or super key, which xkb calls `Mod4`.
    pub logo: bool,
    /// What the keymap puts on `Mod5`.
    ///
    /// The number stands for nothing on its own. A layout reaches the third level of a key through
    /// the virtual modifier `LevelThree`, and the field is named for the keymaps that bind that to
    /// `Mod5`, where it reads as AltGr. A caller that needs the third level asks the keymap for the
    /// level rather than reading this number.
    pub alt_gr: bool,
    /// Caps lock, which xkb calls `Lock`.
    pub caps: bool,
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
    ///
    /// There is nothing to act on yet: which light it was needs `xkb_state_led_name_is_active`,
    /// which this crate does not bind because nothing here drives a light.
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
    ///
    /// A key whose level carries several keysyms answers [`Keysym::NONE`], and so does a key the
    /// keymap has nothing for. Feeding it to [`crate::ComposeState::feed`] is correct either way:
    /// the compose machine ignores it.
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
///
/// libxkbcommon tracks a modifier by counting its transitions, so every press has to be matched by
/// a release. A press with no release leaves the modifier held for good, and two cases produce
/// one:
///
/// - A key that was already down when the state was made. [`State::hold`] is the answer.
/// - Autorepeat. The kernel reports a held key as `EV_KEY` with value 2, over and over, and a
///   caller that treats every non-zero value as a press calls [`State::press`] many times and
///   [`State::release`] once. A repeat is not a transition: read it with [`State::sym`] and
///   [`State::text`] and record nothing. [`crate::Keymap::key_repeats`] says which keys the
///   keymap wants repeated at all.
#[derive(Debug)]
pub struct State {
    /// The library every call goes through.
    library: Arc<Library>,
    /// The state itself.
    handle: NonNull<XkbState>,
    /// Where each modifier in [`NAMED`] sits in this keymap's table, resolved once.
    indices: [Option<u32>; MODIFIERS],
}

impl State {
    /// Takes ownership of a new state.
    pub(crate) fn new(
        library: Arc<Library>,
        handle: NonNull<XkbState>,
        indices: [Option<u32>; MODIFIERS],
    ) -> Self {
        Self {
            library,
            handle,
            indices,
        }
    }

    /// Records a key going down, and reports what it produced.
    ///
    /// The keysym and the text are read before the state is told about the press. libxkbcommon
    /// calls that order the conventional one, and a latched modifier is why: a level-three latch is
    /// set, `Q` is pressed, and the update that records the press clears the latch. A reading taken
    /// after the update therefore answers `q` where the person typed `@`, for the one key that
    /// follows each latch.
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

    /// Records a key that was already down when this state was made.
    ///
    /// A keyboard is opened in whatever state somebody left it, and nothing that arrives afterwards
    /// says so: a modifier held before this process was listening is in the kernel's own map of
    /// held keys and in no event. `EVIOCGKEY` reads that map, and every key it reports comes
    /// through here.
    ///
    /// This is [`State::press`] without the reading, because a key that was already down produced
    /// its character before anybody was watching. The release that balances it is the one the
    /// kernel sends when the finger comes up.
    ///
    /// Caps lock is on with its key up, so it is in no map of held keys. `EVIOCGLED` reports it.
    /// Turning it on is a `hold` and a [`State::release`] of the caps key, the same pair of
    /// transitions that turned it on under somebody's finger.
    pub fn hold(&mut self, key: Keycode) -> Changed {
        // `xkb_state_update_mask` looks like a shorter route. libxkbcommon states that it feeds a
        // *client* state and must never update a *server* state, and a state fed by `press` is a
        // server state. Mixing the two leaves a modifier that no release turns off.
        self.update(key, DOWN)
    }

    /// Returns what this key means in the state as it stands.
    ///
    /// Nothing is recorded, so this is what a caller reads for a key that changed no state: an
    /// autorepeat of a key that is already down. A key that was down before this state existed is
    /// a different problem, and [`State::hold`] is its answer.
    ///
    /// Answers [`Keysym::NONE`] for a code the keymap has no key for, and for a level that carries
    /// several keysyms rather than one.
    pub fn sym(&self, key: Keycode) -> Keysym {
        // SAFETY: the symbol is `xkb_state_key_get_one_sym`, which reads the state and the keymap
        // behind it. A code the keymap has no key for answers `XKB_KEY_NoSymbol`.
        let raw = unsafe {
            (self.library.symbols.core.state_key_get_one_sym)(self.handle.as_ptr(), key.raw())
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
                (self.library.symbols.core.state_key_get_utf8)(
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
    /// looks the key up in through [`crate::Keymap::syms_at_level`]. A code the keymap has no key
    /// for answers [`Layout::INVALID`].
    pub fn layout(&self, key: Keycode) -> Layout {
        // SAFETY: the symbol is `xkb_state_key_get_layout`, which reads the state and answers an
        // index into the keymap's layouts.
        let raw = unsafe {
            (self.library.symbols.core.state_key_get_layout)(self.handle.as_ptr(), key.raw())
        };
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
    ///
    /// The indices were resolved when the state was made, so this costs one call each rather than
    /// a scan of the keymap's modifier table by name. A caller reads this on every key press.
    fn modifiers_in(&self, component: c_uint) -> Modifiers {
        let mut modifiers = Modifiers::default();
        for (index, (_, _, set)) in self.indices.iter().zip(NAMED) {
            let Some(index) = index else {
                continue;
            };
            // SAFETY: the symbol is `xkb_state_mod_index_is_active`. The state is live, and the
            // index came out of the keymap behind it, so it names a modifier that keymap holds.
            let active = unsafe {
                (self.library.symbols.core.state_mod_index_is_active)(
                    self.handle.as_ptr(),
                    *index,
                    component,
                )
            };
            if active == 1 {
                set(&mut modifiers, true);
            }
        }
        modifiers
    }

    /// Tells the state that a key moved.
    fn update(&mut self, key: Keycode, direction: c_uint) -> Changed {
        // SAFETY: the symbol is `xkb_state_update_key`, which changes the state and answers a mask
        // of what it changed. The direction is one of the two values `xkb_key_direction` holds.
        let mask = unsafe {
            (self.library.symbols.core.state_update_key)(self.handle.as_ptr(), key.raw(), direction)
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
        unsafe { (self.library.symbols.core.state_unref)(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    //! The mask and the modifier table, over no library at all.

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

    #[test]
    fn every_modifier_is_read_back_out_of_the_field_it_was_written_to() {
        // Eight rows of the same shape, so a reader and a writer that drifted apart would report
        // control while shift was held. Each row is checked against itself.
        for (name, get, set) in NAMED {
            let mut modifiers = Modifiers::default();
            set(&mut modifiers, true);

            assert!(get(modifiers), "{name:?} reads back what it wrote");
            assert!(!modifiers.is_empty());
            for (other, other_get, _) in NAMED {
                if other != name {
                    assert!(
                        !other_get(modifiers),
                        "{name:?} sets nothing that belongs to {other:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_table_names_the_modifiers_a_layout_needs() {
        let names: Vec<&CStr> = NAMED.iter().map(|(name, _, _)| *name).collect();

        assert!(names.contains(&c"Mod5"), "AltGr reaches the third level");
        assert!(names.contains(&c"Lock"), "caps lock is a modifier");
        assert_eq!(names.len(), MODIFIERS);
    }
}
