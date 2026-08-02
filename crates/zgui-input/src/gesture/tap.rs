//! How far a contact may travel and still count as having stayed put.

use zgui_geom::{Css, CssPx, Point};

/// How far a contact may move, in CSS pixels, before it stops being a tap candidate.
///
/// A finger is not a mouse: the contact patch is centimetres across, the reported point wanders
/// inside it, and a hand resting on a device transmits every small movement of the arm. With no
/// slop a touch screen has no taps at all, and with too much a scroll activates the row it started
/// on. Ten CSS pixels is the value every touch platform converges on.
pub const SLOP: f32 = 10.0;

/// Whether a contact that started at `origin` and is now at `position` has travelled past the slop.
///
/// Measured from the *origin* rather than accumulated over the moves, because a finger that wobbles
/// back and forth over one spot travels an unbounded distance without ever leaving the control it
/// is resting on.
pub fn travelled(origin: Point<CssPx, Css>, position: Point<CssPx, Css>) -> bool {
    let dx = position.x.0 - origin.x.0;
    let dy = position.y.0 - origin.y.0;
    dx * dx + dy * dy > SLOP * SLOP
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Point};

    use super::{SLOP, travelled};

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn a_wobble_inside_the_slop_is_still_a_tap() {
        assert!(!travelled(at(0.0, 0.0), at(3.0, 4.0)));
        assert!(!travelled(at(0.0, 0.0), at(0.0, SLOP)));
    }

    #[test]
    fn a_drag_past_it_is_not() {
        assert!(travelled(at(0.0, 0.0), at(0.0, SLOP + 1.0)));
        assert!(travelled(at(50.0, 50.0), at(50.0, 20.0)));
    }

    #[test]
    fn wobbling_back_and_forth_for_ever_is_still_a_tap() {
        // Distance from the origin, not distance travelled: a finger resting on a control for two
        // seconds covers metres of path and has not left the control.
        let origin = at(0.0, 0.0);
        for step in 0..1_000 {
            let offset = if step % 2 == 0 { 4.0 } else { -4.0 };
            assert!(!travelled(origin, at(offset, 0.0)));
        }
    }
}
