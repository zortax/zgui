//! Ordering a run of primitives as one group: one tree query and one leaf for the whole run.
//!
//! A line of text is a few dozen glyph sprites pushed one after another, side by side, in one
//! colour. Asked one at a time, the tree answers the same question for each glyph against the same
//! neighbourhood, and every answer files one more leaf. A run asks once: the glyphs are ordered
//! against each other locally, the run as a whole is filed as a block above whatever its union
//! overlaps, and each glyph takes the block's base plus its local order. This is the rule a
//! replayed chunk already follows, applied to content that is painted fresh.
//!
//! The result is coarse in the safe direction only. A glyph can come out higher than a query of
//! its own would have placed it, never lower, and two glyphs that overlap never share an order, so
//! the invariant the batching leans on holds.

use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder};
use crate::scene::Scene;

/// The orders a run hands out, one per primitive in pushing order.
#[derive(Debug, Default)]
pub(crate) struct RunOrders {
    /// The block's lowest order in the frame's tree, when the frame's tree orders the run.
    frame: Option<DrawOrder>,
    /// The block's lowest order in the open capture's tree, when a capture is open.
    capture: Option<DrawOrder>,
    /// Each primitive's order among the run's own, counting from one.
    local: Vec<DrawOrder>,
    /// How many primitives of the run were pushed so far.
    next: usize,
}

impl RunOrders {
    /// The frame order of the primitive about to be pushed, and a step to the next one.
    fn step(&mut self) -> Option<DrawOrder> {
        let local = self.local.get(self.next).copied();
        self.next += 1;
        Some(self.frame? + local? - 1)
    }

    /// The capture order of the primitive about to be pushed.
    fn peek_capture(&self) -> Option<DrawOrder> {
        Some(self.capture? + self.local.get(self.next).copied()? - 1)
    }
}

impl Scene {
    /// Orders the next `inks.len()` pushes as one run.
    ///
    /// `inks` are the primitives' rectangles in pushing order, `clip` and `space` the clip and the
    /// coordinate system all of them are pushed under. The pushes that follow must be those
    /// primitives, in that order; [`Scene::end_run`] closes the run. A push past the end of the
    /// run, or a run whose rectangles do not advance from left to right, is ordered one primitive
    /// at a time as before.
    pub fn begin_run(&mut self, inks: &[Rect<DevicePx, Device>], clip: ClipId, space: u32) {
        self.run = None;
        if inks.len() < 2 {
            return;
        }
        let placed: Vec<Rect<DevicePx, Device>> = inks
            .iter()
            .map(|ink| self.on_device(space, *ink))
            .collect();
        let Some(local) = local_orders(&placed) else {
            return;
        };
        let span = local.iter().copied().max().unwrap_or(1).saturating_sub(1);

        let frame = if self.layer_stack.is_empty() {
            let admitted = self.clips.bounds_in(clip, space);
            inks.iter()
                .filter_map(|ink| ink.intersection(admitted))
                .map(|clipped| self.on_device(space, clipped))
                .reduce(Rect::union)
                .map(|union| self.order.insert_block(union, span))
        } else {
            None
        };
        let capture = self.capture.is_some().then(|| {
            let union = placed
                .iter()
                .copied()
                .reduce(Rect::union)
                .expect("a run holds two rectangles at least");
            self.capture_order.insert_block(union, span)
        });
        self.run = Some(RunOrders {
            frame,
            capture,
            local,
            next: 0,
        });
    }

    /// Closes the run [`Scene::begin_run`] opened. Later pushes are ordered one at a time.
    pub fn end_run(&mut self) {
        self.run = None;
    }

    /// The frame order the run holds for the primitive being pushed now, and a step past it.
    ///
    /// Called once per push, culled or not, so the run's count stays with its primitives.
    pub(crate) fn run_order(&mut self) -> Option<DrawOrder> {
        self.run.as_mut().and_then(RunOrders::step)
    }

    /// The capture order the run holds for the primitive being pushed now.
    pub(crate) fn run_capture_order(&self) -> Option<DrawOrder> {
        self.run.as_ref().and_then(RunOrders::peek_capture)
    }
}

