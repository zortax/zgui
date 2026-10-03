//! A touchpad gesture: fingers down, a drag, fingers up.
//!
//! A gesture latches to one container at its first movement and moves only that container until it
//! ends, as AppKit and the browsers do. A list that reaches its end halfway through a gesture then
//! stretches; the page around it stays still. A gesture that starts clearly along one axis also
//! rails to that axis.
//!
//! While the fingers are down, every edge the gesture stretches is gripped: it stays where the
//! finger put it. When the fingers lift, the edges return.

use core::time::Duration;

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_layout::LayoutStore;

use crate::chain;
use crate::scroller::Scroller;
use crate::stretch::Stretch;

/// How much stronger one axis of the first movement must be for the gesture to rail to it.
const RAIL_RATIO: f32 = 2.0;

/// A gap between two events longer than this says nothing about the speed, in seconds.
const STALE: f32 = 0.1;

/// A gap between two events shorter than this is too short to measure a speed over, in seconds.
const COALESCED: f32 = 0.001;

/// Which axes a gesture moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rail {
    /// Both axes.
    Free,
    /// The horizontal axis only.
    Across,
    /// The vertical axis only.
    Down,
}

impl Rail {
    /// The rail a gesture whose first movement is `delta` follows.
    fn for_first(delta: Size<DevicePx, Device>) -> Self {
        let (x, y) = (delta.width.0.abs(), delta.height.0.abs());
        if y >= x * RAIL_RATIO {
            Self::Down
        } else if x >= y * RAIL_RATIO {
            Self::Across
        } else {
            Self::Free
        }
    }

    /// The part of `delta` this rail lets through.
    fn along(self, delta: Size<DevicePx, Device>) -> Size<DevicePx, Device> {
        match self {
            Self::Free => delta,
            Self::Across => Size::new(delta.width, DevicePx(0.0)),
            Self::Down => Size::new(DevicePx(0.0), delta.height),
        }
    }
}

/// How far a gesture has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// The fingers are down.
    Touching,
    /// The fingers are up and the edges were at their ends: momentum may follow.
    Lifted,
    /// The gesture has nothing more to move: momentum that follows is dropped.
    Spent,
}

/// One touchpad gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Gesture {
    /// The container the gesture moves, once its first movement has chosen it.
    pub(crate) latched: Option<NodeKey>,
    /// The axes the gesture moves, once its first movement has chosen them.
    rail: Option<Rail>,
    /// How far the gesture has got.
    pub(crate) stage: Stage,
    /// The speed of the gesture, in device pixels per second.
    pub(crate) velocity: Size<DevicePx, Device>,
    /// The stamp of the last event, as time since an arbitrary origin.
    last: Option<Duration>,
    /// Whether the speed holds a measurement.
    sampled: bool,
}

impl Gesture {
    /// A gesture whose fingers have just come down.
    fn touching() -> Self {
        Self {
            latched: None,
            rail: None,
            stage: Stage::Touching,
            velocity: Size::new(DevicePx(0.0), DevicePx(0.0)),
            last: None,
            sampled: false,
        }
    }

    /// Records that `delta` arrived at `at`, and gives the part of it the rail lets through.
    pub(crate) fn moved(
        &mut self,
        delta: Size<DevicePx, Device>,
        at: Duration,
    ) -> Size<DevicePx, Device> {
        if self.rail.is_none() && !chain::negligible(delta) {
            self.rail = Some(Rail::for_first(delta));
        }
        let along = self.rail.unwrap_or(Rail::Free).along(delta);
        self.measure(along, at);
        along
    }

    /// Updates the speed from `delta`, which arrived at `at`.
    fn measure(&mut self, delta: Size<DevicePx, Device>, at: Duration) {
        let Some(last) = self.last.replace(at) else {
            return;
        };
        let gap = at.saturating_sub(last).as_secs_f32();
        if gap > STALE {
            self.velocity = Size::new(DevicePx(0.0), DevicePx(0.0));
            self.sampled = false;
            return;
        }
        if gap < COALESCED {
            self.last = Some(last);
            return;
        }
        // The first measurement is taken whole, and each later one is averaged with the speed.
        let weight = if self.sampled { 0.5 } else { 0.0 };
        self.sampled = true;
        let sample = |speed: f32, by: f32| weight * speed + (1.0 - weight) * by / gap;
        self.velocity = Size::new(
            DevicePx(sample(self.velocity.width.0, delta.width.0)),
            DevicePx(sample(self.velocity.height.0, delta.height.0)),
        );
    }
}

impl Scroller {
    /// Whether a touchpad gesture has its fingers down.
    pub fn is_touching(&self) -> bool {
        self.gesture
            .is_some_and(|gesture| gesture.stage == Stage::Touching)
    }

    /// The container the gesture in progress moves, once it has chosen one.
    pub fn latched(&self) -> Option<NodeKey> {
        self.gesture.and_then(|gesture| gesture.latched)
    }

