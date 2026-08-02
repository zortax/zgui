//! Opening on a pointer that lingers, and closing on one that leaves.

use core::time::Duration;

use zgui::prelude::*;

use crate::overlay::delay::Delayed;
use crate::overlay::state::OverlayState;

/// A surface that opens when the pointer stays, and closes when it goes.
///
/// Two delays, and neither is decoration. Without the opening one, dragging the pointer across a
/// toolbar raises and drops eight tooltips on the way past. Without the closing one, the surface
/// vanishes in the gap between the trigger and itself, so a hover card can never be reached — and
/// nothing in it can ever be read, let alone clicked.
///
/// Both are cancellable and at most one of each is ever pending, so a pointer that leaves and
/// comes back does not open twice, and one that arrives during the closing delay simply stays.
///
/// ```
/// use core::time::Duration;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::overlay::{HoverIntent, OverlayState};
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     // No delay at all: the surface is up in the frame the pointer arrived.
///     let intent = HoverIntent::new(
///         OverlayState::uncontrolled(false, None),
///         Duration::ZERO,
///         Duration::ZERO,
///     );
///     intent.enter();
///     assert!(intent.state().is_open_untracked());
///     intent.leave();
///     assert!(!intent.state().is_open_untracked());
/// });
/// scope.unmount();
/// ```
#[derive(Clone)]
pub struct HoverIntent {
    /// The surface being opened and closed.
    state: OverlayState,
    /// How long the pointer has to stay before it opens.
    open_after: Duration,
    /// How long it stays open after the pointer leaves.
    close_after: Duration,
    /// The pending open.
    opening: Delayed,
    /// The pending close.
    closing: Delayed,
}

impl HoverIntent {
    /// Wires an overlay up to two delays.
    #[must_use]
    pub fn new(state: OverlayState, open_after: Duration, close_after: Duration) -> Self {
        Self {
            state,
            open_after,
            close_after,
            opening: Delayed::new(),
            closing: Delayed::new(),
        }
    }

    /// Publishes this to every scope below the current one, and hands it back.
    pub fn provide(self) -> Self {
        provide_local_context(self.clone());
        self
    }

    /// The intent the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The surface it opens and closes.
    #[must_use]
    pub fn state(&self) -> OverlayState {
        self.state
    }

    /// The pointer arrived, or focus did: open, once it has stayed long enough.
    pub fn enter(&self) {
        self.closing.cancel();
        let state = self.state;
        self.opening.after(self.open_after, move || state.open());
    }

    /// The pointer left, or focus did: close, unless it comes back first.
    pub fn leave(&self) {
        self.opening.cancel();
        let state = self.state;
        self.closing.after(self.close_after, move || state.close());
    }

    /// Closes it now, whatever was pending.
    ///
    /// What <kbd>Escape</kbd> and a press do: a delay is for a pointer that might not have meant
    /// it, and a key press means it.
    pub fn close_now(&self) {
        self.opening.cancel();
        self.closing.cancel();
        self.state.close();
    }
}
