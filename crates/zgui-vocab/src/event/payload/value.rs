//! A control's value changing.

use core::ops::Range;

use crate::event::kind::EventKind;
use crate::text::SharedString;

/// Whether a value change is provisional or settled.
///
/// The two exist because a field that reported only one of them would be wrong for half its uses.
/// A live search wants every keystroke; a form that validates on the server wants the value once
/// the user has stopped. Reporting both, distinctly, is what lets each ask for what it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueChange {
    /// The value changed as the user is working: every keystroke, every drag of a slider.
    Input,
    /// The user has settled on the value: focus left the field, the drag ended, a choice was made.
    Committed,
}

impl ValueChange {
    /// The kind of event this change is delivered as.
    pub const fn event_kind(self) -> EventKind {
        match self {
            Self::Input => EventKind::Input,
            Self::Committed => EventKind::Change,
        }
    }
}

/// What a value-change event carries.
///
/// The selection travels with the value because the two are read together: an editor reacting to
/// a change almost always needs to know where the caret ended up, and recovering that separately
/// means reading state that the change may already have invalidated.
///
/// This is also the payload of a value change requested from outside — an assistive technology
/// setting a field's text, or incrementing a slider — so a control that handles the event handles
/// both routes with one piece of code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueEvent {
    /// The control's value after the change.
    pub value: SharedString,
    /// Where the selection sits in the new value, in byte offsets.
    pub selection: Range<usize>,
    /// Whether the change is provisional or settled.
    pub kind: ValueChange,
}

impl ValueEvent {
    /// A change to `value` with the caret at its end.
    pub fn new(value: impl Into<SharedString>, kind: ValueChange) -> Self {
        let value = value.into();
        let end = value.len();
        Self {
            value,
            selection: end..end,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ValueChange, ValueEvent};
    use crate::event::kind::EventKind;

    #[test]
    fn the_two_kinds_are_two_events() {
        assert_eq!(ValueChange::Input.event_kind(), EventKind::Input);
        assert_eq!(ValueChange::Committed.event_kind(), EventKind::Change);
    }

    #[test]
    fn a_new_value_puts_the_caret_at_its_end() {
        let event = ValueEvent::new("hello", ValueChange::Input);
        assert_eq!(event.selection, 5..5);
        assert_eq!(event.value, "hello");
    }
}
