//! What a presence is doing, as something a style sheet can select on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;

/// Where a [`Presence`](crate::Presence) is in its life.
///
/// The names are the ones a style sheet writes, because that is where the animation lives:
/// `[data-state="closed"]` is the selector an exit keyframe hangs off, and the whole design is
/// that a component author writes that rule rather than a duration in Rust.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PresenceState {
    /// Mounted and staying.
    Open,
    /// Mounted, on its way out, waiting for its exit animation to finish.
    Closed,
}

impl PresenceState {
    /// How this is written as an attribute value.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Whether the content is on its way out.
    pub const fn is_leaving(self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// What a [`Presence`](crate::Presence) publishes to the content it keeps mounted.
///
/// The content binds [`PresenceContext::state_name`] to a `data-state` attribute and writes its
/// enter and exit animations in CSS against it. Nothing here says how long anything takes.
#[derive(Copy, Clone)]
pub struct PresenceContext {
    /// What the presence is doing.
    state: Signal<PresenceState, LocalStorage>,
}

impl PresenceContext {
    /// Wraps a state signal. [`Presence`](crate::Presence) is what calls this.
    pub fn new(state: Signal<PresenceState, LocalStorage>) -> Self {
        Self { state }
    }

    /// What the presence is doing now.
    pub fn state(&self) -> PresenceState {
        self.state.get()
    }

    /// The same, as the attribute value a style sheet selects on.
    pub fn state_name(&self) -> &'static str {
        self.state.get().name()
    }

    /// The enclosing presence, when there is one.
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }
}

/// What the enclosing [`Presence`](crate::Presence) is doing, when there is one.
///
/// `None` where the content is not inside one, which is an ordinary answer: a surface used without
/// an exit animation is simply mounted and unmounted, and binds nothing.
pub fn use_presence() -> Option<PresenceContext> {
    PresenceContext::current()
}

#[cfg(test)]
mod tests {
    use super::PresenceState;

    #[test]
    fn the_two_states_are_the_two_a_style_sheet_selects_on() {
        assert_eq!(PresenceState::Open.name(), "open");
        assert_eq!(PresenceState::Closed.name(), "closed");
        assert!(PresenceState::Closed.is_leaving());
        assert!(!PresenceState::Open.is_leaving());
    }
}
