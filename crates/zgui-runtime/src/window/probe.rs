//! Asking a settled window what is under a point, without dispatching anything.
//!
//! Routing an event answers the same question and then acts on the answer: the pointer enters and
//! leaves elements, hover states are written, and a press starts a gesture. A caller that only
//! wants to know what is there — a fixture aiming at a control, a check that what is drawn and what
//! is hit are the same thing — must be able to ask without any of that happening, or the asking
//! changes the answer.

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point};

use crate::window::Window;

impl Window {
    /// Throws away what this window holds about where things are, and builds it again.
    ///
    /// The index that answers what is under a point is kept up to date one entry at a time, by the
    /// same pass that writes the fragments: an entry is moved when its fragment moves, and nothing
    /// rebuilds the whole of it unless the painting order itself changed. That is what makes it
    /// cheap, and it is also what makes "the answers are right" a claim about every frame since the
    /// window opened rather than about this one. This is the other way of arriving at it — one pass
    /// over the fragments as they are now, holding nothing from before — so the two can be held
    /// against each other.
    pub fn forget_hit_index(&mut self) {
        let layout = self.layout.borrow();
        self.hit.rebuild(&layout, self.scale);
    }

    /// The elements under a point on the surface, the document's root first.
    ///
    /// Empty when the point is over nothing at all, which for a window is the backdrop outside
    /// everything the document drew.
    ///
    /// The path rather than only the element: an event travels every step of it, so a difference
    /// three levels above the target is a difference in which handlers a press reaches even when
    /// the element under the pointer is the same one.
    ///
    /// Absolute device pixels, and the answer is against the frame that was last drawn — the same
    /// coordinate systems, resolved the same way, as the frame in front of the person pointing.
    pub fn chain_at(&self, point: Point<DevicePx, Device>) -> Vec<NodeKey> {
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let filter = self.engine.filter();
        let world = zgui_input::World {
            document: &document,
            layout: &layout,
            hit: &self.hit,
            clips: &self.scene.clips,
            spatial: &self.scene.spatial,
            scale: zgui_geom::Scale::new(self.scale),
            filter: &filter,
        };
        world.chain_at(point).path().to_vec()
    }
}
