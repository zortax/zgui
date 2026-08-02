//! Which slide is showing, which slides there are, and where each of them sits.

use zgui::geom::{Device, DevicePx, Rect};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui_ui_primitives::{Controllable, Orientation};

/// One slide, as the carousel knows it.
///
/// The name is handed out once and never changes; the geometry is asked afresh every frame it
/// moves, because a slide is only as wide as its own content and a carousel that assumed every
/// slide was the width of its viewport would step past a narrow one and stop short of a wide one.
#[derive(Copy, Clone)]
pub struct CarouselSlot {
    /// What the carousel calls this slide.
    id: u64,
    /// Its box in the window, as of the last completed layout.
    geometry: Signal<Option<Rect<DevicePx, Device>>, LocalStorage>,
}

impl CarouselSlot {
    /// What the carousel calls this slide.
    #[must_use]
    pub fn id(self) -> u64 {
        self.id
    }

    /// Where this slide's leading edge is in the window, along `orientation`.
    ///
    /// In the window rather than in the track, because that is the space geometry is observed in
    /// here. What anything does with it is a *difference* between two readings taken this way —
    /// which is the same number whether or not the track it is on has been translated, since a
    /// translation moves a track and its slides together.
    ///
    /// `None` until the slide has been laid out, which is every frame before the first.
    #[must_use]
    pub fn start(self, orientation: Orientation) -> Option<f32> {
        let box_ = self.geometry.get()?;
        Some(match orientation {
            Orientation::Vertical => box_.origin.y.0,
            _ => box_.origin.x.0,
        })
    }
}

/// What the slides and the arrows read to know where the carousel is.
///
/// `Copy`, so a handler stores one without cloning, and reachable from any depth with
/// [`CarouselContext::current`].
#[derive(Copy, Clone)]
pub struct CarouselContext {
    /// Which slide is showing, counted from zero.
    index: Controllable<usize>,
    /// One slot per slide, in the order they were written.
    slots: RwSignal<Vec<CarouselSlot>, LocalStorage>,
    /// The next name to hand out.
    next: RwSignal<u64, LocalStorage>,
    /// Which way the slides run.
    orientation: Orientation,
    /// Whether stepping past the last slide goes back to the first.
    wrap: bool,
}

impl CarouselContext {
    /// Wires a carousel up from what it was told about its own index.
    #[must_use]
    pub fn new(index: Controllable<usize>, orientation: Orientation, wrap: bool) -> Self {
        Self {
            index,
            slots: RwSignal::new_local(Vec::new()),
            next: RwSignal::new_local(1),
            orientation,
            wrap,
        }
    }

    /// Publishes this to every scope below the current one, and hands it back.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The carousel the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which slide is showing, counted from zero.
    #[must_use]
    pub fn index(self) -> usize {
        self.index.get()
    }

    /// How many slides there are.
    #[must_use]
    pub fn count(self) -> usize {
        self.slots.with(Vec::len)
    }

    /// Which way the slides run.
    #[must_use]
    pub fn orientation(self) -> Orientation {
        self.orientation
    }

    /// Whether there is a slide before this one to go to.
    #[must_use]
    pub fn has_previous(self) -> bool {
        self.wrap || self.index() > 0
    }

    /// Whether there is a slide after this one to go to.
    #[must_use]
    pub fn has_next(self) -> bool {
        let count = self.count();
        self.wrap || (count > 0 && self.index() + 1 < count)
    }

    /// Shows the slide before this one, wrapping when the carousel was told to.
    pub fn previous(self) {
        let count = self.count();
        if count == 0 {
            return;
        }
        let at = self.index.get_untracked();
        if at > 0 {
            self.index.set(at - 1);
        } else if self.wrap {
            self.index.set(count - 1);
        }
    }

    /// Shows the slide after this one, wrapping when the carousel was told to.
    pub fn next(self) {
        let count = self.count();
        if count == 0 {
            return;
        }
        let at = self.index.get_untracked();
        if at + 1 < count {
            self.index.set(at + 1);
        } else if self.wrap {
            self.index.set(0);
        }
    }

    /// Adds a slide at the end, and takes it out again when its scope goes away.
    ///
    /// The handle is the slide's own name; where it *is* is [`CarouselContext::position_of`],
    /// which is asked afresh every time, because a slide's position moves when anything before it
    /// comes or goes and a number captured once would be wrong from then on.
    ///
    /// `element` is watched from here on, because how far the track has to travel to bring this
    /// slide to the front is a question about where the slide actually ended up rather than about
    /// how many slides there are.
    #[must_use]
    pub fn register(self, element: NodeRef) -> u64 {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        let slot = CarouselSlot {
            id,
            geometry: element.observe_border_box(),
        };
        self.slots.update(|slots| slots.push(slot));
        on_cleanup_local(move || {
            self.slots
                .try_update(|slots| slots.retain(|slot| slot.id() != id));
        });
        id
    }

    /// Where the slide called `id` sits, counted from zero.
    #[must_use]
    pub fn position_of(self, id: u64) -> Option<usize> {
        self.slots
            .with(|slots| slots.iter().position(|slot| slot.id() == id))
    }

    /// Where the showing slide's leading edge is in the window, along the carousel's own axis.
    ///
    /// Taken against the track's own leading edge, this is the whole of the geometry: the track has
    /// to travel exactly that far for the showing slide to sit against the viewport's leading edge,
    /// whatever the slides are worth and however many of them a viewport holds at once.
    ///
    /// `None` before the slides have been laid out, and for an index no slide answers to.
    #[must_use]
    pub fn start_of_showing(self) -> Option<f32> {
        let at = self.index();
        let slot = self.slots.with(|slots| slots.get(at).copied())?;
        slot.start(self.orientation)
    }
}
