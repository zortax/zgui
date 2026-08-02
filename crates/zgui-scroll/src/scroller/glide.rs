//! A delta that travels to its destination instead of arriving at it.
//!
//! A wheel detent is a discrete event and a jump is not what anybody wants to see: the content has
//! to travel to its new place over a couple of hundred milliseconds so that the eye can follow what
//! moved where. That is what this is, and the whole of what makes it more than one call to
//! [`Scroller::scroll_to`](crate::Scroller) with a smooth behaviour is the second detent.
//!
//! # Detents compose; they do not restart
//!
//! Somebody turning a wheel three times in half a second is asking to go three times as far, and
//! each detent arrives while the previous one is still travelling. A motion re-aimed from the
//! container's *current* offset would throw away everything the previous detents had not yet
//! covered, so three quick detents would land barely further than one — the well-known "the wheel
//! does nothing when I spin it" behaviour. The delta is therefore added to the destination a
//! running motion is already heading for, and only the ease is restarted.
//!
//! The chaining, the clamping and the elastic overflow all work exactly as they do for an immediate
//! scroll, and for the same reason: they are questions about where the content *ends up*, and a
//! motion that has not arrived yet has still committed to ending up there.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_layout::LayoutStore;

use crate::chain;
use crate::motion::{Motion, Tween};
use crate::scroller::Scroller;
use crate::stretch::Stretch;

impl Scroller {
    /// Shares `delta` out along `chain`, carrying each container there over the next few frames.
    ///
    /// The counterpart of [`Scroller::scroll_by`], which arrives in this frame. Returns every
    /// container whose composed position changed *now* — which is the ones that were pushed past
    /// their end, because that displacement is immediate — while the ones that took a share of the
    /// delta are reported by [`Scroller::advance`] as they travel.
    pub fn glide_by(
        &mut self,
        store: &LayoutStore,
        chain: &[NodeKey],
        delta: Size<DevicePx, Device>,
        stretch: Stretch,
    ) -> SmallVec<[NodeKey; 2]> {
        let mut moved: SmallVec<[NodeKey; 2]> = SmallVec::new();
        let mut left = delta;
        if chain::negligible(left) {
            return moved;
        }
        for (depth, container) in chain.iter().copied().enumerate() {
            let at = self.offset_of(container);
            // Where this container is *going*, not where it is. A detent that arrives mid-flight
            // adds to the previous one's destination; one that arrives with nothing running adds to
            // the offset itself, which is the same thing with nothing in flight.
            let heading = self.heading_of(container).unwrap_or(at);
            let share = chain::absorb(heading, self.limit_for(store, container), left);
            left = share.left;
            if !chain::negligible(share.taken) {
                let to = Point::new(
                    DevicePx(heading.x.0 + share.taken.width.0),
                    DevicePx(heading.y.0 + share.taken.height.0),
                );
                self.aim(container, at, to);
            }
            let outermost = depth + 1 == chain.len();
            if outermost && stretch.is_permitted() && !chain::negligible(left) {
                let edge = self.overscroll_of(container).pulled_by(left);
                if edge != self.overscroll_of(container) {
                    self.displace(container, edge);
                    moved.push(container);
                }
            }
            if chain::negligible(left) {
                break;
            }
        }
        moved
    }

    /// Where a container is heading, if a smooth motion is carrying it somewhere.
    ///
    /// A fling has no destination — it has a speed, and where it stops is whatever that speed
    /// carries it to — so a detent during one is a fresh aim from where the content has got to,
    /// which is also what taking over from a fling means.
    fn heading_of(&self, container: NodeKey) -> Option<Point<DevicePx, Device>> {
        match self.motion_of(container)? {
            Motion::Smooth(tween) => Some(tween.destination()),
            Motion::Fling(_) => None,
        }
    }

    /// Points one container's motion at `to`, continuing from `at`.
    fn aim(
        &mut self,
        container: NodeKey,
        at: Point<DevicePx, Device>,
        to: Point<DevicePx, Device>,
    ) {
        match self.motion_mut(container) {
            Some(Motion::Smooth(tween)) => tween.retarget(at, to),
            _ => self.install(container, Motion::Smooth(Tween::new(at, to))),
        }
    }
}
