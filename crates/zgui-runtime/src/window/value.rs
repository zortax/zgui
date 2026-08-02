//! Announcing that an editable element's value changed.
//!
//! The editing model writes text into the document, and the document is not something a view
//! reads: a component binds to signals and is told about the world through events. Without this,
//! a field types perfectly and nothing above it can ever learn what it now holds — which is a
//! framework whose own text field cannot be bound to anything, and a component library forced to
//! keep a second, private copy of the text and the caret to work around it.
//!
//! Two events, because a field that reported only one of them would be wrong for half its uses. A
//! live search wants every keystroke; a form that validates on the server wants the value once the
//! user has stopped. So every edit announces
//! [`Input`](zgui_vocab::ValueChange::Input), and leaving a field that was edited announces
//! [`Committed`](zgui_vocab::ValueChange::Committed) exactly once.
//!
//! Both go down the ordinary capture, target and bubble path, so a wrapper that listens on an
//! ancestor hears a field's value change without the field knowing the wrapper is there.

use core::ops::Range;

use zgui_dom::NodeKey;
use zgui_vocab::{Payload, SharedString, Timestamp, ValueChange, ValueEvent};

use crate::window::Window;

impl Window {
    /// Announces that an edit changed an element's value.
    ///
    /// `selection` is where the caret or the selection ended up, when the edit moved it; the
    /// caret at the end of the new text otherwise, which is where an edit that reported no
    /// movement has left it.
    pub(crate) fn report_input(
        &mut self,
        node: NodeKey,
        value: String,
        selection: Option<Range<usize>>,
        timestamp: Timestamp,
    ) {
        self.report_value(node, value, selection, ValueChange::Input, timestamp);
    }

    /// Announces that the user has settled on an element's value, if they changed it at all.
    ///
    /// Called when focus leaves. A field that was only read reports nothing: "settled" is a claim
    /// about a change having happened, and a form that revalidated every time the user tabbed past
    /// a field would complain about text nobody touched.
    pub(crate) fn report_change(&mut self, node: NodeKey, timestamp: Timestamp) {
        let Some(value) = self.editors.settle(node) else {
            return;
        };
        self.report_value(node, value, None, ValueChange::Committed, timestamp);
    }

    /// Dispatches one value event at an element.
    fn report_value(
        &mut self,
        node: NodeKey,
        value: String,
        selection: Option<Range<usize>>,
        kind: ValueChange,
        timestamp: Timestamp,
    ) {
        let value = SharedString::from(value);
        // Clamped, because the range travels with the value and a listener slices one with the
        // other: an offset past the end is a panic in a handler that did nothing wrong.
        let end = value.len();
        let selection = selection
            .map(|range| {
                let start = range.start.min(end);
                start..range.end.clamp(start, end)
            })
            .unwrap_or(end..end);
        let payload = Payload::Value(ValueEvent {
            value,
            selection,
            kind,
        });
        self.dispatch_synthetic(node, kind.event_kind(), payload, timestamp);
    }
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{EventKind, ValueChange};

    #[test]
    fn the_two_changes_are_the_two_events_a_view_can_listen_for() {
        // The mapping this module relies on to pick which event to send. Written down here as
        // well, because a value change delivered as the wrong kind reaches listeners registered
        // for the other one and no assertion about the payload would notice.
        assert_eq!(ValueChange::Input.event_kind(), EventKind::Input);
        assert_eq!(ValueChange::Committed.event_kind(), EventKind::Change);
    }
}
