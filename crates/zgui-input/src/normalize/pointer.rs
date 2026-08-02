//! Where a pointer is, in the space the fragments were measured in.
//!
//! A pointer position arrives in CSS pixels, because that is the space a handler wants to compare
//! it against its own element in. The fragments it has to be tested against are in device pixels,
//! because that is the space they were rounded in. The conversion is one multiplication and it is
//! written once, here, so that a hit test cannot silently be performed in the wrong space on a
//! display whose scale is not one.

use zgui_geom::{Css, Device, DevicePx, Point, Scale};
use zgui_vocab::{PointerAction, PointerEvent, PointerId};

use smallvec::SmallVec;

/// Where this pointer is, in device pixels.
///
/// ```
/// use zgui_geom::{CssPx, DevicePx, Point, Scale};
/// use zgui_input::normalize::pointer::device_position;
/// use zgui_vocab::PointerEvent;
///
/// let event = PointerEvent::mouse(Point::new(CssPx(10.0), CssPx(4.0)));
/// let at = device_position(&event, Scale::new(2.0));
/// assert_eq!(at, Point::new(DevicePx(20.0), DevicePx(8.0)));
/// ```
pub fn device_position(event: &PointerEvent, scale: Scale<Css, Device>) -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(event.position.x.0 * scale.get()),
        DevicePx(event.position.y.0 * scale.get()),
    )
}

/// Where each pointer was last seen.
///
/// Kept because two things need it and neither has an event to read it from. A re-test under a
/// pointer that has not moved is how a control that slid out from under the cursor stops being
/// hovered, and it happens in a frame with no pointer event in it at all. And a pointer that
/// leaves the surface has to be forgotten, or the next re-test hovers whatever has since moved
/// under a position nothing is pointing at.
///
/// ```
/// use zgui_geom::{CssPx, Point, Scale};
/// use zgui_input::normalize::pointer::Pointers;
/// use zgui_vocab::{PointerAction, PointerEvent, PointerId};
///
/// let mut pointers = Pointers::default();
/// let event = PointerEvent::mouse(Point::new(CssPx(3.0), CssPx(3.0)));
/// pointers.observe(PointerAction::Moved, &event, Scale::new(1.0));
///
/// assert!(pointers.position_of(PointerId::MOUSE).is_some());
/// pointers.observe(PointerAction::Left, &event, Scale::new(1.0));
/// assert!(pointers.position_of(PointerId::MOUSE).is_none());
/// ```
#[derive(Clone, Debug, Default)]
pub struct Pointers {
    /// One entry per pointer that is on the surface.
    seen: SmallVec<[(PointerId, Point<DevicePx, Device>); 2]>,
}

impl Pointers {
    /// Records where `event`'s pointer now is, or forgets it when it has gone.
    pub fn observe(
        &mut self,
        action: PointerAction,
        event: &PointerEvent,
        scale: Scale<Css, Device>,
    ) {
        if matches!(action, PointerAction::Left | PointerAction::Cancelled) {
            self.forget(event.id);
            return;
        }
        let at = device_position(event, scale);
        match self.seen.iter_mut().find(|(id, _)| *id == event.id) {
            Some((_, held)) => *held = at,
            None => self.seen.push((event.id, at)),
        }
    }

    /// Where a pointer was last seen, if it is on the surface.
    pub fn position_of(&self, pointer: PointerId) -> Option<Point<DevicePx, Device>> {
        self.seen
            .iter()
            .find(|(id, _)| *id == pointer)
            .map(|(_, at)| *at)
    }

    /// Every pointer on the surface and where it is, in the order they arrived.
    pub fn all(&self) -> impl Iterator<Item = (PointerId, Point<DevicePx, Device>)> + '_ {
        self.seen.iter().copied()
    }

    /// Forgets one pointer.
    pub fn forget(&mut self, pointer: PointerId) {
        self.seen.retain(|(id, _)| *id != pointer);
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, DevicePx, Point, Scale};
    use zgui_vocab::{PointerAction, PointerEvent, PointerId};

    use super::{Pointers, device_position};

    fn at(x: f32, y: f32) -> PointerEvent {
        PointerEvent::mouse(Point::new(CssPx(x), CssPx(y)))
    }

    #[test]
    fn a_scale_of_one_leaves_the_position_alone() {
        assert_eq!(
            device_position(&at(7.5, 2.5), Scale::new(1.0)),
            Point::new(DevicePx(7.5), DevicePx(2.5))
        );
    }

    #[test]
    fn a_second_pointer_does_not_displace_the_first() {
        let mut pointers = Pointers::default();
        pointers.observe(PointerAction::Moved, &at(1.0, 1.0), Scale::new(1.0));
        let mut second = at(9.0, 9.0);
        second.id = PointerId::new(7);
        pointers.observe(PointerAction::Moved, &second, Scale::new(1.0));

        assert_eq!(
            pointers.position_of(PointerId::MOUSE),
            Some(Point::new(DevicePx(1.0), DevicePx(1.0)))
        );
        assert_eq!(
            pointers.position_of(PointerId::new(7)),
            Some(Point::new(DevicePx(9.0), DevicePx(9.0)))
        );
        assert_eq!(pointers.all().count(), 2);
    }

    #[test]
    fn a_cancelled_interaction_forgets_the_pointer_exactly_as_leaving_does() {
        for action in [PointerAction::Left, PointerAction::Cancelled] {
            let mut pointers = Pointers::default();
            pointers.observe(PointerAction::Moved, &at(1.0, 1.0), Scale::new(1.0));
            pointers.observe(action, &at(1.0, 1.0), Scale::new(1.0));
            assert!(pointers.position_of(PointerId::MOUSE).is_none());
        }
    }

    #[test]
    fn moving_a_pointer_replaces_where_it_was() {
        let mut pointers = Pointers::default();
        pointers.observe(PointerAction::Moved, &at(1.0, 1.0), Scale::new(2.0));
        pointers.observe(PointerAction::Moved, &at(3.0, 4.0), Scale::new(2.0));
        assert_eq!(pointers.all().count(), 1);
        assert_eq!(
            pointers.position_of(PointerId::MOUSE),
            Some(Point::new(DevicePx(6.0), DevicePx(8.0)))
        );
    }
}
