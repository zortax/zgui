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
//! that range. A console keymap names as many terminals as its own entries do, and nothing here
//! bounds the console side.

use zgui_evdev::Entry;

/// The first terminal `fkey2vt` names.
const FIRST: u32 = 1;

/// The last one: twelve function keys, twelve terminals.
const LAST: u32 = 12;

/// What a keysym that asks for a terminal is called, up to the number.
///
/// **Both spellings are read.** `xkb_keysym_get_name` answers the first, and `xkeyboard-config`
/// writes the second — the alias — in the symbol files it ships. A parser that knew one of them
/// would work on the machine it was written on and answer nothing on the next.
const NAMES: [&str; 2] = ["XF86Switch_VT_", "XF86_Switch_VT_"];

/// The terminal a keysym called `name` asks for.
pub(crate) fn from_keysym(name: &str) -> Option<u32> {
    let asked = NAMES.iter().find_map(|start| name.strip_prefix(start))?;
    // `str::parse` reads a leading `+` and a leading zero, and libxkbcommon names no keysym either
    // way, so the digits have to read back as themselves.
    let terminal: u32 = asked
        .parse()
        .ok()
        .filter(|terminal: &u32| terminal.to_string() == asked)?;

    (FIRST..=LAST).contains(&terminal).then_some(terminal)
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
    //! A keysym name in and a terminal out, and the same for a keymap entry.
    //!
    //! All of it is arithmetic over a string and over a number, so none of it needs libxkbcommon
    //! and none of it needs a console. That the names below are the names libxkbcommon answers with
    //! is a separate question, and `layout`'s own tests hold it up against the library.

    use super::{from_entry, from_keysym};
    use zgui_evdev::Entry;

    #[test]
    fn both_spellings_of_a_switch_keysym_name_the_same_terminal() {
        // libxkbcommon answers the first spelling and `xkeyboard-config` writes the second, so a
        // reader of one alone works on one machine and reports nothing on the next.
        assert_eq!(from_keysym("XF86Switch_VT_1"), Some(1));
        assert_eq!(from_keysym("XF86_Switch_VT_1"), Some(1));
        assert_eq!(from_keysym("XF86Switch_VT_12"), Some(12));
        assert_eq!(from_keysym("XF86_Switch_VT_12"), Some(12));
    }

    #[test]
    fn a_number_outside_what_xkb_names_is_no_terminal() {
        // xkb defines 1 to 12. A number outside them is a name libxkbcommon has no keysym for, so
        // one accepted here could only come from somewhere that made it up.
        assert_eq!(from_keysym("XF86Switch_VT_0"), None);
        assert_eq!(from_keysym("XF86Switch_VT_13"), None);
        assert_eq!(from_keysym("XF86_Switch_VT_13"), None);
    }

    #[test]
    fn a_keysym_that_merely_starts_the_same_way_asks_for_nothing() {
        // Each of these would take a terminal away from a key that types, which is a key a person
        // presses and a program never hears.
        assert_eq!(from_keysym("XF86Switch_VTx"), None);
        assert_eq!(from_keysym("XF86Switch_VT_"), None);
        assert_eq!(from_keysym("XF86Switch_VT_1x"), None);
        assert_eq!(from_keysym("XF86Switch_VT_1_1"), None);
        // `str::parse` takes both of these and answers 1.
        assert_eq!(from_keysym("XF86Switch_VT_01"), None);
        assert_eq!(from_keysym("XF86Switch_VT_+1"), None);
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
            from_keysym("XF86Switch_VT_1"),
            "`Ctrl+Alt+F1` is terminal 1 on both layouts"
        );
        assert_eq!(
            from_entry(Entry::Switch(11)),
            from_keysym("XF86Switch_VT_12"),
            "and `Ctrl+Alt+F12` is terminal 12 on both"
        );
        // The console side carries no bound of its own: a keymap can bind `Console_13` and the
        // kernel switches to it, while xkb has no keysym for one.
        assert_eq!(from_entry(Entry::Switch(12)), Some(13));
    }

    #[test]
    fn an_ordinary_key_asks_for_no_terminal_on_either_path() {
        // A key that answered a terminal is a key a document never sees, so every one of these is
        // a letter or a function key silently taken away.
        assert_eq!(from_keysym("a"), None);
        assert_eq!(from_keysym("F1"), None);
        assert_eq!(from_keysym("XF86AudioPlay"), None);
        assert_eq!(from_entry(Entry::Latin(b'a')), None);
        assert_eq!(from_entry(Entry::Function(0)), None, "F1 with nothing held");
        assert_eq!(from_entry(Entry::Modifier(0)), None);
        assert_eq!(from_entry(Entry::Hole), None);
    }
}
