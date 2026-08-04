//! What a key press means to an editor.
//!
//! Kept apart from the editor itself so that an application can rebind its keyboard without
//! touching the editing model, and so that the model can be exercised without inventing key
//! events. The bindings here are the desktop ones every field on the platform has.

use zgui_vocab::{Key, KeyEvent, Modifiers, NamedKey};

use crate::editor::command::Command;
use crate::select::{Granularity, Motion, Selection};

/// The command a key press means, or nothing when it means nothing here.
///
/// Text comes from the key's own text and never from a name: the enter key's standard value is a
/// name, and a mapping that inserted the text of every key would put a carriage return into the
/// document. A key held with a command modifier inserts nothing at all — control-a selects, it
/// does not type an `a`.
pub fn command(event: &KeyEvent, modifiers: Modifiers) -> Option<Command> {
    let extend = modifiers.shift();
    let command = modifiers.control() || modifiers.meta();
    let word = if modifiers.control() || modifiers.alt() {
        Granularity::Word
    } else {
        Granularity::Grapheme
    };
    match &event.key {
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Move(Motion::new(word, false, extend))),
        Key::Named(NamedKey::ArrowRight) => Some(Command::Move(Motion::new(word, true, extend))),
        Key::Named(NamedKey::Home) => Some(Command::Move(Motion::new(
            if command {
                Granularity::Document
            } else {
                Granularity::Paragraph
            },
            false,
            extend,
        ))),
        Key::Named(NamedKey::End) => Some(Command::Move(Motion::new(
            if command {
                Granularity::Document
            } else {
                Granularity::Paragraph
            },
            true,
            extend,
        ))),
        Key::Named(NamedKey::Backspace) => Some(Command::DeleteBackwards(word)),
        Key::Named(NamedKey::Delete) => Some(Command::DeleteForwards(word)),
        Key::Named(NamedKey::Enter) if !command => Some(Command::Insert("\n".to_owned())),
        Key::Named(NamedKey::Space) if !command => Some(Command::Insert(" ".to_owned())),
        Key::Named(NamedKey::Copy) => Some(Command::Copy),
        Key::Named(NamedKey::Cut) => Some(Command::Cut),
        Key::Named(NamedKey::Paste) => Some(Command::RequestPaste),
        Key::Named(NamedKey::Undo) => Some(Command::Undo),
        Key::Named(NamedKey::Redo) => Some(Command::Redo),
        Key::Character(text) if command => shortcut(text, modifiers),
        Key::Character(text) if !text.as_str().is_empty() => {
            Some(Command::Insert(text.as_str().to_owned()))
        }
        _ => None,
    }
}

/// The command a letter held with the platform's command modifier means.
///
/// Matched on the character the layout produces, lowercased, so shift-control-Z is redo on every
/// layout rather than only on the ones where the shifted letter is still reported unshifted.
fn shortcut(text: &str, modifiers: Modifiers) -> Option<Command> {
    match text.to_lowercase().as_str() {
        "a" => Some(Command::SelectAll),
        "c" => Some(Command::Copy),
        "x" => Some(Command::Cut),
        // A request rather than a paste, because the text is not here to paste: the clipboard is
        // the platform's, and whoever holds it answers with [`Command::Paste`].
        "v" => Some(Command::RequestPaste),
        "z" if modifiers.shift() => Some(Command::Redo),
        "z" => Some(Command::Undo),
        "y" => Some(Command::Redo),
        _ => None,
    }
}

/// The command a click means: put the caret where it landed, extending when shift is held.
pub fn pointer(offset: usize, held: Selection, modifiers: Modifiers) -> Command {
    Command::Select(if modifiers.shift() {
        held.moved_to(offset, true)
    } else {
        Selection::caret(offset)
    })
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey};

    use super::command;
    use crate::editor::command::Command;
    use crate::select::{Granularity, Motion};

    /// A press of a character key.
    fn character(text: &str) -> KeyEvent {
        KeyEvent {
            key: Key::Character(text.into()),
            key_without_modifiers: Key::Character(text.into()),
            physical: PhysicalKey::Code(KeyCode::KeyA),
            location: zgui_vocab::KeyLocation::Standard,
            repeat: false,
        }
    }

    #[test]
    fn a_letter_is_inserted_and_the_same_letter_under_control_is_a_shortcut() {
        assert_eq!(
            command(&character("a"), Modifiers::NONE),
            Some(Command::Insert("a".to_owned()))
        );
        assert_eq!(
            command(&character("a"), Modifiers::CONTROL),
            Some(Command::SelectAll)
        );
    }

    #[test]
    fn enter_inserts_a_break_and_not_the_name_of_the_key() {
        let enter = KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
        assert_eq!(
            command(&enter, Modifiers::NONE),
            Some(Command::Insert("\n".to_owned()))
        );
    }

    #[test]
    fn control_widens_an_arrow_to_a_word_and_shift_makes_it_extend() {
        let right = KeyEvent::named(NamedKey::ArrowRight, PhysicalKey::Code(KeyCode::ArrowRight));
        assert_eq!(
            command(&right, Modifiers::NONE),
            Some(Command::Move(Motion::new(
                Granularity::Grapheme,
                true,
                false
            )))
        );
        assert_eq!(
            command(&right, Modifiers::CONTROL | Modifiers::SHIFT),
            Some(Command::Move(Motion::new(Granularity::Word, true, true)))
        );
    }

    #[test]
    fn shift_control_z_is_redo_and_control_z_is_undo() {
        assert_eq!(
            command(&character("z"), Modifiers::CONTROL),
            Some(Command::Undo)
        );
        assert_eq!(
            command(&character("Z"), Modifiers::CONTROL | Modifiers::SHIFT),
            Some(Command::Redo)
        );
    }

    #[test]
    fn control_v_asks_for_the_clipboard_and_a_plain_v_is_typed() {
        assert_eq!(
            command(&character("v"), Modifiers::CONTROL),
            Some(Command::RequestPaste)
        );
        assert_eq!(
            command(&character("v"), Modifiers::NONE),
            Some(Command::Insert("v".to_owned()))
        );
    }

    #[test]
    fn a_key_the_editor_has_no_use_for_is_left_alone() {
        let escape = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
        assert!(command(&escape, Modifiers::NONE).is_none());
        let tab = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
        assert!(command(&tab, Modifiers::NONE).is_none());
    }
}
