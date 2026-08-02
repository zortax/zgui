//! Where every container is scrolled to, and everything that writes one.
//!
//! Two offsets are kept per container and they are not the same number. The **clamped** offset is
//! what the container is actually scrolled to: it is what a listener is told, what an observation
//! delivers and what a scrollbar thumb is a function of, and it never leaves the range the content
//! allows. The **composed** offset is the clamped one plus whatever elastic displacement a gesture
//! is holding past the end, and it is what the fragment pass reads. Collapsing the two would make a
//! finger held past the bottom of a list report a scroll position that does not exist.
//!
//! Both are clamped against numbers this crate does not own, and those numbers move without any
//! offset being written: a window that is resized changes the scrollport, a document that reflows
//! changes the content, and a surface that changes its device pixel ratio changes the unit both are
//! measured in. What a held offset owes at each of those moments is [`extent`](self), and the rule
//! it states — clamp when the end moved past you, never move a reader who is still inside the
//! document — is the one an application author has to be able to reason about.

mod apply;
pub mod extent;
mod glide;
mod motion;

use rustc_hash::FxHashMap;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::ScrollOffsets;

use crate::elastic::Overscroll;
use crate::motion::Motion;
use crate::report::Scrolled;

/// Every container's scroll offset, and the motions carrying some of them.
///
/// Everything here is filed under the **element** that scrolls, never under a box it generated.
/// A box tree is rebuilt whole whenever any element anywhere in the document gains or loses a box,
/// and every key in it changes when that happens — so an offset filed under a box lasts until the
/// first tooltip, menu or hover-revealed control, and then the container silently returns to the
/// top. See [`zgui_layout::scroll_region::ScrollOffsets`] for the whole of that argument.
///
/// ```
/// use zgui_scroll::Scroller;
///
/// let scroller = Scroller::new();
/// assert!(scroller.composed().is_empty(), "an untouched document is at the origin throughout");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Scroller {
    /// Where each container is scrolled to, clamped to what its content allows.
    offsets: ScrollOffsets,
    /// The same with any elastic displacement added, which is what the fragment pass composes.
    composed: ScrollOffsets,
    /// How far past its end each container is being held, and how fast it is springing back.
    elastic: FxHashMap<NodeKey, Overscroll>,
    /// The containers whose offsets are moving on their own.
    motions: FxHashMap<NodeKey, Motion>,
    /// What has moved since the last drain.
    moved: Vec<Scrolled>,
}

impl Scroller {
    /// Nothing scrolled, nothing moving.
    pub fn new() -> Self {
        Self::default()
    }

    /// What the fragment pass composes positions against.
    ///
    /// The clamped offsets with any elastic displacement added, which is deliberately not what
    /// [`Scroller::offset_of`] answers: the fragment pass draws the content where the gesture is
    /// holding it, and everything else reports where the container is actually scrolled to.
    pub fn composed(&self) -> &ScrollOffsets {
        &self.composed
    }

    /// Where one container is scrolled to, as everything but the fragment pass sees it.
    pub fn offset_of(&self, container: NodeKey) -> Point<DevicePx, Device> {
        self.offsets.of(container)
    }

    /// How far past its end one container is being held.
    pub fn elastic_of(&self, container: NodeKey) -> Size<DevicePx, Device> {
        self.overscroll_of(container).held()
    }

    /// The whole state of one container's displaced edge, speed included.
    pub(crate) fn overscroll_of(&self, container: NodeKey) -> Overscroll {
        self.elastic.get(&container).copied().unwrap_or_default()
    }

    /// Whether anything is moving on its own, so that the next frame has a deadline.
    ///
    /// Both a running motion and a displacement waiting to relax count. Leaving the second out is
    /// a list that stays stretched past its end until something else happens to ask for a frame.
    pub fn is_animating(&self) -> bool {
        !self.motions.is_empty() || !self.elastic.is_empty()
    }

    /// Whether nothing is displaced past its end.
    pub fn settled(&self) -> bool {
        self.elastic.is_empty()
    }

    /// Everything that has moved since this was last called.
    pub fn take_moved(&mut self) -> Vec<Scrolled> {
        core::mem::take(&mut self.moved)
    }

