//! Keys, their positions and the held modifiers.

use std::str::FromStr;

use zgui_vocab::{Key, KeyCode, KeyEvent, KeyLocation, Modifiers, NamedKey, PhysicalKey};

/// The modifiers a set of DOM modifier flags describes.
pub fn modifiers(
    shift: bool,
    control: bool,
    alt: bool,
    meta: bool,
    caps_lock: bool,
    num_lock: bool,
) -> Modifiers {
    Modifiers::NONE
        .with(Modifiers::SHIFT, shift)
        .with(Modifiers::CONTROL, control)
        .with(Modifiers::ALT, alt)
        .with(Modifiers::META, meta)
        .with(Modifiers::CAPS_LOCK, caps_lock)
        .with(Modifiers::NUM_LOCK, num_lock)
}

/// The modifiers held during a keyboard event.
#[cfg(target_arch = "wasm32")]
pub fn of_key(event: &web_sys::KeyboardEvent) -> Modifiers {
    modifiers(
        event.shift_key(),
        event.ctrl_key(),
        event.alt_key(),
        event.meta_key(),
        event.get_modifier_state("CapsLock"),
        event.get_modifier_state("NumLock"),
    )
}

/// The modifiers held during a mouse, pointer or wheel event.
#[cfg(target_arch = "wasm32")]
pub fn of_mouse(event: &web_sys::MouseEvent) -> Modifiers {
    modifiers(
        event.shift_key(),
        event.ctrl_key(),
        event.alt_key(),
        event.meta_key(),
        event.get_modifier_state("CapsLock"),
        event.get_modifier_state("NumLock"),
    )
}

/// The key a DOM `key` value names.
///
/// A name of the standard set is a named key. Anything else that is one character, or that is not
/// ASCII, is the text the key inserts. The rest is a name this vocabulary does not hold.
pub fn key(value: &str) -> Key {
    if let Ok(named) = NamedKey::from_str(value) {
        return Key::Named(named);
    }
    match value {
        "" | "Unidentified" => Key::Unidentified,
        "Dead" => Key::Dead(None),
        text if text.chars().count() == 1 || !text.is_ascii() => Key::character(text),
        other => Key::Other(other.into()),
    }
}

/// The key a position produces with no modifier held, as far as the position says.
///
/// A browser reports no such value. A letter position and a digit position name their own
/// character; any other key keeps the value it was reported with, in lower case.
pub fn unmodified(code: &str, key: &Key) -> Key {
    if let Some(letter) = code.strip_prefix("Key")
        && letter.len() == 1
    {
        return Key::character(letter.to_ascii_lowercase());
    }
    if let Some(digit) = code.strip_prefix("Digit")
        && digit.len() == 1
    {
        return Key::character(digit);
    }
    match key {
        Key::Character(text) => Key::character(text.as_str().to_lowercase()),
        other => other.clone(),
    }
}

/// The position a DOM `code` value names.
pub fn physical(code: &str) -> PhysicalKey {
    KeyCode::from_str(code).map_or(PhysicalKey::Unidentified(0), PhysicalKey::Code)
}

/// Where on the keyboard a DOM key location is.
pub const fn location(location: u32) -> KeyLocation {
    match location {
        1 => KeyLocation::Left,
        2 => KeyLocation::Right,
        3 => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

/// The key event a DOM keyboard event describes.
#[cfg(target_arch = "wasm32")]
pub fn event(event: &web_sys::KeyboardEvent) -> KeyEvent {
    let code = event.code();
    let key = key(&event.key());
    KeyEvent {
        key_without_modifiers: unmodified(&code, &key),
        key,
        physical: physical(&code),
        location: location(event.location()),
        repeat: event.repeat(),
    }
}

/// Whether the page keeps its own answer to a key.
///
/// A key with the control or the command modifier is a browser shortcut more often than not:
/// reload, a new tab, the developer tools, copy and paste. The function keys are the browser's
/// too. The application still receives each of them. Every other key belongs to the application
/// alone, so that a space or an arrow key does not scroll the page under the canvas.
pub fn page_keeps(event: &KeyEvent, modifiers: Modifiers) -> bool {
    if modifiers.control() || modifiers.meta() {
        return true;
    }
    matches!(
        &event.key,
        Key::Named(
            NamedKey::F1
                | NamedKey::F2
                | NamedKey::F3
                | NamedKey::F4
                | NamedKey::F5
                | NamedKey::F6
                | NamedKey::F7
                | NamedKey::F8
                | NamedKey::F9
                | NamedKey::F10
                | NamedKey::F11
                | NamedKey::F12
        )
    )
}

#[cfg(test)]
mod tests {
    use super::{key, location, modifiers, page_keeps, physical, unmodified};
    use zgui_vocab::{Key, KeyCode, KeyEvent, KeyLocation, Modifiers, NamedKey, PhysicalKey};

    #[test]
    fn a_standard_name_is_a_named_key_and_one_character_is_text() {
        assert_eq!(key("Enter"), Key::Named(NamedKey::Enter));
        assert_eq!(key(" "), Key::Named(NamedKey::Space));
        assert_eq!(key("a"), Key::character("a"));
        assert_eq!(key("é"), Key::character("é"));
        assert_eq!(key("Dead"), Key::Dead(None));
        assert_eq!(key("Unidentified"), Key::Unidentified);
        assert_eq!(key("SomeFutureKey"), Key::Other("SomeFutureKey".into()));
    }

    #[test]
    fn a_letter_position_names_its_own_character_whatever_was_typed() {
        assert_eq!(
            unmodified("KeyV", &Key::character("V")),
            Key::character("v")
        );
        assert_eq!(
            unmodified("Digit1", &Key::character("!")),
            Key::character("1")
        );
        assert_eq!(
            unmodified("Enter", &Key::Named(NamedKey::Enter)),
            Key::Named(NamedKey::Enter)
        );
    }

    #[test]
    fn a_position_outside_the_vocabulary_is_carried_as_unidentified() {
        assert_eq!(physical("KeyA"), PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(physical("NoSuchCode"), PhysicalKey::Unidentified(0));
        assert_eq!(location(3), KeyLocation::Numpad);
        assert_eq!(location(0), KeyLocation::Standard);
    }

    #[test]
    fn the_page_keeps_its_shortcuts_and_the_application_keeps_the_rest() {
        let held = modifiers(false, true, false, false, false, false);
        let reload = KeyEvent::character("r");
        assert!(page_keeps(&reload, held));
        assert!(!page_keeps(&reload, Modifiers::NONE));
        let space = KeyEvent::named(NamedKey::Space, PhysicalKey::Code(KeyCode::Space));
        assert!(!page_keeps(&space, Modifiers::NONE));
        let tools = KeyEvent::named(NamedKey::F12, PhysicalKey::Unidentified(0));
        assert!(page_keeps(&tools, Modifiers::NONE));
    }
}
