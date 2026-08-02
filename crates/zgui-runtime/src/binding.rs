//! The seam a downstream script engine attaches to.
//!
//! The document carries no scripting language, and the hooks a scripting language needs are
//! frame-loop concepts rather than document ones: a callback that runs before the next frame is
//! painted, a checkpoint at which queued work is drained, and a chance to see an event before the
//! ordinary listeners do. All three are questions about *when*, and only the loop knows when.
//!
//! Nothing in this framework implements them. They exist so that somebody building a browser, or
//! an application with an embedded scripting language, has one object to install rather than a
//! fork of the loop.

use zgui_view::NodeId;
use zgui_vocab::EventKind;

/// What a frame tells a downstream script engine.
///
/// Every method has a do-nothing default, so an implementation states only the hooks it wants. The
/// order within a frame is fixed and is the whole content of the contract:
///
/// 1. [`HostBinding::before_dispatch`], once per event, before any listener runs;
/// 2. [`HostBinding::checkpoint`], after the reactive work of the frame has settled;
/// 3. [`HostBinding::before_paint`], after layout and before anything is emitted, which is where
///    an animation-frame callback belongs — early enough that what it writes is painted in this
///    frame rather than the next.
pub trait HostBinding {
    /// An event is about to be dispatched, before the first listener on its path.
    ///
    /// Answering `false` means the event is not dispatched at all, which is what an engine
    /// intercepting an event for its own dispatcher does.
    fn before_dispatch(&mut self, target: Option<NodeId>, event: EventKind) -> bool {
        let _ = (target, event);
        true
    }

    /// The frame's reactive work has settled.
    ///
    /// Where queued work belonging to the embedded engine is drained, so that anything it writes
    /// is picked up by the same frame's restyle.
    fn checkpoint(&mut self) {}

    /// Layout has settled and nothing has been emitted yet.
    ///
    /// The frame's timestamp is handed over so that a callback measuring elapsed time reads the
    /// clock the rest of the frame read, rather than a second one that disagrees with it.
    fn before_paint(&mut self, timestamp: zgui_vocab::Timestamp) {
        let _ = timestamp;
    }

    /// The window this binding was installed on is going away.
    fn shutting_down(&mut self) {}
}

/// The binding installed when nothing has been installed.
///
/// Every method is the default, so a window with no script engine costs three calls that compile
/// to nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoBinding;

impl HostBinding for NoBinding {}

#[cfg(test)]
mod tests {
    use super::{HostBinding, NoBinding};
    use zgui_vocab::EventKind;

    /// A binding that refuses every event and counts its checkpoints.
    #[derive(Default)]
    struct Intercepting {
        checkpoints: u32,
    }

    impl HostBinding for Intercepting {
        fn before_dispatch(
            &mut self,
            _target: Option<zgui_view::NodeId>,
            _event: EventKind,
        ) -> bool {
            false
        }

        fn checkpoint(&mut self) {
            self.checkpoints += 1;
        }
    }

    #[test]
    fn the_empty_binding_lets_everything_through() {
        let mut binding = NoBinding;
        assert!(binding.before_dispatch(None, EventKind::Click));
        binding.checkpoint();
        binding.before_paint(zgui_vocab::Timestamp::ORIGIN);
    }

    #[test]
    fn a_binding_can_take_an_event_over_entirely() {
        let mut binding = Intercepting::default();
        assert!(!binding.before_dispatch(None, EventKind::Click));
        binding.checkpoint();
        assert_eq!(binding.checkpoints, 1);
    }

    #[test]
    fn the_seam_is_usable_behind_a_pointer() {
        let mut binding: Box<dyn HostBinding> = Box::new(NoBinding);
        assert!(binding.before_dispatch(None, EventKind::KeyDown));
    }
}
