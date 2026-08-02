//! A key press, under the three readings a press has to be given.
//!
//! One press answers three unrelated questions and gives three different answers, and all three
//! have to survive the crossing together or something above has to guess:
//!
//! * **what to insert** — the layout applied, with the modifiers applied;
//! * **which shortcut this is** — the layout applied, the modifiers *not* applied, so that a
//!   shortcut written for one key stays on that key when a modifier would have remapped it;
//! * **which position was pressed** — no layout at all, so that a game's movement keys sit where
//!   the fingers are rather than where the letters are.
//!
//! A press also says whether it is a repeat, and that too is carried rather than resolved:
//! holding a letter down should insert another letter and must not run a command a second time,
//! and only the thing being told about the press knows which of those it is doing.

mod code;
mod modifiers;
mod named;

pub(crate) use crate::input::keyboard::modifiers::modifiers;

use winit::event::{ElementState, KeyEvent as WinitKeyEvent};
use zgui_vocab::{KeyEvent, KeyLocation, KeyState};

/// The platforms whose key events carry a modifier-free reading of the key.
///
/// Where the platform supplies one it is used, because it is the layout's own answer. Where it
/// does not, the reading with modifiers applied stands in — which makes a shortcut on those
/// platforms follow the modified key, and is the honest degradation rather than a guess.
#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Whether a key went down or came up.
pub(crate) const fn state(state: ElementState) -> KeyState {
    match state {
        ElementState::Pressed => KeyState::Pressed,
        ElementState::Released => KeyState::Released,
    }
}

/// Where a key sits when a keyboard has more than one of it.
pub(crate) const fn location(location: winit::keyboard::KeyLocation) -> KeyLocation {
    match location {
        winit::keyboard::KeyLocation::Left => KeyLocation::Left,
        winit::keyboard::KeyLocation::Right => KeyLocation::Right,
        winit::keyboard::KeyLocation::Numpad => KeyLocation::Numpad,
        winit::keyboard::KeyLocation::Standard => KeyLocation::Standard,
    }
}

/// A key press, with all three readings of it.
pub(crate) fn event(event: &WinitKeyEvent) -> KeyEvent {
    KeyEvent {
        key: named::key(&event.logical_key, event.text.as_deref()),
        key_without_modifiers: unmodified(event),
        physical: code::physical(event.physical_key),
        location: location(event.location),
        repeat: event.repeat,
    }
}

/// The key with the layout applied and the modifiers not applied.
#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn unmodified(event: &WinitKeyEvent) -> zgui_vocab::Key {
    named::key(&event.key_without_modifiers(), None)
}

/// The same, on a platform that does not supply a modifier-free reading.
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn unmodified(event: &WinitKeyEvent) -> zgui_vocab::Key {
    named::key(&event.logical_key, None)
}

#[cfg(test)]
mod tests {
    use super::{location, state};
    use winit::event::ElementState;
    use zgui_vocab::{EventKind, KeyLocation, KeyState};

    #[test]
    fn a_press_and_a_release_stay_different_all_the_way_down() {
        assert_eq!(state(ElementState::Pressed), KeyState::Pressed);
        assert_eq!(state(ElementState::Released), KeyState::Released);
        assert_eq!(
            state(ElementState::Pressed).event_kind(),
            EventKind::KeyDown
        );
        assert_eq!(state(ElementState::Released).event_kind(), EventKind::KeyUp);
    }

    #[test]
    fn every_place_a_key_can_sit_crosses_to_its_own_place() {
        let pairs = [
            (
                winit::keyboard::KeyLocation::Standard,
                KeyLocation::Standard,
            ),
            (winit::keyboard::KeyLocation::Left, KeyLocation::Left),
            (winit::keyboard::KeyLocation::Right, KeyLocation::Right),
            (winit::keyboard::KeyLocation::Numpad, KeyLocation::Numpad),
        ];
        for (platform, standard) in pairs {
            assert_eq!(location(platform), standard);
        }
    }
}
