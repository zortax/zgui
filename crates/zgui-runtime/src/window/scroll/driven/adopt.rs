//! Taking over the offsets the compositor shows.

use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_scroll::Behavior;

use zgui_profile::{Counter, counter};

use crate::window::Window;

impl Window {
    /// Puts every container the compositor scrolled at the offset it shows.
    ///
    /// The frame's commit acknowledges each adopted event, which changes no pixel. A node that
    /// came to rest stops being one the compositor moves. When a container moved, the offsets are
    /// composed before input is routed and before the hover is moved, see
    /// [`Window::compose_for_input`].
    pub(crate) fn adopt_compositor_scroll(&mut self) {
        if self.adopt_offsets() {
            self.compose_for_input();
        }
        self.driven.hover_owed = false;
    }

    /// Puts every container the compositor scrolled at the offset it shows, and reports whether
    /// any moved.
    fn adopt_offsets(&mut self) -> bool {
        if self.driven.shown.is_empty() {
            return false;
        }
        let shown: Vec<_> = self.driven.shown.drain().collect();
        let mut moved = Vec::new();
        for (node, shown) in shown {
            if shown.settled {
                self.driven.active.remove(&node);
            }
            let Some(container) = self.element_of_scroll(node) else {
                self.driven.active.remove(&node);
                continue;
            };
            counter::bump(Counter::ScrollsAdopted);
            if let Some(sync) = self.painter.compositor() {
                sync.adopt_scroll(node, shown.serial);
            }
            let before = self.scroll.borrow().offset_of(container);
            let to = Point::new(DevicePx(shown.offset[0]), DevicePx(shown.offset[1]));
            {
                let layout = self.layout.borrow();
                self.scroll
                    .borrow_mut()
                    .scroll_to(&layout, container, to, Behavior::Instant);
            }
            if self.scroll.borrow().offset_of(container) != before {
                moved.push(container);
            }
        }
        if moved.is_empty() {
            return false;
        }
        let mut document = self.document.borrow_mut();
        zgui_scroll::mark::scrolled(&mut document, &moved);
        true
    }

    /// Returns the scroll containers the compositor moved since the window's last frame, each
    /// with the offset the compositor shows less the window's own offset.
    ///
    /// The window takes these offsets over at its next frame.
    pub fn scrolls_ahead(&self) -> Vec<(zgui_view::NodeId, Size<DevicePx, Device>)> {
        self.ahead()
            .into_iter()
            .map(|(container, ahead)| (zgui_view_dom::id::to_view(container), ahead))
            .collect()
    }

    /// Returns the containers of [`Window::scrolls_ahead`], by their document keys.
    pub(super) fn ahead(&self) -> Vec<(zgui_dom::NodeKey, Size<DevicePx, Device>)> {
        let Some(sync) = self.painter.compositor_ref() else {
            return Vec::new();
        };
        let layout = self.layout.borrow();
        let scroll = self.scroll.borrow();
        self.driven
            .shown
            .iter()
            .filter_map(|(node, shown)| {
                let container = layout.get(sync.scroll_container(*node)?)?.source?;
                let at = scroll.offset_of(container);
                let ahead = Size::new(
                    DevicePx(shown.offset[0] - at.x.0),
                    DevicePx(shown.offset[1] - at.y.0),
                );
                Some((container, ahead))
            })
            .collect()
    }

    /// Returns how far the compositor moves the content of `container` per frame, zero while it
    /// does not move it.
    pub(crate) fn scroll_motion(&self, container: zgui_dom::NodeKey) -> Size<DevicePx, Device> {
        let still = Size::new(DevicePx(0.0), DevicePx(0.0));
        let Some(sync) = self.painter.compositor_ref() else {
            return still;
        };
        let Some(first) = self.layout.borrow().boxes_of(container).first().copied() else {
            return still;
        };
        sync.scroll_node(&self.tables, first)
            .and_then(|node| self.driven.active.get(&node))
            .map_or(still, |active| {
                Size::new(DevicePx(active.step[0]), DevicePx(active.step[1]))
            })
    }

    /// Composes the adopted offsets before input is routed and before the hover is moved, so a
    /// hit test reads what the compositor shows.
    ///
    /// The hover moves here, before the frame's restyle, which then shows it.
    fn compose_for_input(&mut self) {
        let routes = self.queued.iter().any(|queued| {
            matches!(
                queued.event,
                zgui_platform::SurfaceEvent::Pointer { .. }
                    | zgui_platform::SurfaceEvent::Wheel { .. }
            )
        });
        let hovering = self.router.pointers().all().next().is_some();
        if !routes && !hovering {
            return;
        }
        self.restyle();
        self.update_text_brushes();
        self.build_boxes();
        self.lay_out();
        if hovering {
            self.rehit_if_moved();
        }
    }
}
