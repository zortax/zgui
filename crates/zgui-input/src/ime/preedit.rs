//! Whether an input method has a composition in progress.

use zgui_vocab::ImeEvent;

/// The composition state, as the platform reports it.
///
/// This is not the editing model's composition — that one knows *where* the provisional text sits
/// in a document, which is a question about the text. This one answers the routing question: is an
/// input method in the middle of something right now? The frame loop asks it before it lets a key
/// event take a framework default, because a key that arrives during a composition belongs to the
/// composition even when the input method did not consume it.
///
/// ```
/// use zgui_input::ime::Preedit;
/// use zgui_vocab::ImeEvent;
///
/// let mut preedit = Preedit::default();
/// assert!(!preedit.is_active());
///
/// preedit.observe(&ImeEvent::Preedit { text: "に".into(), cursor: None });
/// assert!(preedit.is_active());
///
/// preedit.observe(&ImeEvent::Commit("日".into()));
/// assert!(!preedit.is_active(), "the keys are released again");
/// ```
///
/// # An empty preedit is not provisional text
///
/// Both Linux backends clear the provisional text before they commit, and clear it again when a
/// composition ends having produced nothing — the same event either way, and the second is never
/// followed by anything at all. Nothing is released too early by treating it as the end, because a
/// commit follows the first with no key in between; and treating it as a composition still running
/// would hold every key for the rest of the surface's life after the second.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// The provisional text, when there is any.
    text: Option<String>,
}

impl Preedit {
    /// Whether provisional text is on the screen right now.
    ///
    /// An input method being merely *enabled* is not a composition: a Japanese input method in
    /// direct mode is enabled all the time, and a field that refused keys whenever one was
    /// installed would never accept a letter. Neither is provisional text that is empty.
    pub fn is_active(&self) -> bool {
        self.text.as_deref().is_some_and(|text| !text.is_empty())
    }

    /// The provisional text.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Follows one step of a composition.
    pub fn observe(&mut self, event: &ImeEvent) {
        match event {
            ImeEvent::Preedit { text, .. } => self.text = Some(text.as_str().to_owned()),
            ImeEvent::Commit(_) | ImeEvent::Disabled => self.text = None,
            // Being enabled is not composing, and a stage this build has never heard of leaves the
            // composition exactly as it is rather than guessing that it ended.
            _ => {}
        }
    }

    /// Forgets any composition, which is what losing focus does.
    pub fn clear(&mut self) {
        self.text = None;
    }
}

#[cfg(test)]
mod tests {
    use zgui_vocab::ImeEvent;

    use super::Preedit;

    #[test]
    fn an_enabled_input_method_with_nothing_composed_is_not_composing() {
        let mut preedit = Preedit::default();
        preedit.observe(&ImeEvent::Enabled);
        assert!(
            !preedit.is_active(),
            "a field with an input method installed still takes ordinary keys"
        );
    }

    #[test]
    fn an_empty_preedit_shows_nothing_and_holds_nothing() {
        // The window system clears the provisional text before it commits *and* when a composition
        // ends having produced nothing, and it never says which. Holding the keys for the second
        // holds them for ever, because nothing follows it; releasing them at the first releases
        // nothing, because the commit is the very next event.
        let mut preedit = Preedit::default();
        preedit.observe(&ImeEvent::Preedit {
            text: "に".into(),
            cursor: None,
        });
        assert!(preedit.is_active());

        preedit.observe(&ImeEvent::Preedit {
            text: String::new().into(),
            cursor: None,
        });
        assert!(!preedit.is_active(), "there is nothing on the screen");
        assert_eq!(
            preedit.text(),
            Some(""),
            "and the input method still has a composition object, which is a separate question"
        );
    }

    #[test]
    fn losing_focus_ends_whatever_was_being_composed() {
        let mut preedit = Preedit::default();
        preedit.observe(&ImeEvent::Preedit {
            text: "に".into(),
            cursor: None,
        });
        preedit.clear();
        assert!(!preedit.is_active());
    }
}
