//! What an input method did to the text being composed.

use winit::event::Ime;
use winit::window::ImePurpose;
use zgui_platform::TextInputPurpose;
use zgui_vocab::ImeEvent;

/// One step of a composition.
///
/// The cursor inside a preedit is a byte range rather than a caret, because an input method
/// selects a span of what is being composed as often as it places a point in it — and a selection
/// collapsed to a point is expressible while a point widened to a selection is not.
pub(crate) fn event(ime: Ime) -> ImeEvent {
    match ime {
        Ime::Enabled => ImeEvent::Enabled,
        Ime::Preedit(text, cursor) => ImeEvent::Preedit {
            text: text.into(),
            cursor: cursor.map(|(start, end)| start..end),
        },
        Ime::Commit(text) => ImeEvent::Commit(text.into()),
        Ime::Disabled => ImeEvent::Disabled,
    }
}

/// What kind of text a field expects, in the platform's own vocabulary.
///
/// The platform offers three answers where this framework has eight, so the mapping is lossy in
/// one direction and honest about it: everything secret is a password, and everything else is
/// ordinary text. The two the platform does distinguish are the two that change its behaviour —
/// a password field must not be remembered or suggested — and inventing a distinction it does not
/// have would tell the input method something untrue.
pub(crate) const fn purpose(purpose: TextInputPurpose) -> ImePurpose {
    match purpose {
        TextInputPurpose::Password | TextInputPurpose::Pin => ImePurpose::Password,
        _ => ImePurpose::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::{event, purpose};
    use winit::event::Ime;
    use winit::window::ImePurpose;
    use zgui_platform::TextInputPurpose;
    use zgui_vocab::{EventKind, ImeEvent};

    #[test]
    fn every_step_of_a_composition_crosses_to_its_own_step() {
        assert_eq!(event(Ime::Enabled), ImeEvent::Enabled);
        assert_eq!(event(Ime::Disabled), ImeEvent::Disabled);
        assert_eq!(
            event(Ime::Commit("字".to_owned())),
            ImeEvent::Commit("字".into())
        );
    }

    #[test]
    fn a_preedit_keeps_the_span_the_input_method_selected() {
        let preedit = event(Ime::Preedit("にほん".to_owned(), Some((3, 9))));
        match preedit {
            ImeEvent::Preedit { text, cursor } => {
                assert_eq!(text.as_str(), "にほん");
                assert_eq!(cursor, Some(3..9));
            }
            other => panic!("a preedit crossed as {other:?}"),
        }
    }

    #[test]
    fn a_composition_that_is_still_going_says_so() {
        assert!(event(Ime::Enabled).is_composing());
        assert!(event(Ime::Preedit(String::new(), None)).is_composing());
        assert!(!event(Ime::Commit("a".to_owned())).is_composing());
        assert!(!event(Ime::Disabled).is_composing());
        assert_eq!(
            event(Ime::Commit("a".to_owned())).event_kind(),
            EventKind::ImeCommit
        );
    }

    #[test]
    fn every_secret_purpose_reaches_the_platform_as_a_secret() {
        // A password field whose purpose was reported as ordinary text is a password remembered by
        // the input method's history, which is the one failure here that cannot be undone.
        assert_eq!(purpose(TextInputPurpose::Password), ImePurpose::Password);
        assert_eq!(purpose(TextInputPurpose::Pin), ImePurpose::Password);
        assert_eq!(purpose(TextInputPurpose::Email), ImePurpose::Normal);
        assert_eq!(purpose(TextInputPurpose::Normal), ImePurpose::Normal);
    }
}
