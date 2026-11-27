//! Smooth scrolls the compositor animates.

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point};
use zgui_paint::sync::CompositorSync;
use zgui_scroll::motion::Tween;

use crate::window::Window;
use crate::window::scroll::driven::Driven;

/// How many steps the curve of a smooth scroll the compositor animates is drawn with.
const CURVE_STEPS: u16 = 16;

impl Window {
    /// Hands a smooth scroll of `container` to `to` to the compositor, and reports whether it took
    /// it.
    ///
    /// The compositor takes it when it animates and draws the content of `container` through a
    /// scroll node. The scroll starts where the compositor shows the node, and the window adopts
    /// the offsets it moves through as it adopts any other motion of the compositor's.
    pub(crate) fn animate_on_compositor(
        &mut self,
        container: NodeKey,
        to: Point<DevicePx, Device>,
    ) -> bool {
        if !self.compositor.capabilities().animations {
            return false;
        }
        let Some(node) = self.scroll_node_of(container) else {
            return false;
        };
        let clamped = {
            let layout = self.layout.borrow();
            self.scroll.borrow().clamped(&layout, container, to)
        };
        let Some(to) = clamped else {
            return false;
        };
        let held = self.scroll.borrow().offset_of(container);
        let from = self
            .driven
            .shown
            .get(&node)
            .map_or([held.x.0, held.y.0], |shown| shown.offset);
        let to = [to.x.0, to.y.0];
        if from != to {
            self.driven.animations.push((node, [from, to]));
            self.moving(node);
            self.request_frame();
        }
        true
    }
}

impl Driven {
    /// Writes the smooth scrolls handed over since the last frame into the transaction `sync`
    /// has open.
    pub(crate) fn write_animations(&mut self, sync: &mut CompositorSync) {
        if self.animations.is_empty() {
            return;
        }
        let curve: Vec<[f32; 2]> = (0..=CURVE_STEPS)
            .map(|step| {
                let progress = f32::from(step) / f32::from(CURVE_STEPS);
                [progress, Tween::eased(progress)]
            })
            .collect();
        for (node, span) in self.animations.drain(..) {
            sync.animate_scroll(node, span, Tween::DURATION, &curve);
        }
    }
}