    /// Records that `container` moved from `from` to `to`.
    ///
    /// Coalesced per container: a frame that moved one container three times reports one move,
    /// from where it started to where it ended, because three `scroll` events for one frame is
    /// three effect re-runs for one visible change.
    fn record(
        &mut self,
        container: NodeKey,
        from: Point<DevicePx, Device>,
        to: Point<DevicePx, Device>,
    ) {
        if let Some(held) = self
            .moved
            .iter_mut()
            .find(|held| held.container == container)
        {
            held.to = to;
            return;
        }
        self.moved.push(Scrolled {
            container,
            from,
            to,
        });
    }

    /// Rewrites one container's composed offset from its clamped one and its displacement.
    ///
    /// The two halves are put on the device grid differently, and the difference is the whole of
    /// this function.
    ///
    /// **The clamped offset is snapped.** A scroll carried to its destination over several frames
    /// passes through fractional offsets, and where a box's *content* is broken and measured
    /// depends on where the box starts — so a scrollport whose contents begin a third of a pixel
    /// further along is a scrollport whose pieces round to the grid differently, and the paint
    /// stage can only answer that by encoding them again. A smooth scroll of a long list would
    /// re-encode a slice of the document on every frame of every detent, for a difference nobody
    /// can see. Snapping costs the picture nothing, because the grid is where it was going to land
    /// anyway, and leaves what a listener, a scrollbar and a virtualiser read exact.
    ///
    /// **The elastic displacement is not**, for three separate reasons, and rounding it is the
    /// difference between a bounce that glides and a bounce that ratchets.
    ///
    /// It is *not a scroll position*: it is a rigid translation of a whole scrollport, added after
    /// the grid snap, taken equally by both edges of every box below it. No size changes, nothing
    /// is broken anywhere else, and the offsetting path composes it exactly at any fraction — see
    /// [`Fragment::subtree_rigid`](zgui_layout::Fragment::subtree_rigid).
    ///
    /// It is *slower than a pixel per frame for most of its length*. A return covers the band in
    /// about a third of a second, which on a two-hundred-and-forty hertz output is around eighty
    /// frames for a hundred and twenty pixels — and it decelerates, so the second half of the
    /// return moves well under a pixel per frame. Rounded, the edge then stands still for a frame
    /// or two and jumps, which is exactly the stutter the spring exists to avoid: the frames are
    /// all there and half of them draw the picture the last one drew.
    ///
    /// And rounding it *costs more than not rounding it*. A frame in which the spring moved but the
    /// rounded sum did not still marks the container scrolled, so its subtree is composed again
    /// rather than translated — and a jump of a whole pixel re-encodes what a fraction of one
    /// translates.
    fn compose(&mut self, container: NodeKey) {
        let base = self.offsets.of(container);
        let held = self.elastic_of(container);
        self.composed.place(
            container,
            Point::new(
                DevicePx(base.x.0.round() + held.width.0),
                DevicePx(base.y.0.round() + held.height.0),
            ),
        );
    }

    /// The motion carrying one container, if one is.
    fn motion_of(&self, container: NodeKey) -> Option<&Motion> {
        self.motions.get(&container)
    }

    /// The same, to be changed in place.
    fn motion_mut(&mut self, container: NodeKey) -> Option<&mut Motion> {
        self.motions.get_mut(&container)
    }

    /// Puts one container on a motion, replacing whatever was carrying it.
    fn install(&mut self, container: NodeKey, motion: Motion) {
        self.motions.insert(container, motion);
    }

    /// How far one container may be scrolled, which is zero for one that does not scroll.
    fn limit_for(&self, store: &LayoutStore, container: NodeKey) -> Point<DevicePx, Device> {
        zgui_layout::scroll_region::region_of_element(store, container)
            .map(|region| region.limit())
            .unwrap_or(Point::new(DevicePx(0.0), DevicePx(0.0)))
    }

