//! Opening on a pointer that lingers, and closing on one that leaves.

use core::cell::Cell;
use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;

use crate::overlay::delay::Delayed;
use crate::overlay::state::OverlayState;

/// One showing surface at a time among the hover surfaces that share this.
///
/// Moving the pointer from one trigger to the next hands the showing over at once: the next
/// surface opens with no delay, and the one it replaces goes with no exit animation. Without it
/// the two surfaces overlap for the length of the old one's exit, which is two surfaces mounted
/// and animating for every crossing.
///
/// `Copy`, and reachable from any depth with [`Handoff::current`].
#[derive(Copy, Clone)]
pub struct Handoff {
    /// The surface that opened last, while it is the one showing.
    showing: StoredValue<Option<Showing>, LocalStorage>,
}

/// One surface in a [`Handoff`].
#[derive(Copy, Clone)]
struct Showing {
    /// Which surface this is.
    id: u64,
    /// Whether it is open.
    state: OverlayState,
    /// Whether its next close skips the exit animation.
    instant: RwSignal<bool, LocalStorage>,
}

impl Handoff {
    /// A group with nothing showing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            showing: StoredValue::new_local(None),
        }
    }

    /// Publishes this to every scope below the current one, and hands it back.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The group the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The surface that opened last, while its scope still stands.
    ///
    /// A trigger that went away — a row a list unmounted — takes its surface with it, and nothing
    /// tells the group; its signals are simply gone.
    fn last(self) -> Option<Showing> {
        let last = self.showing.try_get_value().flatten()?;
        if last.instant.try_get_untracked().is_none() {
            self.showing.try_set_value(None);
            return None;
        }
        Some(last)
    }

    /// Whether a surface other than `id` is showing.
    fn another_showing(self, id: u64) -> bool {
        self.last()
            .is_some_and(|other| other.id != id && other.state.is_open_untracked())
    }

    /// Makes `next` the showing surface, and takes the one it replaces down without an exit.
    ///
    /// The one it replaces may have closed already and still be on its way out, which is the
    /// ordinary case when the pointer leaves one trigger before it reaches the next: its exit is
    /// cut short too.
    fn take_over(self, next: Showing) {
        if let Some(other) = self.last()
            && other.id != next.id
        {
            other.instant.set(true);
            if other.state.is_open_untracked() {
                other.state.close();
            }
        }
        self.showing.try_set_value(Some(next));
    }
}

impl Default for Handoff {
    fn default() -> Self {
        Self::new()
    }
}

/// Opens `me`, taking over the showing in `handoff` when there is one.
fn show(me: Showing, handoff: Option<Handoff>) {
    me.instant.set(false);
    if let Some(group) = handoff {
        group.take_over(me);
    }
    me.state.open();
}

/// The next identity a [`HoverIntent`] takes in a [`Handoff`].
fn next_id() -> u64 {
    thread_local! {
        static NEXT: Cell<u64> = const { Cell::new(0) };
    }
    NEXT.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    })
}

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
    /// The group this surface hands the showing over in, when it is in one.
    handoff: Option<Handoff>,
    /// Which surface this is in its group.
    id: u64,
    /// Whether the surface's next close skips the exit animation.
    instant: RwSignal<bool, LocalStorage>,
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
            handoff: None,
            id: next_id(),
            instant: RwSignal::new_local(false),
        }
    }

    /// The same, handing the showing over in `handoff` when there is one.
    #[must_use]
    pub fn in_handoff(mut self, handoff: Option<Handoff>) -> Self {
        self.handoff = handoff;
        self
    }

    /// Whether the surface's next close skips the exit animation, as a signal to bind.
    ///
    /// True only for a surface a [`Handoff`] took down to show another one.
    #[must_use]
    pub fn instant(&self) -> Signal<bool, LocalStorage> {
        self.instant.into()
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
        let handoff = self.handoff;
        let me = Showing {
            id: self.id,
            state,
            instant: self.instant,
        };
        // A pointer moving from one surface's trigger to the next has already waited once.
        let delay = if handoff.is_some_and(|group| group.another_showing(me.id)) {
            Duration::ZERO
        } else {
            self.open_after
        };
        self.opening.after(delay, move || show(me, handoff));
    }

    /// Opens it now, whatever was pending.
    ///
    /// What focus from the keyboard does: a keyboard user reached the control on purpose.
    pub fn open_now(&self) {
        self.opening.cancel();
        self.closing.cancel();
        show(
            Showing {
                id: self.id,
                state: self.state,
                instant: self.instant,
            },
            self.handoff,
        );
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