    /// Starts a gesture: fingers came down over `chain`.
    ///
    /// Stops every motion on the chain and grips every displaced edge, so a touch catches a bounce
    /// that is still returning.
    pub fn touch(&mut self, chain: &[NodeKey]) {
        for container in chain {
            self.motions.remove(container);
        }
        for edge in self.elastic.values_mut() {
            *edge = edge.gripped();
        }
        self.gesture = Some(Gesture::touching());
    }

    /// Moves the latched container of the gesture by `delta`, which arrived at `at`.
    ///
    /// The first movement latches the gesture to the innermost container of `chain` that can move
    /// that way. If none can, it latches to the outermost one that scrolls on that axis, which
    /// then stretches. The edge stays gripped while the fingers are down. Returns the containers
    /// whose composed position changed.
    pub fn drag_by(
        &mut self,
        store: &LayoutStore,
        chain: &[NodeKey],
        delta: Size<DevicePx, Device>,
        at: Duration,
        stretch: Stretch,
    ) -> SmallVec<[NodeKey; 2]> {
        let Some(mut gesture) = self
            .gesture
            .filter(|gesture| gesture.stage == Stage::Touching)
        else {
            return self.scroll_by(store, chain, delta, stretch);
        };
        let delta = gesture.moved(delta, at);
        if gesture.latched.is_none() && !chain::negligible(delta) {
            gesture.latched = self.latch(store, chain, delta);
        }
        self.gesture = Some(gesture);
        match gesture.latched {
            Some(container) if !chain::negligible(delta) => {
                self.move_latched(store, container, delta, stretch, true)
            }
            _ => SmallVec::new(),
        }
    }

    /// Moves the latched container of the gesture by a momentum `delta`, which arrived at `at`.
    ///
    /// The platform carries the content on after the fingers lift, and this follows it. When the
    /// momentum reaches an end, the edge travels on past it at the speed of the gesture and
    /// returns, and the rest of the momentum is dropped. Momentum after a lift with a displaced
    /// edge is also dropped. Returns the containers whose composed position changed.
    pub fn coast_by(
        &mut self,
        store: &LayoutStore,
        chain: &[NodeKey],
        delta: Size<DevicePx, Device>,
        at: Duration,
        stretch: Stretch,
    ) -> SmallVec<[NodeKey; 2]> {
        let Some(mut gesture) = self.gesture else {
            return self.scroll_by(store, chain, delta, Stretch::Refused);
        };
        if gesture.stage == Stage::Touching {
            self.lift();
            gesture.stage = self.gesture.map_or(Stage::Lifted, |lifted| lifted.stage);
        }
        if gesture.stage == Stage::Spent {
            return SmallVec::new();
        }
        let delta = gesture.moved(delta, at);
        if gesture.latched.is_none() && !chain::negligible(delta) {
            gesture.latched = self.latch(store, chain, delta);
        }
        let Some(container) = gesture.latched.filter(|_| !chain::negligible(delta)) else {
            self.gesture = Some(gesture);
            return SmallVec::new();
        };
        let limit = self.hand_limit_for(store, container);
        let share = chain::absorb(self.offset_of(container), limit, delta);
        let mut moved = self.move_latched(store, container, delta, Stretch::Refused, false);
        let past = Size::new(
            DevicePx(if limit.x.0 > 0.0 {
                share.left.width.0
            } else {
                0.0
            }),
            DevicePx(if limit.y.0 > 0.0 {
                share.left.height.0
            } else {
                0.0
            }),
        );
        if !chain::negligible(past) {
            gesture.stage = Stage::Spent;
            if stretch.is_permitted() {
                let speed = Size::new(
                    DevicePx(throw(past.width.0, gesture.velocity.width.0)),
                    DevicePx(throw(past.height.0, gesture.velocity.height.0)),
                );
                self.displace(container, self.overscroll_of(container).thrown(speed));
                if !moved.contains(&container) {
                    moved.push(container);
                }
            }
        }
        self.gesture = Some(gesture);
        moved
    }

    /// Ends the touch of a gesture: the fingers lifted.
    ///
    /// Lets go of every gripped edge so that it returns. An edge still displaced at the lift
    /// spends the gesture: the momentum that follows is dropped, as AppKit drops it. Returns the
    /// containers whose edges now return.
    pub fn lift(&mut self) -> SmallVec<[NodeKey; 2]> {
        let mut returning: SmallVec<[NodeKey; 2]> = SmallVec::new();
        for (container, edge) in self.elastic.iter_mut() {
            if edge.is_gripped() {
                *edge = edge.released();
                returning.push(*container);
            }
        }
        if let Some(gesture) = self.gesture.as_mut() {
            gesture.stage = if returning.is_empty() {
                Stage::Lifted
            } else {
                Stage::Spent
            };
        }
        returning
    }

