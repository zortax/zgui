//! What a key press means to an editor.
//!
//! Kept apart from the editor itself so that an application can rebind its keyboard without
//! touching the editing model, and so that the model can be exercised without inventing key
//! events. The bindings here are the desktop ones every field on the platform has.

use zgui_vocab::{Key, KeyEvent, Modifiers, NamedKey};

use crate::editor::command::Command;
use crate::select::{Granularity, Motion, Selection};

/// Which desktop's text keys an editor follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Keymap {
    /// Linux and Windows: control is the command modifier, and control with an arrow moves by a
    /// word.
    Standard,
    /// macOS: command is the command modifier and moves to an edge, option moves by a word, and
    /// control runs the Emacs keys every Cocoa field has.
    Mac,
}

impl Keymap {
    /// The keymap of the desktop this program was built for.
    pub const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::Mac
    } else {
        Self::Standard
    };
}

/// The command a key press means on this desktop, or nothing when it means nothing here.
///
/// Text comes from the key's own text and never from a name: the enter key's standard value is a
/// name, and a mapping that inserted the text of every key would put a carriage return into the
/// document. A key held with a command modifier inserts nothing at all — control-a selects, it
/// does not type an `a`.
pub fn command(event: &KeyEvent, modifiers: Modifiers) -> Option<Command> {
    command_on(Keymap::CURRENT, event, modifiers)
}

/// The command a key press means under `keymap`.
pub fn command_on(keymap: Keymap, event: &KeyEvent, modifiers: Modifiers) -> Option<Command> {
    match keymap {
        Keymap::Mac => mac(event, modifiers),
        Keymap::Standard => standard(event, modifiers),
    }
}

/// The text keys of Linux and Windows.
fn standard(event: &KeyEvent, modifiers: Modifiers) -> Option<Command> {
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
        Key::Character(text) if command => shortcut(text, modifiers),
        _ => common(event, command),
    }
}

/// The text keys of macOS.
///
/// Command with an arrow goes to the edge of the line or the text, option moves by a word, and
/// command or option with a delete key takes the same stretch. Control runs the Emacs keys: A and E
/// go to the line edges, B and F move by a character, H and D delete one, and K deletes to the end
/// of the line. A line is a paragraph here, which is the whole line of a single-line field.
fn mac(event: &KeyEvent, modifiers: Modifiers) -> Option<Command> {
    let extend = modifiers.shift();
    let command = modifiers.meta();
    let stretch = if command {
        Granularity::Paragraph
    } else if modifiers.alt() {
        Granularity::Word
    } else {
        Granularity::Grapheme
    };
    let edge = |forwards| {
        Some(Command::Move(Motion::new(
            Granularity::Document,
            forwards,
            extend,
        )))
    };
    match &event.key {
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Move(Motion::new(stretch, false, extend))),
        Key::Named(NamedKey::ArrowRight) => Some(Command::Move(Motion::new(stretch, true, extend))),
        Key::Named(NamedKey::ArrowUp) if command => edge(false),
        Key::Named(NamedKey::ArrowDown) if command => edge(true),
        Key::Named(NamedKey::Home) => Some(Command::Move(Motion::new(
            Granularity::Paragraph,
            false,
            extend,
        ))),
        Key::Named(NamedKey::End) => Some(Command::Move(Motion::new(
            Granularity::Paragraph,
            true,
            extend,
        ))),
        Key::Named(NamedKey::Backspace) => Some(Command::DeleteBackwards(stretch)),
        Key::Named(NamedKey::Delete) => Some(Command::DeleteForwards(stretch)),
        Key::Character(text) if command => shortcut(text, modifiers),
        _ if modifiers.control() => emacs(event, extend),
        _ => common(event, command),
    }
}

/// The Emacs keys a Cocoa field answers with control held.
///
/// Read from the key without modifiers, because control turns a letter into a control character.
/// A control chord the field has no use for inserts nothing.
fn emacs(event: &KeyEvent, extend: bool) -> Option<Command> {
    let Key::Character(text) = &event.key_without_modifiers else {
        return None;
    };
    let motion =
        |granularity, forwards| Some(Command::Move(Motion::new(granularity, forwards, extend)));
    match text.to_lowercase().as_str() {
        "a" => motion(Granularity::Paragraph, false),
        "e" => motion(Granularity::Paragraph, true),
        "b" => motion(Granularity::Grapheme, false),
        "f" => motion(Granularity::Grapheme, true),
        "h" => Some(Command::DeleteBackwards(Granularity::Grapheme)),
        "d" => Some(Command::DeleteForwards(Granularity::Grapheme)),
        "k" => Some(Command::DeleteForwards(Granularity::Paragraph)),
        _ => None,
    }
}

