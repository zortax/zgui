//! Where a word starts and ends.
//!
//! There is no universal answer and pretending otherwise is how an editor ends up wrong for
//! everybody: an editor over prose stops at every punctuation mark, one over source code treats
//! `foo_bar` as one word and `->` as another, and a word processor takes the space with the word.
//! What is written here is the desktop convention every text field on the platform follows, stated
//! rather than inherited:
//!
//! * characters are word, punctuation, or space;
//! * a step forwards crosses the run the caret is standing in and then the space after it, so it
//!   lands on the start of the next word rather than beside it;
//! * a step backwards is its mirror: skip any space, then the run of one class.
//!
//! A component that needs a different rule computes its own boundaries and moves the selection to
//! them; nothing here is reached except through a word-granularity motion.

/// What kind of character something is, as far as word motion is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// Whitespace, which separates words and belongs to neither.
    Space,
    /// A letter, a digit, or an underscore.
    Word,
    /// Everything else.
    Punctuation,
}

/// Which class a character belongs to.
fn class(character: char) -> Class {
    if character.is_whitespace() {
        Class::Space
    } else if character.is_alphanumeric() || character == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// The offset at the start of the next word after `offset`.
pub fn next(text: &str, offset: usize) -> usize {
    let mut cursor = clamp(text, offset);
    let start = char_at(text, cursor).map(class);
    // Cross whatever the caret is standing in …
    while let Some(character) = char_at(text, cursor) {
        if start != Some(class(character)) {
            break;
        }
        cursor += character.len_utf8();
    }
    // … and then the space that follows it, so the caret lands on a word and not beside one.
    while let Some(character) = char_at(text, cursor) {
        if class(character) != Class::Space {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

/// The offset at the start of the word before `offset`.
pub fn previous(text: &str, offset: usize) -> usize {
    let mut cursor = clamp(text, offset);
    while let Some((back, character)) = char_before(text, cursor) {
        if class(character) != Class::Space {
            break;
        }
        cursor = back;
    }
    let Some((_, first)) = char_before(text, cursor) else {
        return cursor;
    };
    let wanted = class(first);
    while let Some((back, character)) = char_before(text, cursor) {
        if class(character) != wanted {
            break;
        }
        cursor = back;
    }
    cursor
}

/// The character starting at `offset`, if there is one.
fn char_at(text: &str, offset: usize) -> Option<char> {
    text.get(offset..).and_then(|rest| rest.chars().next())
}

/// The character before `offset`, and where it starts.
fn char_before(text: &str, offset: usize) -> Option<(usize, char)> {
    if offset == 0 {
        return None;
    }
    let mut back = offset - 1;
    while !text.is_char_boundary(back) {
        back -= 1;
    }
    char_at(text, back).map(|character| (back, character))
}

/// The nearest character boundary at or before `offset`.
fn clamp(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::{next, previous};

    #[test]
    fn a_step_forwards_lands_on_the_next_word_rather_than_the_space_before_it() {
        let text = "one two three";
        assert_eq!(next(text, 0), 4);
        assert_eq!(next(text, 4), 8);
        assert_eq!(next(text, 8), text.len());
    }

    #[test]
    fn a_step_backwards_lands_on_the_start_of_the_word_it_is_in_or_behind() {
        let text = "one two three";
        assert_eq!(previous(text, text.len()), 8);
        assert_eq!(previous(text, 8), 4);
        assert_eq!(previous(text, 6), 4, "from inside a word, its own start");
        assert_eq!(previous(text, 0), 0);
    }

    #[test]
    fn punctuation_is_its_own_run_and_is_not_swallowed_by_the_word_beside_it() {
        let text = "a->b";
        assert_eq!(next(text, 0), 1, "the letter, then the punctuation run");
        assert_eq!(next(text, 1), 3);
        assert_eq!(previous(text, 4), 3);
        assert_eq!(previous(text, 3), 1);
    }

    #[test]
    fn an_underscore_belongs_to_the_word_it_is_written_in() {
        let text = "foo_bar baz";
        assert_eq!(next(text, 0), 8, "one identifier, not two");
        assert_eq!(previous(text, 7), 0);
    }

    #[test]
    fn the_ends_of_the_text_are_fixed_points() {
        assert_eq!(next("", 0), 0);
        assert_eq!(previous("", 0), 0);
        assert_eq!(next("ab", 2), 2);
    }
}
