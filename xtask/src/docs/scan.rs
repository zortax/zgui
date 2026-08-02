//! Reading words out of a line without a pattern-matching dependency.
//!
//! The gate runner has to build on a checkout where nothing else compiles yet, so it carries no
//! dependency it can do without. Every rule here needs the same two questions answered — does this
//! line use this word, and what follows it — so both are answered once, here.

/// A word is a run of alphanumeric characters, an underscore or a hyphen.
fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}

/// Walks the words of one line.
pub(crate) struct Cursor<'a> {
    /// What has not been read yet.
    rest: &'a str,
}

impl<'a> Cursor<'a> {
    /// Starts at the beginning of `line`.
    pub(crate) fn new(line: &'a str) -> Self {
        Self { rest: line }
    }

    /// The next word, and everything after it.
    pub(crate) fn next_word(&mut self) -> Option<(&'a str, &'a str)> {
        let start = self.rest.find(is_word_character)?;
        let rest = &self.rest[start..];
        let end = rest
            .find(|c: char| !is_word_character(c))
            .unwrap_or(rest.len());
        let (word, tail) = rest.split_at(end);
        self.rest = tail;
        Some((word, tail))
    }

    /// Everything after the next occurrence of `word`, compared without regard to case.
    pub(crate) fn after_word(&mut self, word: &str) -> Option<&'a str> {
        while let Some((candidate, tail)) = self.next_word() {
            if candidate.eq_ignore_ascii_case(word) {
                return Some(tail);
            }
        }
        None
    }
}

/// Whether `line` uses `word` as a whole word, compared without regard to case.
pub(crate) fn contains_word(line: &str, word: &str) -> bool {
    Cursor::new(line).after_word(word).is_some()
}

/// Whether `line` uses `word` as a whole word with a number after it, as in "phase 27".
pub(crate) fn word_then_digit(line: &str, word: &str) -> bool {
    let mut cursor = Cursor::new(line);
    while let Some(rest) = cursor.after_word(word) {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if trimmed.len() < rest.len() && trimmed.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{contains_word, word_then_digit};

    /// A whole word matches; the same letters inside a longer word do not.
    #[test]
    fn a_word_is_a_whole_word() {
        assert!(contains_word("the spike measured it", "spike"));
        assert!(contains_word("Spikes, plural", "spikes"));
        assert!(!contains_word("a spiked drink", "spike"));
        assert!(!contains_word("subspike", "spike"));
    }

    /// A number has to follow the word, with only spaces between.
    #[test]
    fn a_number_has_to_follow() {
        assert!(word_then_digit("landed in phase 27", "phase"));
        assert!(!word_then_digit("the last phase of the frame", "phase"));
        assert!(!word_then_digit("phase", "phase"));
        assert!(!word_then_digit("phase27", "phase"));
    }
}
