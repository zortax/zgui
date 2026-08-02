//! Sharing one delta out along a chain of nested scroll containers.
//!
//! A wheel turned over a list inside a page scrolls the list until the list runs out and then
//! scrolls the page. That handover is not a special case anyone writes: it falls out of asking each
//! container in turn how much of the remaining delta it can absorb, innermost first, and passing on
//! what is left. What survives the outermost container is what nothing could absorb, and that is
//! what an elastic edge or a pull-to-refresh affordance is a function of.

use zgui_geom::{Device, DevicePx, Point, Size};

/// How much of a delta one container took, and how much it passed on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Absorbed {
    /// How far this container actually moved.
    pub taken: Size<DevicePx, Device>,
    /// What was left over for whatever contains it.
    pub left: Size<DevicePx, Device>,
}

/// What one container can absorb of `delta`, given where it is and how far it may go.
///
/// The two axes are answered independently, which is what makes a horizontal flick over a vertical
/// list scroll the page sideways while the list stays put.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Size};
/// use zgui_scroll::chain::absorb;
///
/// // 40 pixels of room left below, asked to move 100.
/// let at = Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(60.0));
/// let limit = Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(100.0));
/// let share = absorb(at, limit, Size::new(DevicePx(0.0), DevicePx(100.0)));
/// assert_eq!(share.taken.height, DevicePx(40.0));
/// assert_eq!(share.left.height, DevicePx(60.0));
/// ```
pub fn absorb(
    at: Point<DevicePx, Device>,
    limit: Point<DevicePx, Device>,
    delta: Size<DevicePx, Device>,
) -> Absorbed {
    let (taken_x, left_x) = axis(at.x.0, limit.x.0, delta.width.0);
    let (taken_y, left_y) = axis(at.y.0, limit.y.0, delta.height.0);
    Absorbed {
        taken: Size::new(DevicePx(taken_x), DevicePx(taken_y)),
        left: Size::new(DevicePx(left_x), DevicePx(left_y)),
    }
}

/// One axis of it: what fits between the current offset and the ends, and what does not.
fn axis(at: f32, limit: f32, delta: f32) -> (f32, f32) {
    let wanted = at + delta;
    let clamped = wanted.clamp(0.0, limit.max(0.0));
    (clamped - at, wanted - clamped)
}

/// Whether a delta is small enough that moving by it would move nothing.
///
/// Offsets are device pixels and a fragment is snapped to the device grid, so a delta below a
/// hundredth of one cannot change a composed position — and treating it as a scroll would mark a
/// container dirty, damage its scrollport and redraw it identically, once per event, for as long as
/// a trackpad keeps reporting the residue of a gesture that has stopped.
pub fn negligible(delta: Size<DevicePx, Device>) -> bool {
    delta.width.0.abs() < 0.01 && delta.height.0.abs() < 0.01
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Size};

    use super::{Absorbed, absorb, negligible};

    fn at(y: f32) -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(y))
    }

    fn delta(y: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(0.0), DevicePx(y))
    }

    #[test]
    fn a_container_with_room_absorbs_the_whole_delta() {
        let share = absorb(at(0.0), at(500.0), delta(120.0));
        assert_eq!(
            share,
            Absorbed {
                taken: delta(120.0),
                left: delta(0.0),
            }
        );
    }

    #[test]
    fn a_container_at_its_end_absorbs_nothing_and_passes_everything_on() {
        let share = absorb(at(500.0), at(500.0), delta(120.0));
        assert_eq!(share.taken, delta(0.0));
        assert_eq!(
            share.left,
            delta(120.0),
            "which is what makes the page scroll once the list inside it has bottomed out"
        );
    }

    #[test]
    fn scrolling_back_up_from_the_origin_passes_the_whole_delta_on() {
        let share = absorb(at(0.0), at(500.0), delta(-30.0));
        assert_eq!(share.taken, delta(0.0));
        assert_eq!(share.left, delta(-30.0));
    }

    #[test]
    fn an_unscrollable_container_passes_everything_on() {
        let share = absorb(at(0.0), at(0.0), delta(80.0));
        assert_eq!(share.taken, delta(0.0));
        assert_eq!(share.left, delta(80.0));
    }

    #[test]
    fn the_two_axes_are_answered_independently() {
        let share = absorb(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Point::new(DevicePx(0.0), DevicePx(400.0)),
            Size::new(DevicePx(50.0), DevicePx(50.0)),
        );
        assert_eq!(share.taken, Size::new(DevicePx(0.0), DevicePx(50.0)));
        assert_eq!(
            share.left,
            Size::new(DevicePx(50.0), DevicePx(0.0)),
            "the horizontal half is handed on while the vertical half is taken"
        );
    }

    #[test]
    fn the_residue_of_a_gesture_that_has_stopped_is_not_a_scroll() {
        assert!(negligible(delta(0.0)));
        assert!(negligible(delta(0.004)));
        assert!(!negligible(delta(0.5)));
    }
}
