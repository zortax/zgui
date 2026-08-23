//! What an input method is composing, and where to put its window.

use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ContentHint, ContentPurpose,
};
use zgui_platform::{TextInput, TextInputPurpose};
use zgui_vocab::ImeEvent;

/// What kind of text a field expects, in the protocol's own vocabulary.
///
/// The protocol enumerates more kinds than this framework does and every one of ours has a home in
/// it, so nothing is lost in this direction.
pub const fn purpose(purpose: TextInputPurpose) -> ContentPurpose {
    match purpose {
        TextInputPurpose::Password => ContentPurpose::Password,
        TextInputPurpose::Pin => ContentPurpose::Pin,
        TextInputPurpose::Number => ContentPurpose::Number,
        TextInputPurpose::Phone => ContentPurpose::Phone,
        TextInputPurpose::Url => ContentPurpose::Url,
        TextInputPurpose::Email => ContentPurpose::Email,
        _ => ContentPurpose::Normal,
    }
}

/// What the input method may do with what is typed here.
///
/// A secret gets neither completion, correction nor storage — and the last of those is the one
/// that cannot be undone: an input method that recorded a password into its history has published
/// it, and no later request takes it back.
pub fn hint(purpose: TextInputPurpose) -> ContentHint {
    if purpose.is_secret() {
        return ContentHint::HiddenText | ContentHint::SensitiveData;
    }
    match purpose {
        TextInputPurpose::Search | TextInputPurpose::Normal => {
            ContentHint::Completion | ContentHint::Spellcheck | ContentHint::AutoCapitalization
        }
        // An address and a name are text a person knows exactly and a dictionary does not, so
        // correcting them is worse than leaving them alone.
        TextInputPurpose::Url | TextInputPurpose::Email => ContentHint::None,
        _ => ContentHint::None,
    }
}

/// Where a candidate window should avoid, in whole surface-local pixels.
///
/// The rectangle is the *caret* rather than the field, so the candidate list appears beside the
/// insertion point. A rectangle of no extent is widened, because a caret is one pixel wide and a
/// compositor given a zero-width rectangle places the window at the surface's corner.
pub fn caret(state: &TextInput) -> (i32, i32, i32, i32) {
    (
        state.caret_origin.x.0.round() as i32,
        state.caret_origin.y.0.round() as i32,
        (state.caret_size.width.0.round() as i32).max(1),
        (state.caret_size.height.0.round() as i32).max(1),
    )
}

/// What one composition step means, once the compositor has said it is complete.
///
/// The protocol reports the parts of a change separately and then says the change is over; nothing
/// may be applied before then. So a step is accumulated and turned into events here, in the order
/// the protocol requires them to be applied: what was deleted, what was committed, then what is
/// still being composed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Composing {
    /// The provisional text, and where the caret sits inside it.
    pub preedit: Option<(String, Option<(i32, i32)>)>,
    /// The text that has become real.
    pub commit: Option<String>,
    /// How much of the surrounding text is to be deleted, before and after the caret.
    pub delete: Option<(u32, u32)>,
}

impl Composing {
    /// The events this step produces, in the order they must be applied.
    ///
    /// A commit and a preedit in one step is the ordinary case rather than a corner: an input
    /// method that accepts a syllable and starts another sends both, and applying them the other
    /// way round inserts the new composition before the accepted text.
    pub fn events(&self) -> Vec<ImeEvent> {
        let mut events = Vec::new();
        if let Some(text) = &self.commit {
            events.push(ImeEvent::Commit(text.as_str().into()));
        }
        match &self.preedit {
            Some((text, cursor)) => events.push(ImeEvent::Preedit {
                text: text.as_str().into(),
                cursor: cursor.and_then(|(from, to)| span(text, from, to)),
            }),
            // No provisional text after a step that had some is the composition being cleared,
            // which a field has to be told about or the old text stays on screen for ever.
            None => events.push(ImeEvent::Preedit {
                text: zgui_vocab::SharedString::from(""),
                cursor: None,
            }),
        }
        events
    }

    /// Whether this step says anything at all.
    pub const fn is_empty(&self) -> bool {
        self.preedit.is_none() && self.commit.is_none() && self.delete.is_none()
    }

    /// How much surrounding text this step asks to delete.
    pub const fn deletion(&self) -> Option<(u32, u32)> {
        self.delete
    }
}

