//! Text arriving from the keyboard after the layout has had its say.

use crate::text::SharedString;

/// Text produced by the keyboard, ready to insert.
///
/// A key event and a text event are not the same thing and neither can be derived from the other.
/// One press can produce several characters, several presses can produce one, and a press that
/// starts an accent produces none at all until the next one arrives. So text insertion listens for
/// this, and commands listen for key events, and neither has to reason about the other's case.
///
/// ```
/// use zgui_vocab::TextEvent;
///
/// let event = TextEvent::new("é");
/// assert_eq!(event.text.as_str(), "é");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEvent {
    /// The text to insert at the caret.
    pub text: SharedString,
}

impl TextEvent {
    /// A text event carrying `text`.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::TextEvent;

    #[test]
    fn text_is_carried_verbatim() {
        assert_eq!(TextEvent::new("ß").text.as_str(), "ß");
    }
}
