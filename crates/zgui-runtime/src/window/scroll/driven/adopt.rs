//! Taking over the offsets the compositor shows.

use zgui_geom::{DevicePx, Point};
use zgui_scroll::Behavior;

use crate::window::Window;

impl Window {
    /// Puts every container the compositor scrolled at the offset it shows, and reports whether
    /// any moved.
    ///
    /// The frame's commit acknowledges each adopted event, which changes no pixel. A node that
    /// came to rest stops being one the compositor moves.
    pub(crate) fn adopt_compositor_scroll(&mut self) -> bool {
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
            zgui_profile::counter::bump(zgui_profile::Counter::ScrollsAdopted);
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

    /// Composes the adopted offsets before input is routed, so a hit test reads what the
    /// compositor shows.
    pub(crate) fn compose_for_input(&mut self) {
        let routes = self.queued.iter().any(|queued| {
            matches!(
                queued.event,
                zgui_platform::SurfaceEvent::Pointer { .. } | zgui_platform::SurfaceEvent::Wheel { .. }
            )
        });
        if !routes {
            return;
        }
        self.restyle();
        self.update_text_brushes();
        self.build_boxes();
        self.lay_out();
    }
}
