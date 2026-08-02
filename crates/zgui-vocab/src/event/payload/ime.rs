//! Composition by an input method.

use core::ops::Range;

use crate::event::kind::EventKind;
use crate::text::SharedString;

/// What an input method is doing to the text being composed.
///
/// An input method turns several key presses into one piece of text — a Japanese reading into a
/// kanji, a Chinese phonetic spelling into a character — and while it is doing so the field shows
/// *provisional* text that is not yet part of its value. The four stages below are that lifecycle,
/// and an editor that treats provisional text as real produces a field that duplicates every
/// character the user composes.
///
/// While a composition is running the preedit range is authoritative and ordinary key events must
/// be ignored: the platform keeps delivering the keys the input method did not consume, and
/// letting an arrow key move the caret mid-composition makes the eventual commit land in the wrong
/// place.
///
/// ```
/// use zgui_vocab::ImeEvent;
///
/// let preedit = ImeEvent::Preedit { text: "にほん".into(), cursor: Some(3..3) };
/// assert!(preedit.is_composing());
/// assert!(!ImeEvent::Commit("日本".into()).is_composing());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImeEvent {
    /// The input method has taken over, and provisional text may follow.
    Enabled,
    /// The provisional text has changed.
    Preedit {
        /// The provisional text, which is not yet part of the field's value.
        text: SharedString,
        /// Where the caret sits inside the provisional text, in byte offsets.
        cursor: Option<Range<usize>>,
    },
    /// The composition finished and this text is now real.
    Commit(SharedString),
    /// The input method has let go, and any provisional text is abandoned.
    Disabled,
}

impl ImeEvent {
    /// Whether this event leaves a composition in progress.
    pub fn is_composing(&self) -> bool {
        matches!(self, Self::Enabled | Self::Preedit { .. })
    }

    /// The kind of event this is delivered as.
    pub const fn event_kind(&self) -> EventKind {
        match self {
            Self::Enabled => EventKind::ImeStart,
            Self::Preedit { .. } => EventKind::ImePreedit,
            Self::Commit(_) => EventKind::ImeCommit,
            Self::Disabled => EventKind::ImeEnd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ImeEvent;
    use crate::event::kind::EventKind;

    #[test]
    fn the_lifecycle_stages_map_to_four_distinct_event_kinds() {
        let kinds = [
            ImeEvent::Enabled.event_kind(),
            ImeEvent::Preedit {
                text: "a".into(),
                cursor: None,
            }
            .event_kind(),
            ImeEvent::Commit("a".into()).event_kind(),
            ImeEvent::Disabled.event_kind(),
        ];
        for (index, kind) in kinds.iter().enumerate() {
            assert!(!kinds[index + 1..].contains(kind));
        }
        assert_eq!(kinds[0], EventKind::ImeStart);
    }

    #[test]
    fn only_the_provisional_stages_count_as_composing() {
        assert!(ImeEvent::Enabled.is_composing());
        assert!(!ImeEvent::Commit("x".into()).is_composing());
        assert!(!ImeEvent::Disabled.is_composing());
    }
}
