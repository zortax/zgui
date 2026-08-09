//! Which modifiers are being held, as the layout that answered reports them.
//!
//! Two layouts answer this question and they answer it in different words, so each has a
//! conversion of its own here. Both narrow onto the same four bits, so a shortcut is written once
//! and matches whichever layout the machine had.
//!
//! # What the four bits leave out
//!
//! The vocabulary names shift, control, alt and the command modifier. AltGr, caps lock and num
//! lock reach none of them, and each is left out on purpose:
//!
//! * **AltGr** selects a third character on a key. It is how `@` is typed on a German layout, so
//!   reporting it as alt would make every such key look like a shortcut being pressed.
//! * **caps lock** and **num lock** change what a key produces, which the layout has already
//!   applied by the time a press arrives here. A shortcut matched against them would fire
//!   differently depending on a light on the keyboard.

use zgui_vocab::Modifiers;

/// Returns the held set libxkbcommon reports, in the standard vocabulary.
///
/// The command key is *meta* here whatever the desktop calls it. xkb calls it `Mod4` and a person
/// calls it super, command or the key with a flag on it. The names collapse at this boundary, so a
/// shortcut is written once.
pub(crate) fn from_xkb(held: zgui_xkb::Modifiers) -> Modifiers {
    Modifiers::NONE
        .with(Modifiers::SHIFT, held.shift)
        .with(Modifiers::CONTROL, held.control)
        .with(Modifiers::ALT, held.alt)
        .with(Modifiers::META, held.logo)
}

/// Returns the held set a console keymap reports, in the standard vocabulary.
///
/// The kernel's eight modifier groups tell the left and the right key of a pair apart, and the
/// vocabulary does not, so each pair folds onto one bit. There is **no command modifier at all**
/// among the eight, so a shortcut that names meta can never match under a console keymap. That is
/// one of the things a program gives up on that layout.
pub(crate) fn from_console(held: zgui_evdev::console::Modifiers) -> Modifiers {
    use zgui_evdev::console::Modifiers as Console;

    let any = |one: Console, other: Console| held.contains(one) || held.contains(other);
    Modifiers::NONE
        .with(
            Modifiers::SHIFT,
            held.contains(Console::SHIFT) || any(Console::LEFT_SHIFT, Console::RIGHT_SHIFT),
        )
        .with(
            Modifiers::CONTROL,
            held.contains(Console::CONTROL) || any(Console::LEFT_CONTROL, Console::RIGHT_CONTROL),
        )
        .with(Modifiers::ALT, held.contains(Console::ALT))
}

#[cfg(test)]
mod tests {
    use super::{from_console, from_xkb};
    use zgui_evdev::console::Modifiers as Console;
    use zgui_vocab::Modifiers;

    /// The xkb set with one modifier on, chosen by name.
    fn xkb(set: impl FnOnce(&mut zgui_xkb::Modifiers)) -> zgui_xkb::Modifiers {
        let mut held = zgui_xkb::Modifiers::default();
        set(&mut held);
        held
    }

    #[test]
    fn nothing_held_is_nothing_held() {
        assert_eq!(from_xkb(zgui_xkb::Modifiers::default()), Modifiers::NONE);
        assert_eq!(from_console(Console::NONE), Modifiers::NONE);
        assert!(from_console(Console::NONE).is_empty());
    }

    #[test]
    fn each_modifier_xkb_names_crosses_to_its_own_bit_and_no_other() {
        let pairs: [(fn(&mut zgui_xkb::Modifiers), Modifiers); 4] = [
            (|held| held.shift = true, Modifiers::SHIFT),
            (|held| held.control = true, Modifiers::CONTROL),
            (|held| held.alt = true, Modifiers::ALT),
            (|held| held.logo = true, Modifiers::META),
        ];
        for (set, expected) in pairs {
            assert_eq!(from_xkb(xkb(set)), expected);
        }
    }

    #[test]
    fn the_command_key_is_meta_here_whatever_xkb_calls_it() {
        // xkb calls it `Mod4`. A shortcut written against the command key has to match on every
        // backend, and the only way that happens is if it is one bit by the time anything above
        // this crate sees it.
        assert!(from_xkb(xkb(|held| held.logo = true)).meta());
        assert!(!from_xkb(xkb(|held| held.logo = true)).control());
    }

    #[test]
    fn a_level_modifier_and_a_lock_reach_no_bit_at_all() {
        // AltGr types `@` on a German layout, so a shortcut matcher that saw it as alt would read
        // every such key as a chord. Caps lock and num lock have already changed what the key
        // produced by the time a press arrives here.
        assert!(from_xkb(xkb(|held| held.alt_gr = true)).is_empty());
        assert!(from_xkb(xkb(|held| held.caps = true)).is_empty());
        assert!(from_xkb(xkb(|held| held.num = true)).is_empty());
        assert!(from_xkb(xkb(|held| held.mod3 = true)).is_empty());
        assert!(from_console(Console::ALTGR).is_empty());
    }

    #[test]
    fn holding_everything_at_once_sets_everything_at_once() {
        let all = xkb(|held| {
            held.shift = true;
            held.control = true;
            held.alt = true;
            held.logo = true;
        });

        assert_eq!(from_xkb(all), Modifiers::ALL);
    }

    #[test]
    fn each_modifier_a_console_keymap_names_crosses_to_its_own_bit() {
        let pairs = [
            (Console::SHIFT, Modifiers::SHIFT),
            (Console::CONTROL, Modifiers::CONTROL),
            (Console::ALT, Modifiers::ALT),
        ];
        for (held, expected) in pairs {
            assert_eq!(from_console(held), expected, "{held:?}");
        }
    }

    #[test]
    fn a_console_keymap_that_tells_the_two_shift_keys_apart_still_reports_shift() {
        // The kernel numbers a left-only and a right-only group beside the shared one, and a
        // keymap may bind either. A conversion that read the shared group alone would report
        // nothing held while the person was holding shift.
        assert_eq!(from_console(Console::LEFT_SHIFT), Modifiers::SHIFT);
        assert_eq!(from_console(Console::RIGHT_SHIFT), Modifiers::SHIFT);
        assert_eq!(from_console(Console::LEFT_CONTROL), Modifiers::CONTROL);
        assert_eq!(from_console(Console::RIGHT_CONTROL), Modifiers::CONTROL);
    }

    #[test]
    fn a_console_keymap_can_never_report_the_command_modifier() {
        // The kernel's eight groups hold no super key. A program on this layout cannot match a
        // shortcut that names meta, and that is a fact about the keymap rather than about this
        // conversion.
        let everything = Console::SHIFT
            | Console::CONTROL
            | Console::ALT
            | Console::ALTGR
            | Console::LEFT_SHIFT
            | Console::RIGHT_SHIFT
            | Console::LEFT_CONTROL
            | Console::RIGHT_CONTROL;

        assert!(!from_console(everything).meta());
        assert_eq!(
            from_console(everything),
            Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT
        );
    }
}