    /// Sets one container's displaced edge, forgetting it when it has come all the way back.
    fn displace(&mut self, container: NodeKey, edge: Overscroll) {
        if edge.arrived() {
            self.elastic.remove(&container);
        } else {
            self.elastic.insert(container, edge);
        }
        self.compose(container);
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::NodeKey;
    use zgui_geom::{Device, DevicePx, Point, Size};

    use super::{Overscroll, Scroller};

    fn key(index: u32) -> NodeKey {
        NodeKey::new(
            index,
            zgui_arena::Generation::FIRST,
            zgui_arena::DomainId::FIRST,
        )
    }

    #[test]
    fn a_container_nobody_touched_is_at_the_origin_and_holding_nothing() {
        let scroller = Scroller::new();
        assert_eq!(scroller.offset_of(key(3)).y, DevicePx(0.0));
        assert_eq!(
            scroller.elastic_of(key(3)),
            Size::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(0.0))
        );
        assert!(!scroller.is_animating());
        assert!(scroller.settled());
    }

    #[test]
    fn a_displacement_is_carried_per_container_and_not_shared() {
        let mut scroller = Scroller::new();
        let pull =
            |by: f32| Overscroll::default().pulled_by(Size::new(DevicePx(0.0), DevicePx(by)));
        scroller.displace(key(1), pull(20.0));
        scroller.displace(key(2), pull(30.0));
        assert!(scroller.elastic_of(key(1)).height.0 > 0.0);
        assert!(scroller.elastic_of(key(2)).height.0 > scroller.elastic_of(key(1)).height.0);

        // Relaxing one to nothing leaves the other stretched, and takes the first out of the map
        // rather than leaving a zero in it — which is what makes `settled` one emptiness test.
        scroller.displace(key(1), Overscroll::default());
        assert_eq!(scroller.elastic_of(key(1)).height, DevicePx(0.0));
        assert!(scroller.elastic_of(key(2)).height.0 > 0.0);
        assert!(!scroller.settled());
    }

    #[test]
    fn a_spring_returning_moves_the_composed_offset_on_every_frame_of_a_fast_output() {
        // What a rounded displacement costs, stated where the rounding is. A return covers the
        // band in about a third of a second and decelerates the whole way, so on a fast output
        // most of its frames move it by well under a device pixel — and rounded, most of its
        // frames therefore compose the content exactly where the last frame composed it. Nothing
        // downstream can tell those frames from a spring that has stopped: the picture is
        // identical, the renderer refuses it as undamaged, and what is on the screen is a return
        // drawn at a fraction of the rate of the output it is on.
        let container = key(9);
        let mut scroller = Scroller::new();
        let frame = core::time::Duration::from_micros(4_167);
        scroller.displace(
            container,
            Overscroll::default().pulled_by(Size::new(DevicePx(0.0), DevicePx(400.0))),
        );

        let mut composed = vec![scroller.composed().of(container).y];
        let mut edge = scroller.overscroll_of(container);
        while !edge.arrived() && composed.len() < 400 {
            edge = edge.advanced(frame);
            scroller.displace(container, edge);
            composed.push(scroller.composed().of(container).y);
        }

        assert!(
            composed.len() > 40,
            "the return was over in {} frames, so it is not the case being measured",
            composed.len()
        );
        let still = composed
            .windows(2)
            .filter(|pair| pair[0] == pair[1])
            .count();
        assert_eq!(
            still,
            0,
            "the edge stood still for {still} of {} frames of its own return",
            composed.len() - 1
        );
    }

    #[test]
    fn a_container_that_is_not_displaced_is_composed_on_the_device_grid() {
        // The other half of the same decision. A fractional *scroll offset* is rounded, because a
        // scrollport whose contents begin a third of a pixel further along is one whose pieces
        // round to the grid the other way — and a spring that has come all the way back leaves the
        // container exactly where one that was never pulled sits.
        let container = key(4);
        let mut scroller = Scroller::new();
        scroller
            .offsets
            .place(container, Point::new(DevicePx(0.0), DevicePx(120.4)));
        scroller.displace(
            container,
            Overscroll::default().pulled_by(Size::new(DevicePx(0.0), DevicePx(50.0))),
        );
        assert_ne!(
            scroller.composed().of(container).y.0.fract(),
            0.0,
            "the displacement was put on the grid, which is what quantises the return"
        );

        scroller.displace(container, Overscroll::default());
        assert_eq!(scroller.composed().of(container).y, DevicePx(120.0));
    }
}
