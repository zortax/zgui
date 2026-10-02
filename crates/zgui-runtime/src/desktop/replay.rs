//! The key press an editing role of the application menu stands for.

use zgui_platform::{MenuRole, SurfaceEvent};
use zgui_vocab::{Key, KeyEvent, KeyLocation, KeyState, PhysicalKey, Timestamp};

/// The press of `role`'s shortcut, as the focused window receives it.
///
/// The desktop gives the shortcut to the menu, so the window never receives the press itself.
/// This press stands in for it.
pub(crate) fn replay(role: MenuRole, timestamp: Timestamp) -> Option<SurfaceEvent> {
    let shortcut = role.shortcut()?;
    let key = match &shortcut.key {
        Key::Character(text) if shortcut.modifiers.shift() => Key::character(text.to_uppercase()),
        other => other.clone(),
    };
    Some(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent {
            key,
            key_without_modifiers: shortcut.key,
            physical: PhysicalKey::Unidentified(0),
            location: KeyLocation::Standard,
            repeat: false,
        },
        modifiers: shortcut.modifiers,
        timestamp,
    })
}