/// The caret's span inside a provisional text, when the input method named one.
///
/// A value of -1 means the caret is hidden, and a span outside the text is a compositor or an
/// input method with a defect — both are answered with no span rather than with a panic on the
/// next slice.
fn span(text: &str, from: i32, to: i32) -> Option<std::ops::Range<usize>> {
    if from < 0 || to < 0 {
        return None;
    }
    let (from, to) = (from as usize, to as usize);
    let (start, end) = (from.min(to), from.max(to));
    (end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end))
        .then_some(start..end)
}

/// Whether a change to a field is worth telling the input method about.
///
/// The rectangle crosses the protocol as whole numbers, so two carets a fraction of a pixel apart
/// are one rectangle. Sending the same one again costs a round trip and, on some input methods,
/// a re-placed candidate window on every frame that touches the caret.
pub fn moved(before: Option<(i32, i32, i32, i32)>, now: (i32, i32, i32, i32)) -> bool {
    before != Some(now)
}

#[cfg(test)]
mod tests {
    use super::{Composing, caret, hint, moved, purpose, span};
    use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
        ContentHint, ContentPurpose,
    };
    use zgui_geom::{CssPx, Point, Size};
    use zgui_platform::{TextInput, TextInputPurpose};
    use zgui_vocab::ImeEvent;

    fn field() -> TextInput {
        TextInput::new(
            Point::new(CssPx(120.4), CssPx(48.6)),
            Size::new(CssPx(0.0), CssPx(16.0)),
        )
    }

    #[test]
    fn every_secret_purpose_reaches_the_input_method_as_a_secret() {
        // A password recorded into an input method's history has been published, and no later
        // request takes it back.
        assert_eq!(
            purpose(TextInputPurpose::Password),
            ContentPurpose::Password
        );
        assert_eq!(purpose(TextInputPurpose::Pin), ContentPurpose::Pin);
        for secret in [TextInputPurpose::Password, TextInputPurpose::Pin] {
            assert!(hint(secret).contains(ContentHint::SensitiveData));
            assert!(hint(secret).contains(ContentHint::HiddenText));
            assert!(!hint(secret).contains(ContentHint::Completion));
        }
    }

    #[test]
    fn an_ordinary_field_is_offered_the_help_a_person_expects() {
        let ordinary = hint(TextInputPurpose::Normal);
        assert!(ordinary.contains(ContentHint::Completion));
        assert!(ordinary.contains(ContentHint::Spellcheck));
    }

    #[test]
    fn an_address_is_not_corrected_against_a_dictionary() {
        assert_eq!(hint(TextInputPurpose::Url), ContentHint::None);
        assert_eq!(hint(TextInputPurpose::Email), ContentHint::None);
    }

    #[test]
    fn a_caret_of_no_width_is_widened_rather_than_sent_as_nothing() {
        // A compositor given a zero-width rectangle places the candidate window at the corner of
        // the surface, which is nowhere near what is being typed.
        assert_eq!(caret(&field()), (120, 49, 1, 16));
    }

    #[test]
    fn a_commit_is_applied_before_the_composition_that_followed_it() {
        // An input method accepting a syllable and starting another sends both in one step, and
        // the other order inserts the new composition in front of the accepted text.
        let step = Composing {
            commit: Some("字".to_owned()),
            preedit: Some(("か".to_owned(), Some((0, 3)))),
            delete: None,
        };
        let events = step.events();
        assert_eq!(events[0], ImeEvent::Commit("字".into()));
        assert!(matches!(events[1], ImeEvent::Preedit { .. }));
    }

    #[test]
    fn a_step_with_no_composition_clears_the_one_that_was_there() {
        // Without this the abandoned text stays on screen for ever.
        let cleared = Composing::default();
        assert_eq!(
            cleared.events(),
            vec![ImeEvent::Preedit {
                text: "".into(),
                cursor: None
            }]
        );
        assert!(cleared.is_empty());
    }

    #[test]
    fn a_hidden_caret_inside_a_composition_is_no_span_rather_than_a_wrong_one() {
        assert_eq!(span("にほん", -1, -1), None);
        assert_eq!(span("にほん", 0, 3), Some(0..3));
    }

    #[test]
    fn a_span_that_would_split_a_character_is_refused() {
        // Slicing there panics, and an input method or a compositor is free to be wrong.
        assert_eq!(span("にほん", 0, 2), None);
        assert_eq!(span("abc", 0, 99), None);
    }

    #[test]
    fn a_rectangle_that_has_not_moved_is_not_sent_again() {
        let at = caret(&field());
        assert!(moved(None, at));
        assert!(!moved(Some(at), at));
        assert!(moved(Some(at), (0, 0, 1, 1)));
    }
}