/// The keys every desktop reads the same way: enter, space, the editing keys and plain text.
fn common(event: &KeyEvent, command: bool) -> Option<Command> {
    match &event.key {
        Key::Named(NamedKey::Enter) if !command => Some(Command::Insert("\n".to_owned())),
        Key::Named(NamedKey::Space) if !command => Some(Command::Insert(" ".to_owned())),
        Key::Named(NamedKey::Copy) => Some(Command::Copy),
        Key::Named(NamedKey::Cut) => Some(Command::Cut),
        Key::Named(NamedKey::Paste) => Some(Command::RequestPaste),
        Key::Named(NamedKey::Undo) => Some(Command::Undo),
        Key::Named(NamedKey::Redo) => Some(Command::Redo),
        Key::Character(text) if !command && !text.as_str().is_empty() => {
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

    use super::{Keymap, command_on};
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
            command_on(Keymap::Standard, &character("a"), Modifiers::NONE),
            Some(Command::Insert("a".to_owned()))
        );
        assert_eq!(
            command_on(Keymap::Standard, &character("a"), Modifiers::CONTROL),
            Some(Command::SelectAll)
        );
    }

    #[test]
    fn enter_inserts_a_break_and_not_the_name_of_the_key() {
        let enter = KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
        assert_eq!(
            command_on(Keymap::Standard, &enter, Modifiers::NONE),
            Some(Command::Insert("\n".to_owned()))
        );
    }

    #[test]
    fn control_widens_an_arrow_to_a_word_and_shift_makes_it_extend() {
        let right = KeyEvent::named(NamedKey::ArrowRight, PhysicalKey::Code(KeyCode::ArrowRight));
        assert_eq!(
            command_on(Keymap::Standard, &right, Modifiers::NONE),
            Some(Command::Move(Motion::new(
                Granularity::Grapheme,
                true,
                false
            )))
        );
        assert_eq!(
            command_on(
                Keymap::Standard,
                &right,
                Modifiers::CONTROL | Modifiers::SHIFT
            ),
            Some(Command::Move(Motion::new(Granularity::Word, true, true)))
        );
    }

    #[test]
    fn shift_control_z_is_redo_and_control_z_is_undo() {
        assert_eq!(
            command_on(Keymap::Standard, &character("z"), Modifiers::CONTROL),
            Some(Command::Undo)
        );
        assert_eq!(
            command_on(
                Keymap::Standard,
                &character("Z"),
                Modifiers::CONTROL | Modifiers::SHIFT
            ),
            Some(Command::Redo)
        );
    }

    #[test]
    fn control_v_asks_for_the_clipboard_and_a_plain_v_is_typed() {
        assert_eq!(
            command_on(Keymap::Standard, &character("v"), Modifiers::CONTROL),
            Some(Command::RequestPaste)
        );
        assert_eq!(
            command_on(Keymap::Standard, &character("v"), Modifiers::NONE),
            Some(Command::Insert("v".to_owned()))
        );
    }

    #[test]
    fn a_key_the_editor_has_no_use_for_is_left_alone() {
        let escape = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
        assert!(command_on(Keymap::Standard, &escape, Modifiers::NONE).is_none());
        let tab = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
        assert!(command_on(Keymap::Standard, &tab, Modifiers::NONE).is_none());
    }

    /// A press of a named key.
    fn named(key: NamedKey) -> KeyEvent {
        KeyEvent::named(key, PhysicalKey::Unidentified(0))
    }

    /// A control chord, as a layout reports it: a control character over the letter.
    fn controlled(letter: &str) -> KeyEvent {
        KeyEvent {
            key: Key::Character("\u{1}".into()),
            key_without_modifiers: Key::Character(letter.into()),
            ..character(letter)
        }
    }

    #[test]
    fn on_a_mac_command_leads_the_shortcuts_and_control_does_not() {
        let mac = |event: &KeyEvent, modifiers| command_on(Keymap::Mac, event, modifiers);
        assert_eq!(
            mac(&character("a"), Modifiers::META),
            Some(Command::SelectAll)
        );
        assert_eq!(
            mac(&character("v"), Modifiers::META),
            Some(Command::RequestPaste)
        );
        assert_ne!(
            mac(&controlled("a"), Modifiers::CONTROL),
            Some(Command::SelectAll)
        );
    }

    #[test]
    fn on_a_mac_command_goes_to_an_edge_and_option_moves_by_a_word() {
        let mac = |event: &KeyEvent, modifiers| command_on(Keymap::Mac, event, modifiers);
        assert_eq!(
            mac(&named(NamedKey::ArrowLeft), Modifiers::META),
            Some(Command::Move(Motion::new(
                Granularity::Paragraph,
                false,
                false
            )))
        );
        assert_eq!(
            mac(
                &named(NamedKey::ArrowRight),
                Modifiers::ALT | Modifiers::SHIFT
            ),
            Some(Command::Move(Motion::new(Granularity::Word, true, true)))
        );
        assert_eq!(
            mac(&named(NamedKey::ArrowDown), Modifiers::META),
            Some(Command::Move(Motion::new(
                Granularity::Document,
                true,
                false
            )))
        );
        assert_eq!(
            mac(&named(NamedKey::Backspace), Modifiers::META),
            Some(Command::DeleteBackwards(Granularity::Paragraph))
        );
        assert_eq!(
            mac(&named(NamedKey::Backspace), Modifiers::ALT),
            Some(Command::DeleteBackwards(Granularity::Word))
        );
    }

    #[test]
    fn on_a_mac_control_runs_the_emacs_keys_and_types_nothing_else() {
        let mac = |event: &KeyEvent| command_on(Keymap::Mac, event, Modifiers::CONTROL);
        assert_eq!(
            mac(&controlled("a")),
            Some(Command::Move(Motion::new(
                Granularity::Paragraph,
                false,
                false
            )))
        );
        assert_eq!(
            mac(&controlled("e")),
            Some(Command::Move(Motion::new(
                Granularity::Paragraph,
                true,
                false
            )))
        );
        assert_eq!(
            mac(&controlled("k")),
            Some(Command::DeleteForwards(Granularity::Paragraph))
        );
        assert_eq!(mac(&controlled("q")), None);
    }

    #[test]
    fn elsewhere_control_still_leads_and_widens_an_arrow_to_a_word() {
        assert_eq!(
            command_on(Keymap::Standard, &character("a"), Modifiers::CONTROL),
            Some(Command::SelectAll)
        );
        assert_eq!(
            command_on(
                Keymap::Standard,
                &named(NamedKey::ArrowLeft),
                Modifiers::CONTROL
            ),
            Some(Command::Move(Motion::new(Granularity::Word, false, false)))
        );
    }
}
