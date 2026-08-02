//! Carrying out what a press on a scrollbar asked for.
//!
//! Both answers are absolute positions rather than movements, and they are computed here rather
//! than by the input system for the reason a wheel's line height is: how far a screenful is, and
//! how far the content may go, belong to the container and are read from the frame that drew it.

use zgui_dom::NodeKey;
use zgui_geom::{DevicePx, Point};
use zgui_layout::Axis;
use zgui_scroll::Behavior;

use crate::window::Window;

impl Window {
    /// Puts one axis of a container at `to`, leaving the other where it is.
    ///
    /// What dragging a thumb asks for. The other axis is read back rather than assumed to be at the
    /// origin: a region scrolled sideways and then dragged down the vertical bar must not jump back
    /// to its left edge.
    pub(crate) fn scroll_along(&mut self, container: NodeKey, axis: Axis, to: f32) {
        let at = self.scroll.borrow().offset_of(container);
        let to = match axis {
            Axis::Vertical => Point::new(at.x, DevicePx(to)),
            Axis::Horizontal => Point::new(DevicePx(to), at.y),
        };
        self.place(container, to, Behavior::Instant);
    }

    /// Moves one axis of a container by one screenful, towards the end or towards the start.
    ///
    /// What a press on a track asks for. A screenful is the scrollport's own extent along that
    /// axis, which is what makes a track press a way of reading a document rather than a way of
    /// seeking in one.
    pub(crate) fn scroll_page(&mut self, container: NodeKey, axis: Axis, forward: bool) {
        let travel = {
            let layout = self.layout.borrow();
            zgui_layout::scroll_region::bar::live::travel_of(&layout, container, axis)
        };
        let Some(travel) = travel else {
            return;
        };
        let at = self.scroll.borrow().offset_of(container);
        let held = match axis {
            Axis::Vertical => at.y.0,
            Axis::Horizontal => at.x.0,
        };
        let by = if forward {
            travel.length
        } else {
            -travel.length
        };
        self.scroll_along(container, axis, (held + by).clamp(0.0, travel.limit));
    }
}
