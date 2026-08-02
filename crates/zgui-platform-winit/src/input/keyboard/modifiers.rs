//! Which modifiers are being held.

use winit::keyboard::ModifiersState;
use zgui_vocab::Modifiers;

/// The held set, in the standard vocabulary.
///
/// The command key is *meta* here whatever the desktop calls it — super, command, or the key with
/// a flag on it. Collapsing the three into one name at this boundary is what lets a shortcut be
/// written once instead of once per desktop.
pub(crate) fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers::NONE
        .with(Modifiers::SHIFT, state.shift_key())
        .with(Modifiers::CONTROL, state.control_key())
        .with(Modifiers::ALT, state.alt_key())
        .with(Modifiers::META, state.super_key())
}

#[cfg(test)]
mod tests {
    use super::modifiers;
    use winit::keyboard::ModifiersState;
    use zgui_vocab::Modifiers;

    #[test]
    fn nothing_held_is_nothing_held() {
        assert_eq!(modifiers(ModifiersState::empty()), Modifiers::NONE);
        assert!(modifiers(ModifiersState::empty()).is_empty());
    }

    #[test]
    fn each_modifier_crosses_to_its_own_bit_and_no_other() {
        let pairs = [
            (ModifiersState::SHIFT, Modifiers::SHIFT),
            (ModifiersState::CONTROL, Modifiers::CONTROL),
            (ModifiersState::ALT, Modifiers::ALT),
            (ModifiersState::SUPER, Modifiers::META),
        ];
        for (state, expected) in pairs {
            assert_eq!(
                modifiers(state),
                expected,
                "{state:?} crossed to the wrong bit"
            );
        }
    }

    #[test]
    fn the_command_key_is_meta_here_whatever_the_desktop_calls_it() {
        // A shortcut written against the command key has to match on every desktop, and the only
        // way that happens is if super, command and the flag key are one bit by the time anything
        // above this crate sees them.
        assert!(modifiers(ModifiersState::SUPER).meta());
        assert!(!modifiers(ModifiersState::SUPER).control());
    }

    #[test]
    fn holding_everything_at_once_sets_everything_at_once() {
        let all = ModifiersState::SHIFT
            | ModifiersState::CONTROL
            | ModifiersState::ALT
            | ModifiersState::SUPER;
        assert_eq!(modifiers(all), Modifiers::ALL);
    }
}
