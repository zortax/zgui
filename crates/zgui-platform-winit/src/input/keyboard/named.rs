//! What a key means once the layout has been applied to it.

use core::str::FromStr;

use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamed};
use zgui_vocab::{Key, NamedKey};

/// A key the layout produced, in the standard vocabulary.
///
/// `text` is what the platform says this press actually produces, and it is preferred over the
/// character the key carries because the two differ exactly where it matters: a dead key that
/// could not be combined produces two characters, and only the first of the two vocabularies knows
/// that.
///
/// A key that merely *has* a text representation is not turned into that text. Enter's carriage
/// return is the case that decides the rule: a field that inserted a carriage return every time
/// enter was pressed would look correct in every test written against key names and would be
/// unusable. So a named key stays named, and what a named key inserts — nothing, for all of them
/// but the space bar — is decided once, in the vocabulary, rather than here.
pub(crate) fn key(logical: &WinitKey, text: Option<&str>) -> Key {
    match logical {
        WinitKey::Named(named) => named_key(*named),
        WinitKey::Character(character) => Key::Character(text.unwrap_or(character.as_str()).into()),
        WinitKey::Dead(character) => Key::Dead(*character),
        WinitKey::Unidentified(_) => Key::Unidentified,
    }
}

/// A named key in the standard vocabulary, or its standard name when this vocabulary has no
/// variant for it.
///
/// Both vocabularies name their variants after the standard key values, so the correspondence is
/// the name rather than a transcribed table of three hundred rows. A key the standard set does not
/// hold keeps its standard name and arrives as an unenumerated key, which is still bindable.
fn named_key(named: WinitNamed) -> Key {
    // The one key whose standard value is not its own name: the space bar's value is a space.
    if named == WinitNamed::Space {
        return Key::Named(NamedKey::Space);
    }
    let name = format!("{named:?}");
    NamedKey::from_str(&name).map_or_else(|_| Key::Other(name.into()), Key::Named)
}

#[cfg(test)]
mod tests {
    use super::{key, named_key};
    use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamed, NativeKey};
    use zgui_vocab::{Key, NamedKey};

    /// The named keys a control or a shortcut is written against.
    const CORRESPONDENCE: &[(WinitNamed, NamedKey)] = &[
        (WinitNamed::Enter, NamedKey::Enter),
        (WinitNamed::Tab, NamedKey::Tab),
        (WinitNamed::Escape, NamedKey::Escape),
        (WinitNamed::Backspace, NamedKey::Backspace),
        (WinitNamed::Delete, NamedKey::Delete),
        (WinitNamed::Insert, NamedKey::Insert),
        (WinitNamed::Home, NamedKey::Home),
        (WinitNamed::End, NamedKey::End),
        (WinitNamed::PageUp, NamedKey::PageUp),
        (WinitNamed::PageDown, NamedKey::PageDown),
        (WinitNamed::ArrowUp, NamedKey::ArrowUp),
        (WinitNamed::ArrowDown, NamedKey::ArrowDown),
        (WinitNamed::ArrowLeft, NamedKey::ArrowLeft),
        (WinitNamed::ArrowRight, NamedKey::ArrowRight),
        (WinitNamed::Shift, NamedKey::Shift),
        (WinitNamed::Control, NamedKey::Control),
        (WinitNamed::Alt, NamedKey::Alt),
        (WinitNamed::Meta, NamedKey::Meta),
        (WinitNamed::Super, NamedKey::Super),
        (WinitNamed::AltGraph, NamedKey::AltGraph),
        (WinitNamed::CapsLock, NamedKey::CapsLock),
        (WinitNamed::ContextMenu, NamedKey::ContextMenu),
        (WinitNamed::F1, NamedKey::F1),
        (WinitNamed::F12, NamedKey::F12),
        (WinitNamed::Copy, NamedKey::Copy),
        (WinitNamed::Paste, NamedKey::Paste),
        (WinitNamed::Process, NamedKey::Process),
        (WinitNamed::Compose, NamedKey::Compose),
        (WinitNamed::Space, NamedKey::Space),
    ];

    #[test]
    fn every_named_key_a_control_is_written_against_survives_the_crossing() {
        for (platform, standard) in CORRESPONDENCE {
            assert_eq!(
                named_key(*platform),
                Key::Named(*standard),
                "{platform:?} did not cross to {standard:?}"
            );
        }
    }

    #[test]
    fn the_space_bar_carries_a_space_and_the_return_key_does_not() {
        // Both are named keys and only one of them inserts anything. Getting this backwards gives
        // a text field that swallows spaces, or one that inserts a carriage return.
        assert_eq!(
            named_key(WinitNamed::Space).inserted_text(),
            Some(" "),
            "the space bar stopped inserting a space"
        );
        assert_eq!(named_key(WinitNamed::Enter).inserted_text(), None);
        assert_eq!(named_key(WinitNamed::Tab).inserted_text(), None);
    }

    #[test]
    fn a_key_this_vocabulary_does_not_name_keeps_its_standard_name() {
        let exotic = named_key(WinitNamed::TVPower);
        assert_eq!(exotic, Key::Other("TVPower".into()));
        assert_eq!(exotic.as_str(), Some("TVPower"));
        assert!(!exotic.is_modifier());
    }

    #[test]
    fn the_text_the_platform_reports_wins_over_the_character_on_the_key() {
        // A dead key that could not be combined produces two characters, and only the platform
        // knows that. Taking the character off the key would insert one of them.
        let composed = key(&WinitKey::Character("e".into()), Some("^e"));
        assert_eq!(composed.inserted_text(), Some("^e"));

        let plain = key(&WinitKey::Character("a".into()), None);
        assert_eq!(plain.inserted_text(), Some("a"));
    }

    #[test]
    fn a_dead_key_is_a_dead_key_and_inserts_nothing() {
        let dead = key(&WinitKey::Dead(Some('\u{301}')), None);
        assert_eq!(dead, Key::Dead(Some('\u{301}')));
        assert_eq!(dead.inserted_text(), None);
    }

    #[test]
    fn a_key_the_platform_could_not_identify_arrives_as_unidentified() {
        assert_eq!(
            key(&WinitKey::Unidentified(NativeKey::Unidentified), None),
            Key::Unidentified
        );
    }
}
