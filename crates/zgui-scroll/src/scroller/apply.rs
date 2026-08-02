//! Writing an offset: a delta shared out along a chain, and a destination asked for outright.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::region_of_element;

use crate::chain;
use crate::motion::{Behavior, Momentum, Motion, Tween};
use crate::scroller::Scroller;
use crate::stretch::Stretch;

impl Scroller {
    /// Shares `delta` out along `chain`, innermost container first.
    ///
    /// Each container takes what it has room for and hands the rest outwards; what survives the
    /// outermost one displaces that container past its end, elastically. The result names every
    /// container whose *composed* position changed, which is what the caller marks — and that
    /// includes one displaced past its end, because the content is drawn somewhere else even though
    /// its reported offset did not move.
    ///
    /// A container that is asked to move is taken off any motion it was on: a wheel turned during a
    /// smooth scroll means the person has taken over, and continuing to interpolate towards a
    /// destination they have since scrolled away from is the "it fights me" bug.
    pub fn scroll_by(
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
            let share = chain::absorb(at, self.limit_for(store, container), left);
            left = share.left;
            if !chain::negligible(share.taken) {
                self.motions.remove(&container);
                let to = Point::new(
                    DevicePx(at.x.0 + share.taken.width.0),
                    DevicePx(at.y.0 + share.taken.height.0),
                );
                let landed = self.offsets.scroll_to(store, container, to);
                self.compose(container);
                self.record(container, at, landed);
                moved.push(container);
            }
            let outermost = depth + 1 == chain.len();
            if outermost && stretch.is_permitted() && !chain::negligible(left) {
                let edge = self.overscroll_of(container).pulled_by(left);
                if edge != self.overscroll_of(container) {
                    self.displace(container, edge);
                    if !moved.contains(&container) {
                        moved.push(container);
                    }
                }
            }
            if chain::negligible(left) {
                break;
            }
        }
        moved
    }

    /// Puts one container at `to`, now or over the next few frames.
    ///
    /// Returns where it landed for an instant scroll and where it is heading for a smooth one, or
    /// nothing when the container does not scroll at all.
    pub fn scroll_to(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        to: Point<DevicePx, Device>,
        behavior: Behavior,
    ) -> Option<Point<DevicePx, Device>> {
        let limit = region_of_element(store, container)?.limit();
        let clamped = Point::new(
            DevicePx(to.x.0.clamp(0.0, limit.x.0)),
            DevicePx(to.y.0.clamp(0.0, limit.y.0)),
        );
        let at = self.offset_of(container);
        match behavior {
            Behavior::Auto | Behavior::Instant => {
                self.motions.remove(&container);
                if at == clamped {
                    return Some(clamped);
                }
                let landed = self.offsets.scroll_to(store, container, clamped);
                self.compose(container);
                self.record(container, at, landed);
                Some(landed)
            }
            Behavior::Smooth => {
                if at == clamped {
                    self.motions.remove(&container);
                    return Some(clamped);
                }
                self.motions
                    .insert(container, Motion::Smooth(Tween::new(at, clamped)));
                Some(clamped)
            }
        }
    }

    /// Hands one container the speed a gesture let go of it with, in device pixels per second.
    ///
    /// A fling towards an edge the container is already at is not installed: it would spend the
    /// next twenty frames asking to move a container that cannot, which is a deadline per frame for
    /// nothing.
    pub fn fling(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        velocity: Size<DevicePx, Device>,
    ) {
        if crate::motion::stopped(velocity) {
            return;
        }
        let at = self.offset_of(container);
        let share = chain::absorb(at, self.limit_for(store, container), velocity);
        if chain::negligible(share.taken) {
            return;
        }
        self.motions
            .insert(container, Motion::Fling(Momentum::new(velocity)));
    }
}