/// Each rectangle's order among `placed`, counting from one: one above the highest earlier
/// rectangle it overlaps.
///
/// Answers `None` when a rectangle starts left of the one before it. The scan back from each
/// rectangle stops where no earlier one can reach it, which is what keeps a long line linear.
fn local_orders(placed: &[Rect<DevicePx, Device>]) -> Option<Vec<DrawOrder>> {
    let mut local: Vec<DrawOrder> = Vec::with_capacity(placed.len());
    let mut widest = 0.0f32;
    for (at, rect) in placed.iter().enumerate() {
        if at > 0 && rect.left().0 < placed[at - 1].left().0 {
            return None;
        }
        let mut order = 1;
        for earlier in (0..at).rev() {
            let other = placed[earlier];
            if other.left().0 + widest < rect.left().0 {
                break;
            }
            if other.intersects(*rect) {
                order = order.max(local[earlier] + 1);
            }
        }
        widest = widest.max(rect.size.width.0);
        local.push(order);
    }
    Some(local)
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::local_orders;
    use crate::clip::ClipLink;
    use crate::id::ClipId;
    use crate::paint::PaintRef;
    use crate::prim::Quad;
    use crate::scene::Scene;
    use crate::spatial::SpatialId;

    fn rect(x: f32, width: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(10.0)),
        )
    }

    /// A scene over a small surface, with one solid paint interned.
    fn scene() -> (Scene, PaintRef) {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(400, 400));
        let id = scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
        (scene, PaintRef::solid(id))
    }

    /// Pushes `rects` as one run of quads under `clip`, and answers the orders they took.
    fn run(
        scene: &mut Scene,
        fill: PaintRef,
        rects: &[Rect<DevicePx, Device>],
        clip: ClipId,
    ) -> Vec<Option<u32>> {
        scene.begin_run(rects, clip, SpatialId::VIEWPORT.index());
        let orders = rects
            .iter()
            .map(|rect| scene.push_quad(Quad::filled(*rect, fill).clipped(clip)))
            .collect();
        scene.end_run();
        orders
    }

    #[test]
    fn a_run_sits_above_what_it_overlaps_and_its_disjoint_members_share_an_order() {
        let (mut scene, fill) = scene();
        let band = scene.push_quad(Quad::filled(
            Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(100.0), DevicePx(10.0)),
            ),
            fill,
        ));
        assert_eq!(band, Some(1));
        let orders = run(
            &mut scene,
            fill,
            &[rect(0.0, 8.0), rect(10.0, 8.0), rect(20.0, 8.0)],
            ClipId::ROOT,
        );
        assert_eq!(orders, [Some(2), Some(2), Some(2)]);
    }

    #[test]
    fn overlapping_members_take_distinct_orders_and_later_content_sorts_above_the_run() {
        let (mut scene, fill) = scene();
        let orders = run(
            &mut scene,
            fill,
            &[rect(0.0, 10.0), rect(8.0, 10.0), rect(30.0, 4.0)],
            ClipId::ROOT,
        );
        assert_eq!(orders, [Some(1), Some(2), Some(1)]);
        let caret = scene.push_quad(Quad::filled(rect(31.0, 1.0), fill));
        assert_eq!(caret, Some(3), "the run's leaf carries its highest order");
        let elsewhere = scene.push_quad(Quad::filled(rect(200.0, 10.0), fill));
        assert_eq!(elsewhere, Some(1), "content beside the run keeps the low order");
    }

    #[test]
    fn a_culled_member_keeps_the_run_counted() {
        let (mut scene, fill) = scene();
        let clip = scene.clips.only(ClipLink::rect(rect(0.0, 15.0)));
        let orders = run(
            &mut scene,
            fill,
            &[rect(0.0, 10.0), rect(20.0, 10.0), rect(8.0 + 20.0, 10.0)],
            clip,
        );
        assert_eq!(orders, [Some(1), None, None]);
        let after = scene.push_quad(Quad::filled(rect(40.0, 10.0), fill));
        assert_eq!(after, Some(1), "nothing past the run takes one of its orders");
    }

    #[test]
    fn a_run_that_steps_back_is_ordered_one_member_at_a_time() {
        let (mut scene, fill) = scene();
        let orders = run(
            &mut scene,
            fill,
            &[rect(10.0, 10.0), rect(0.0, 15.0)],
            ClipId::ROOT,
        );
        assert_eq!(
            orders,
            [Some(1), Some(2)],
            "a run that steps back is ordered one primitive at a time"
        );
    }

    #[test]
    fn a_run_files_one_leaf() {
        let (mut scene, fill) = scene();
        let rects: Vec<_> = (0..40).map(|at| rect(at as f32 * 10.0, 8.0)).collect();
        run(&mut scene, fill, &rects, ClipId::ROOT);
        assert_eq!(scene.order.len(), 1);
    }

    #[test]
    fn disjoint_rectangles_share_the_lowest_order() {
        let placed = [rect(0.0, 8.0), rect(10.0, 8.0), rect(20.0, 8.0)];
        assert_eq!(local_orders(&placed), Some(vec![1, 1, 1]));
    }

    #[test]
    fn a_rectangle_sorts_above_every_earlier_one_it_overlaps() {
        let placed = [rect(0.0, 10.0), rect(8.0, 10.0), rect(16.0, 10.0), rect(30.0, 4.0)];
        assert_eq!(local_orders(&placed), Some(vec![1, 2, 3, 1]));
    }

    #[test]
    fn a_wide_rectangle_is_reached_past_its_narrow_neighbours() {
        let placed = [rect(0.0, 40.0), rect(10.0, 2.0), rect(20.0, 2.0), rect(30.0, 2.0)];
        assert_eq!(local_orders(&placed), Some(vec![1, 2, 2, 2]));
    }

    #[test]
    fn a_run_that_steps_back_is_no_run() {
        let placed = [rect(10.0, 8.0), rect(0.0, 8.0)];
        assert_eq!(local_orders(&placed), None);
    }
}
