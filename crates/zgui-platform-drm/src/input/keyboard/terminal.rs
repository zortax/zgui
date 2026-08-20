//! The terminal a key asks for, out of what a layout answered.
//!
//! `Ctrl+Alt+F1` is a chord nothing here binds. Both layouts a Linux machine has answer it already,
//! each in a vocabulary of its own, and this is where the two answers become one number.
//!
//! * **libxkbcommon.** `symbols/pc` includes `srvr_ctrl(fkey2vt)`, which puts `XF86Switch_VT_1` to
//!   `XF86Switch_VT_12` on level five of a `CTRL+ALT` key type. Every standard layout has it, so
//!   the chord produces that keysym and `Ctrl+Alt+F1` has never produced `F1`.
//! * **The console keymap.** `defkeymap.map` binds `control alt keycode 59` to `Console_1`, which
//!   is a `KT_CONS` entry and reaches this crate as [`Entry::Switch`].
//!
//! # The two numberings
//!
//! `XF86Switch_VT_1` is terminal 1. `Console_1` is `K(KT_CONS, 0)`, because the kernel's
//! `set_console` indexes `vc_cons` and that array starts at zero — so `Entry::Switch(0)` is
//! terminal 1 as well. Both cross to the number a person says here, and a caller reads one
//! numbering.
//!
//! # How many terminals each source names
//!
//! xkb names twelve, because twelve is what `fkey2vt` binds, and libxkbcommon has no keysym outside
//! that range. A console keymap names up to `MAX_NR_CONSOLES`, which is 63: `loadkeys` knows
//! `Console_1` to `Console_63` and has no name above them. Nothing here holds that bound, because a
//! number past it is refused by whatever is asked for the terminal.

use zgui_evdev::Entry;

/// The keysym `fkey2vt` puts on the first terminal: `XF86Switch_VT_1`.
///
/// **The keysym is matched by value.** The twelve sit in one contiguous range, so the terminal is
/// how far past this one a keysym sits.
// A name would have to be asked of libxkbcommon and parsed on every key event, and one keysym
// carries two spellings: `xkb_keysym_get_name` answers `XF86Switch_VT_1`, and `xkeyboard-config`
// writes the alias `XF86_Switch_VT_1` in the symbol files it ships. The value is one number under
// either name.
const FIRST: u32 = 0x1008_fe01;

/// The last one: twelve function keys, twelve terminals. `XF86Switch_VT_12`.
const LAST: u32 = 0x1008_fe0c;

/// Returns the terminal the keysym `sym` asks for, as `xkb_keysym_t` numbers it.
pub(crate) const fn from_keysym(sym: u32) -> Option<u32> {
    match sym {
        FIRST..=LAST => Some(sym - FIRST + 1),
        _ => None,
    }
}

/// Returns the terminal a console keymap entry asks for.
pub(crate) fn from_entry(entry: Entry) -> Option<u32> {
    match entry {
        // The kernel counts these from zero. See the module documentation.
        Entry::Switch(console) => Some(u32::from(console) + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! A keysym value in and a terminal out, and the same for a keymap entry.
    //!
    //! All of it is arithmetic over two numbers, so none of it needs libxkbcommon and none of it
    //! needs a console. That the twelve values below are the values libxkbcommon answers with for
    //! the twelve names is a separate question, and `layout`'s own tests hold it up against the
    //! library.

    use super::{from_entry, from_keysym};
    use zgui_evdev::Entry;

    #[test]
    fn the_twelve_switch_keysyms_are_the_twelve_terminals() {
        // One contiguous range, so the terminal is read off the value. A reader
        // that counted from the wrong end would send a person to terminal 12 for `Ctrl+Alt+F1`.
        assert_eq!(from_keysym(0x1008_fe01), Some(1));
        assert_eq!(from_keysym(0x1008_fe02), Some(2));
        assert_eq!(from_keysym(0x1008_fe0c), Some(12));
    }

    #[test]
    fn a_keysym_outside_the_range_asks_for_nothing() {
        // Each of these would take a terminal away from a key that types or names something, which
        // is a key a person presses and a program never hears.
        assert_eq!(from_keysym(0x1008_fe00), None, "one below the first");
        assert_eq!(from_keysym(0x1008_fe0d), None, "one above the last");
        assert_eq!(from_keysym(0x0061), None, "the letter a");
        assert_eq!(from_keysym(0xffbe), None, "`F1` with nothing held");
        assert_eq!(from_keysym(0x1008_ff14), None, "`XF86AudioPlay`");
        assert_eq!(from_keysym(0), None, "no keysym at all");
    }

    #[test]
    fn the_console_keymap_counts_from_zero_and_the_keysym_counts_from_one() {
        // The one trap this module exists for. `Console_1` is `K(KT_CONS, 0)` because the kernel's
        // `set_console` indexes `vc_cons`, so a backend that carried the value through would send
        // a person to the terminal before the one they asked for — and `Ctrl+Alt+F1` would answer
        // terminal 0, which is no terminal at all.
        assert_eq!(from_entry(Entry::Switch(0)), Some(1));
        assert_eq!(from_entry(Entry::Switch(11)), Some(12));
        // The two sources name one terminal for one chord, which is why both of them cross here.
        assert_eq!(
            from_entry(Entry::Switch(0)),
            from_keysym(0x1008_fe01),
            "`Ctrl+Alt+F1` is terminal 1 on both layouts"
        );
        assert_eq!(
            from_entry(Entry::Switch(11)),
            from_keysym(0x1008_fe0c),
            "and `Ctrl+Alt+F12` is terminal 12 on both"
        );
        // The console side reaches further than xkb: a keymap can bind `Console_13` and the kernel
        // switches to it, while libxkbcommon has no keysym for one.
        assert_eq!(from_entry(Entry::Switch(12)), Some(13));
    }

    #[test]
    fn the_widest_console_entry_stays_a_number_libseat_can_be_asked_for() {
        // The console side is deliberately unbounded, so the widest entry a keymap can hold has to
        // survive the whole way down: `Session::switch` takes a `u32` and `libseat_switch_session`
        // takes a `c_int`, so a value that wrapped, went negative or overflowed here would reach
        // the daemon as a terminal nobody asked for.
        let widest =
            from_entry(Entry::Switch(u8::MAX)).expect("a switch entry asks for a terminal");

        assert_eq!(widest, 256, "255 counts from zero, so it is terminal 256");
        assert!(
            i32::try_from(widest).is_ok(),
            "and it crosses to libseat's `c_int` as itself"
        );
    }

    #[test]
    fn an_ordinary_key_asks_for_no_terminal_on_either_path() {
        // A key that answered a terminal is a key a document never sees, so every one of these is
        // a letter or a function key silently taken away.
        assert_eq!(from_entry(Entry::Latin(b'a')), None);
        assert_eq!(from_entry(Entry::Function(0)), None, "F1 with nothing held");
        assert_eq!(from_entry(Entry::Modifier(0)), None);
        assert_eq!(from_entry(Entry::Hole), None);
    }
}
