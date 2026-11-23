//! Scrolling the compositor drives.
//!
//! When the compositor routes gestures itself, the window hands it every wheel and touchpad
//! gesture over a container with a scroll node, and runs no frame of its own while the content
//! moves. The compositor reports each offset it shows as an event. The window adopts the newest
//! one at the start of its next frame, and that frame's commit acknowledges it.
//!
//! The window wakes for one of four reasons:
//!
//! - The compositor is about to show content the window has not recorded. While a node moves, the
//!   window records a margin of content around its port, so it wakes once per margin and not per
//!   frame. A virtualised list that recycles its rows has recorded no rows past its own window,
//!   so its rows end the margin early and it wakes at the row that needs building.
//! - The node came to rest, so hit testing, accessibility and scroll listeners catch up.
//! - The content moved an element a `:hover` rule can match under the pointer or out from under
//!   it, so the hover lands one frame after the compositor showed the move.
//! - Input arrived, which is resolved against the offsets the compositor shows.

mod adopt;
mod animate;
mod hand;
mod hover;
mod margin;
mod tick;

use std::cell::Cell;

use compositor_types::{ScrollId, Sources};
use rustc_hash::FxHashMap;
use web_time::Instant;
use zgui_geom::{Device, Rect};

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
    /// When the driver latches its next frame on the window's thread.
    due: Option<Instant>,
    /// Whether a movement of the gesture in progress was handed to the compositor with a node
    /// predicted for it.
    routed: bool,
    /// The first movement of the gesture in progress handed with a node predicted for it, which
    /// decides who moves the gesture when the compositor routes it to no node.
    first_move: Option<compositor_types::ScrollGesture>,
    /// The gesture steps handed to the compositor since it last built a frame.
    handed: Vec<compositor_types::ScrollGesture>,
    /// Whether the client was registered again since those steps were handed, so the compositor
    /// lost them.
    lost: bool,
    /// Whether a `:hover` rule can match an element inside each container asked about since the
    /// window's last frame.
    sensitive: FxHashMap<zgui_dom::NodeKey, bool>,
    /// Whether the compositor's scrolling moved an element a `:hover` rule can match under the
    /// pointer or out from under it since the window's last frame.
    hover_owed: bool,
    /// The smooth scrolls handed to the compositor that the next frame writes, each with the
    /// offsets it runs from and to.
    animations: Vec<(ScrollId, [[f32; 2]; 2])>,
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
    /// The content the window's last frame recorded around the port, as `[low, high]` per axis
    /// in the content's coordinates.
    recorded: Option<margin::Span>,
    /// The device pixels that content and the port cover.
    reach: Option<Rect<i32, Device>>,
    /// How far the node moved between its last two events.
    step: [f32; 2],
    /// The index of the range in the node's watch that holds the element under the pointer.
    hover: Option<usize>,
}

/// Who moves the content for a gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    /// The compositor, which routes the gesture to one of its scroll nodes.
    Compositor,
    /// The window, through its own scrolling.
    Window,
}

impl Driven {
    /// When the driver latches its next frame on the window's thread, if it does.
    pub(crate) fn due(&self) -> Option<Instant> {
        self.due
    }

    /// Sets when the driver latches its next frame on the window's thread.
    pub(crate) fn arm(&mut self, due: Option<Instant>) {
        self.due = due;
    }

    /// Forgets every node, for a client registered again without them.
    ///
    /// The gesture in progress is kept, and the compositor routes its next steps to the nodes it
    /// is given again. The steps handed since the compositor's last frame are handed again before
    /// its next one.
    pub(crate) fn forget(&mut self) {
        self.shown.clear();
        self.lost = !self.handed.is_empty();
        self.active.clear();
        self.due = None;
    }

    /// Notes that a whole frame started, which presents the compositor's scrolling too.
    pub(crate) fn frame_started(&mut self) {
        self.tick_owed.set(false);
        self.due = None;
        self.sensitive.clear();
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
        let mut bounce = Sources::NONE;
        if settings.elastic.admits_a_detent() {
            bounce |= Sources::WHEEL | Sources::KEYBOARD;
        }
        if settings.elastic.admits_a_gesture() {
            bounce |= Sources::TOUCHPAD | Sources::TOUCH;
        }
        // A gesture stays on the container it latched onto, and a detent passes on.
        let drive = on.then_some(zgui_paint::ScrollDrive {
            kinetic: settings.wheel.framework_animates(),
            bounce,
            chain: Sources::WHEEL | Sources::KEYBOARD,
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
