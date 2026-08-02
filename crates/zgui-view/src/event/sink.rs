//! Commands a handler issues, carried out after the dispatch it was issued in.

use zgui_vocab::EventKind;

use crate::id::NodeId;

/// Where a handler's commands go.
///
/// A handler runs while the document is mid-mutation, so a command that took effect immediately
/// would re-enter a mutation that has not finished. Everything a handler asks for is appended
/// here instead and carried out once the dispatch it was issued in has completed.
///
/// The runtime implements this. A test implements it to assert on what a component asked for.
///
/// ```
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use zgui_view::{DocumentId, EventSink, NodeId};
/// use zgui_vocab::EventKind;
///
/// #[derive(Default)]
/// struct Recorder(Vec<String>);
///
/// impl EventSink for Recorder {
///     fn capture_pointer(&mut self, node: NodeId) { self.0.push(format!("capture {node:?}")); }
///     fn release_pointer(&mut self, node: NodeId) { self.0.push(format!("release {node:?}")); }
///     fn request_focus(&mut self, node: NodeId) { self.0.push(format!("focus {node:?}")); }
///     fn synthesize(&mut self, node: NodeId, event: EventKind) {
///         self.0.push(format!("synthesize {}", event.name()));
///     }
/// }
///
/// let mut sink = Recorder::default();
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
/// sink.synthesize(node, EventKind::Click);
/// assert_eq!(sink.0, vec!["synthesize click".to_owned()]);
/// ```
pub trait EventSink {
    /// Routes every subsequent pointer event to `node` until the button is released, wherever the
    /// pointer goes.
    ///
    /// The capture is the framework's, not the operating system's: no portable pointer grab
    /// exists, and one that did would take the pointer away from the rest of the desktop.
    fn capture_pointer(&mut self, node: NodeId);

    /// Ends a capture early.
    fn release_pointer(&mut self, node: NodeId);

    /// Moves focus to `node`.
    fn request_focus(&mut self, node: NodeId);

    /// Dispatches `event` on `node` through the ordinary capture, target and bubble path.
    ///
    /// This is how keyboard activation of a custom control is written in one line, and it is the
    /// same path an inbound accessibility action takes, so a control written this way is operable
    /// by an assistive technology without any further work.
    fn synthesize(&mut self, node: NodeId, event: EventKind);
}

/// An [`EventSink`] that drops every command.
///
/// For building an [`EventCx`](crate::EventCx) where nothing is going to act on the commands
/// anyway — a unit test of a handler's signal writes, for instance.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiscardCommands;

impl EventSink for DiscardCommands {
    fn capture_pointer(&mut self, _node: NodeId) {}

    fn release_pointer(&mut self, _node: NodeId) {}

    fn request_focus(&mut self, _node: NodeId) {}

    fn synthesize(&mut self, _node: NodeId, _event: EventKind) {}
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{DiscardCommands, EventSink};

    #[test]
    fn the_trait_is_object_safe() {
        let sink: Rc<dyn EventSink> = Rc::new(DiscardCommands);
        assert_eq!(Rc::strong_count(&sink), 1);
    }
}
