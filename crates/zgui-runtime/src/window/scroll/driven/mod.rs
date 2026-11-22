//! Scrolling the compositor drives.
//!
//! When the compositor routes gestures itself, the window hands it every wheel and touchpad
//! gesture over a container with a scroll node, and runs no frame of its own while the content
//! moves. The compositor reports each offset it shows as an event. The window adopts the newest
//! one at the start of its next frame, and that frame's commit acknowledges it.
//!
//! The window wakes for one of three reasons:
//!
//! - The compositor is about to show content the window has not recorded. While a node moves, the
//!   window records a margin of content around its port, so it wakes once per margin and not per
//!   frame. A virtualised list that recycles its rows has recorded no rows past its own window,
//!   so its rows end the margin early and it wakes at the row that needs building.
//! - The node came to rest, so hit testing, accessibility and scroll listeners catch up.
//! - Input arrived, which is resolved against the offsets the compositor shows.

mod adopt;
mod hand;
mod margin;
mod tick;

use std::cell::Cell;

use compositor_types::{CompositorEvent, ScrollId};
use rustc_hash::FxHashMap;
use web_time::Instant;

use crate::window::Window;

/// The scrolling a window's compositor drives.
#[derive(Debug, Default)]
pub(crate) struct Driven {
    /// Whether the compositor routes gestures and moves scroll nodes.
    on: bool,
    /// The newest offset each node shows that the window has not adopted.
    shown: FxHashMap<ScrollId, Shown>,
    /// The nodes the compositor moves, and how far each may move before the window records more.
    active: FxHashMap<ScrollId, Active>,
    /// Who moves the content for the gesture in progress.
    owner: Option<Owner>,
    /// The element that scrolled under the pointer when the gesture in progress started.
    latched: Option<zgui_dom::NodeKey>,
    /// The rail and the speed of the touchpad gesture in progress.
    track: zgui_scroll::Track,
    /// Whether a frame that only presents the compositor's scrolling was asked for.
    tick_owed: Cell<bool>,
    /// Whether the compositor may be moving content, since a gesture was handed to it.
    moving: bool,
    /// When the compositor's motion needs its next frame.
    due: Option<Instant>,
    /// The buffer the compositor's events are taken into, kept for its allocation.
    events: Vec<CompositorEvent>,
}

/// The newest offset one node shows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    /// The offset, in device pixels.
    offset: [f32; 2],
    /// The serial of the event that reported it.
    serial: u64,
    /// Whether the node came to rest there.
    settled: bool,
}

/// One node the compositor moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Active {
    /// The offsets at which every pixel of the port is recorded, as `[low, high]` per axis, or
    /// `None` before the window recorded a margin.
    vouched: Option<[[f32; 2]; 2]>,
    /// The offset the node last showed.
    last: Option<[f32; 2]>,
    /// How far the node moved between its last two events.
    step: [f32; 2],
}

/// Who moves the content for a gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    /// The compositor, which routed the gesture to one of its scroll nodes.
    Compositor,
    /// The window, through its own scrolling.
    Window,
}

impl Driven {
    /// When the compositor's motion needs its next frame, if it does.
    pub(crate) fn due(&self) -> Option<Instant> {
        self.due
    }

    /// Notes that a whole frame started, which presents the compositor's scrolling too.
    pub(crate) fn frame_started(&mut self) {
        self.tick_owed.set(false);
        self.due = None;
    }
}

impl Window {
    /// Tells the compositor how to scroll this window's content on its own.
    ///
    /// A compositor that routes gestures scrolls every node with this desktop's scroll settings,
    /// and the frozen container under none. Any other compositor scrolls only by the window's
    /// writes.
    pub(crate) fn sync_scroll_drive(&mut self) {
        let on = self.compositor.capabilities().scroll_input;
        self.driven.on = on;
        let settings = self.scroll_settings();
        let drive = on.then_some(zgui_paint::ScrollDrive {
            kinetic: settings.wheel.framework_animates(),
            bounce: settings.elastic.admits_a_gesture(),
        });
        let frozen = self
            .frozen_container()
            .and_then(|container| self.layout.borrow().boxes_of(container).first().copied());
        if let Some(sync) = self.painter.compositor() {
            sync.set_scroll_drive(drive);
            sync.set_scroll_frozen(frozen);
        }
    }
}

impl Active {
    /// Reports whether the port at `offset` shows only recorded content.
    fn covers(&self, offset: [f32; 2]) -> bool {
        self.vouched.is_some_and(|vouched| {
            (0..2).all(|axis| {
                let [low, high] = vouched[axis];
                (low..=high).contains(&offset[axis])
            })
        })
    }
}
