//! Which modifiers are being held.

use smithay_client_toolkit::seat::keyboard::Modifiers as Held;
use zgui_vocab::Modifiers;

/// The held set, in the standard vocabulary.
///
/// The logo key is *meta* here, as it is on every other desktop this framework runs on: X11 named
/// it after what the window manager did with it and this compositor kept the name, but a shortcut
/// is written once and has to match everywhere.
///
/// The latched locks are not carried, because the vocabulary holds four modifiers and neither of
/// them is one. Nothing is lost: a caps-locked letter arrives as an upper-case letter, which is
/// what the layout already resolved it to.
pub fn modifiers(held: Held) -> Modifiers {
    Modifiers::NONE
        .with(Modifiers::SHIFT, held.shift)
        .with(Modifiers::CONTROL, held.ctrl)
        .with(Modifiers::ALT, held.alt)
        .with(Modifiers::META, held.logo)
}

#[cfg(test)]
mod tests {
    use super::modifiers;
    use smithay_client_toolkit::seat::keyboard::Modifiers as Held;
    use zgui_vocab::Modifiers;

    fn holding(apply: impl FnOnce(&mut Held)) -> Held {
        let mut held = Held::default();
        apply(&mut held);
        held
    }

    #[test]
    fn nothing_held_is_nothing_held() {
        assert_eq!(modifiers(Held::default()), Modifiers::NONE);
        assert!(modifiers(Held::default()).is_empty());
    }

    #[test]
    fn each_modifier_crosses_to_its_own_bit_and_no_other() {
        assert_eq!(
            modifiers(holding(|held| held.shift = true)),
            Modifiers::SHIFT
        );
        assert_eq!(
            modifiers(holding(|held| held.ctrl = true)),
            Modifiers::CONTROL
        );
        assert_eq!(modifiers(holding(|held| held.alt = true)), Modifiers::ALT);
        assert_eq!(modifiers(holding(|held| held.logo = true)), Modifiers::META);
    }

    #[test]
    fn the_logo_key_is_meta_here_whatever_the_desktop_calls_it() {
        let logo = modifiers(holding(|held| held.logo = true));
        assert!(logo.meta());
        assert!(!logo.control());
    }

    #[test]
    fn a_lock_is_not_a_modifier_and_does_not_become_one() {
        // The layout has already resolved a caps-locked letter to an upper-case one. Reporting the
        // lock as a held modifier as well would make every letter typed under it look like a chord.
        let locked = holding(|held| {
            held.caps_lock = true;
            held.num_lock = true;
        });
        assert_eq!(modifiers(locked), Modifiers::NONE);
    }

    #[test]
    fn holding_everything_at_once_sets_everything_at_once() {
        let all = holding(|held| {
            held.shift = true;
            held.ctrl = true;
            held.alt = true;
            held.logo = true;
        });
        assert_eq!(modifiers(all), Modifiers::ALL);
    }
}