    /// Forgets the gesture, letting go of every edge it held.
    pub fn forget_gesture(&mut self) -> SmallVec<[NodeKey; 2]> {
        let returning = self.lift();
        self.gesture = None;
        returning
    }

    /// The container a gesture that first moves by `delta` over `chain` latches to.
    fn latch(
        &self,
        store: &LayoutStore,
        chain: &[NodeKey],
        delta: Size<DevicePx, Device>,
    ) -> Option<NodeKey> {
        let held = chain
            .iter()
            .copied()
            .find(|container| self.elastic.contains_key(container));
        let moves = || {
            chain.iter().copied().find(|container| {
                let share = chain::absorb(
                    self.offset_of(*container),
                    self.hand_limit_for(store, *container),
                    delta,
                );
                !chain::negligible(share.taken)
            })
        };
        let stretches = || {
            chain.iter().rev().copied().find(|container| {
                let limit = self.hand_limit_for(store, *container);
                (delta.width.0 != 0.0 && limit.x.0 > 0.0)
                    || (delta.height.0 != 0.0 && limit.y.0 > 0.0)
            })
        };
        held.or_else(moves).or_else(stretches)
    }

    /// Moves one container by `delta`: first back towards its end, then its offset, then past
    /// its end on each axis it scrolls.
    pub(crate) fn move_latched(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        delta: Size<DevicePx, Device>,
        stretch: Stretch,
        grip: bool,
    ) -> SmallVec<[NodeKey; 2]> {
        let mut moved: SmallVec<[NodeKey; 2]> = SmallVec::new();
        let left = self.unwind(store, &[container], delta, &mut moved);
        let at = self.offset_of(container);
        let limit = self.hand_limit_for(store, container);
        let share = chain::absorb(at, limit, left);
        if !chain::negligible(share.taken) {
            self.motions.remove(&container);
            let to = Point::new(
                DevicePx(at.x.0 + share.taken.width.0),
                DevicePx(at.y.0 + share.taken.height.0),
            );
            let landed = self.offsets.scroll_to(store, container, to);
            self.compose(container);
            self.record(container, at, landed);
            if !moved.contains(&container) {
                moved.push(container);
            }
        }
        let past = Size::new(
            DevicePx(if limit.x.0 > 0.0 {
                share.left.width.0
            } else {
                0.0
            }),
            DevicePx(if limit.y.0 > 0.0 {
                share.left.height.0
            } else {
                0.0
            }),
        );
        if stretch.is_permitted() && !chain::negligible(past) {
            let edge = self
                .overscroll_of(container)
                .pulled_by(past, self.extent_for(store, container));
            let edge = if grip { edge.gripped() } else { edge };
            if edge != self.overscroll_of(container) {
                self.displace(container, edge);
                if !moved.contains(&container) {
                    moved.push(container);
                }
            }
        }
        moved
    }
}

/// The speed an edge is thrown at when momentum carries `past` beyond its end.
///
/// The speed of the gesture, in the direction of `past`. A gesture with no measured speed throws
/// at the speed of one frame of `past` at sixty hertz.
fn throw(past: f32, velocity: f32) -> f32 {
    if past == 0.0 {
        0.0
    } else if velocity == 0.0 {
        past * 60.0
    } else {
        past.signum() * velocity.abs()
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{DevicePx, Size};

    use super::{Gesture, Rail};

    fn by(x: f32, y: f32) -> Size<DevicePx, zgui_geom::Device> {
        Size::new(DevicePx(x), DevicePx(y))
    }

    #[test]
    fn a_gesture_that_starts_clearly_along_one_axis_rails_to_it() {
        assert_eq!(Rail::for_first(by(3.0, 20.0)), Rail::Down);
        assert_eq!(Rail::for_first(by(-20.0, 3.0)), Rail::Across);
        assert_eq!(Rail::for_first(by(10.0, 12.0)), Rail::Free);
    }

    #[test]
    fn a_railed_gesture_drops_the_other_axis_until_it_ends() {
        let mut gesture = Gesture::touching();
        assert_eq!(gesture.moved(by(1.0, 20.0), Duration::ZERO), by(0.0, 20.0));
        assert_eq!(
            gesture.moved(by(30.0, 2.0), Duration::from_millis(8)),
            by(0.0, 2.0)
        );
    }

    #[test]
    fn the_speed_is_measured_from_the_stamps() {
        let mut gesture = Gesture::touching();
        for step in 0..10 {
            gesture.moved(by(0.0, 10.0), Duration::from_millis(10 * step));
        }
        assert!(
            (gesture.velocity.height.0 - 1000.0).abs() < 1.0,
            "{:?}",
            gesture.velocity
        );
        gesture.moved(by(0.0, 10.0), Duration::from_secs(5));
        assert_eq!(gesture.velocity, by(0.0, 0.0), "a stale gap says nothing");
    }
}
