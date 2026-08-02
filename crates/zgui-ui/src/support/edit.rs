//! What a key does to the text of a field, as a pure function of the two.

use zgui::vocab::{Key, NamedKey};

/// One change to a field's text or to where its caret is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Edit {
    /// Put this text in at the caret.
    Insert(String),
    /// Take out the character before the caret.
    DeleteBefore,
    /// Take out the character after it.
    DeleteAfter,
    /// Move one character left.
    Left,
    /// Move one character right.
    Right,
    /// Go to the start.
    Start,
    /// Go to the end.
    End,
}

/// What `key` asks for, or nothing when the field does not claim it.
///
/// The space bar is the reason this asks the key what text it inserts rather than matching on
/// characters: space is a *named* key whose inserted text is one space, and a field that matched
/// characters alone would be one you cannot type a space into — while a field that claimed every
/// named key would swallow tab and trap the focus inside itself.
///
/// `multiline` decides only what <kbd>Enter</kbd> means. In a single-line field it belongs to
/// whatever is around it, which is what makes a form submit on Enter.
///
/// ```
/// use zgui::vocab::{Key, NamedKey};
/// use zgui_ui::support::{Edit, key_edit};
///
/// assert_eq!(key_edit(&Key::Named(NamedKey::Backspace), false), Some(Edit::DeleteBefore));
/// assert_eq!(
///     key_edit(&Key::Named(NamedKey::Space), false),
///     Some(Edit::Insert(" ".to_owned()))
/// );
/// assert_eq!(key_edit(&Key::Named(NamedKey::Enter), false), None);
/// assert_eq!(
///     key_edit(&Key::Named(NamedKey::Enter), true),
///     Some(Edit::Insert("\n".to_owned()))
/// );
/// assert_eq!(key_edit(&Key::Named(NamedKey::Tab), false), None);
/// ```
#[must_use]
pub fn key_edit(key: &Key, multiline: bool) -> Option<Edit> {
    match key {
        Key::Named(NamedKey::Backspace) => Some(Edit::DeleteBefore),
        Key::Named(NamedKey::Delete) => Some(Edit::DeleteAfter),
        Key::Named(NamedKey::ArrowLeft) => Some(Edit::Left),
        Key::Named(NamedKey::ArrowRight) => Some(Edit::Right),
        Key::Named(NamedKey::Home) => Some(Edit::Start),
        Key::Named(NamedKey::End) => Some(Edit::End),
        Key::Named(NamedKey::Enter) if multiline => Some(Edit::Insert("\n".to_owned())),
        Key::Named(NamedKey::Enter | NamedKey::Tab | NamedKey::Escape) => None,
        key => key
            .inserted_text()
            .map(|text| Edit::Insert(text.to_owned())),
    }
}

/// Carries out `edit` on `text` with its caret at `caret`, and reports where the caret ends up.
///
/// `caret` is a byte offset and every result is one too, always on a character boundary: an offset
/// inside a multi-byte character would panic the next time the string is sliced, which is a field
/// that works in English and crashes in Japanese.
///
/// ```
/// use zgui_ui::support::{Edit, apply};
///
/// let mut text = "aé".to_owned();
/// // The caret is past both characters, which is three bytes in.
/// let caret = apply(&mut text, 3, &Edit::DeleteBefore);
/// assert_eq!(text, "a");
/// assert_eq!(caret, 1);
/// ```
pub fn apply(text: &mut String, caret: usize, edit: &Edit) -> usize {
    let caret = caret.min(text.len());
    match edit {
        Edit::Insert(inserted) => {
            text.insert_str(caret, inserted);
            caret + inserted.len()
        }
        Edit::DeleteBefore => match before(text, caret) {
            Some(start) => {
                text.replace_range(start..caret, "");
                start
            }
            None => caret,
        },
        Edit::DeleteAfter => match after(text, caret) {
            Some(end) => {
                text.replace_range(caret..end, "");
                caret
            }
            None => caret,
        },
        Edit::Left => before(text, caret).unwrap_or(caret),
        Edit::Right => after(text, caret).unwrap_or(caret),
        Edit::Start => 0,
        Edit::End => text.len(),
    }
}

/// Where the character before `caret` starts.
fn before(text: &str, caret: usize) -> Option<usize> {
    text[..caret].char_indices().next_back().map(|(at, _)| at)
}

/// Where the character after `caret` ends.
fn after(text: &str, caret: usize) -> Option<usize> {
    text[caret..]
        .chars()
        .next()
        .map(|character| caret + character.len_utf8())
}

#[cfg(test)]
mod tests {
    use zgui::vocab::{Key, NamedKey};

    use super::{Edit, apply, key_edit};

    #[test]
    fn typing_at_the_end_appends_and_the_caret_follows() {
        let mut text = String::new();
        let mut caret = 0;
        for letter in ["h", "i"] {
            caret = apply(&mut text, caret, &Edit::Insert(letter.to_owned()));
        }
        assert_eq!(text, "hi");
        assert_eq!(caret, 2);
    }

    #[test]
    fn every_movement_lands_on_a_character_boundary() {
        // The test that matters for anything but ASCII: an offset inside a character panics the
        // next time the string is sliced, which is a crash nothing in an English test can produce.
        let mut text = "aéb漢".to_owned();
        let mut caret = 0;
        for _ in 0..6 {
            caret = apply(&mut text, caret, &Edit::Right);
            assert!(
                text.is_char_boundary(caret),
                "{caret} is inside a character"
            );
        }
        assert_eq!(caret, text.len());
        for _ in 0..6 {
            caret = apply(&mut text, caret, &Edit::Left);
            assert!(
                text.is_char_boundary(caret),
                "{caret} is inside a character"
            );
        }
        assert_eq!(caret, 0);
        assert_eq!(text, "aéb漢", "moving about changed the text");
    }

    #[test]
    fn deleting_at_either_end_does_nothing_rather_than_panicking() {
        let mut text = "x".to_owned();
        assert_eq!(apply(&mut text, 0, &Edit::DeleteBefore), 0);
        assert_eq!(text, "x");
        assert_eq!(apply(&mut text, 1, &Edit::DeleteAfter), 1);
        assert_eq!(text, "x");
    }

    #[test]
    fn a_caret_past_the_end_is_brought_back_rather_than_slicing_past_it() {
        let mut text = "ab".to_owned();
        assert_eq!(apply(&mut text, 99, &Edit::DeleteBefore), 1);
        assert_eq!(text, "a");
    }

    #[test]
    fn the_keys_a_field_leaves_alone_stay_alone() {
        for key in [NamedKey::Tab, NamedKey::Escape, NamedKey::F1] {
            assert_eq!(key_edit(&Key::Named(key), true), None, "{key:?}");
        }
    }
}
