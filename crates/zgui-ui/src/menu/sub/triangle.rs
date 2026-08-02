//! Telling a pointer on its way to a submenu from one on its way somewhere else.

use zgui::geom::{Device, DevicePx, Point, Rect};

/// Whether a pointer that was at `from` and is now at `to` is travelling into `target`.
///
/// The problem this answers is the oldest one in menus. A submenu opens to the right of its
/// trigger; the pointer has to cross the items *below* the trigger to reach it, because that is
/// the shape of the diagonal a hand actually draws. Closing the submenu the moment the pointer
/// leaves the trigger makes it unreachable by any path except a perfect right angle.
///
/// So the question is not *has the pointer left the trigger* but *is it heading for the submenu*,
/// and the answer is a triangle: the pointer's last position and the two corners of the submenu on
/// the side facing it. Anywhere inside that triangle is on the way there.
///
/// ```
/// use zgui::geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_ui::menu::heading_toward;
///
/// let at = |x: f32, y: f32| Point::new(DevicePx(x), DevicePx(y));
/// let submenu = Rect::new(at(200.0, 100.0), Size::new(DevicePx(160.0), DevicePx(200.0)));
///
/// // Leaving the trigger diagonally, down and to the right: on the way there.
/// assert!(heading_toward(at(100.0, 110.0), at(140.0, 160.0), submenu));
///
/// // Straight down the parent menu, away from the submenu: not on the way there.
/// assert!(!heading_toward(at(100.0, 110.0), at(100.0, 400.0), submenu));
/// ```
#[must_use]
pub fn heading_toward(
    from: Point<DevicePx, Device>,
    to: Point<DevicePx, Device>,
    target: Rect<DevicePx, Device>,
) -> bool {
    let left = target.origin.x.0;
    let right = left + target.size.width.0;
    let top = target.origin.y.0;
    let bottom = top + target.size.height.0;

    // The two corners on the side the pointer is coming from. A submenu that opened to the left is
    // reached across its right edge, and using the near edge either way is what makes one triangle
    // serve both directions.
    let near = if from.x.0 <= left { left } else { right };
    let apex = (from.x.0, from.y.0);
    let corners = [(near, top), (near, bottom)];

    inside(apex, corners[0], corners[1], (to.x.0, to.y.0))
}

/// Whether `point` is inside the triangle `a`, `b`, `c`, edges included.
fn inside(a: (f32, f32), b: (f32, f32), c: (f32, f32), point: (f32, f32)) -> bool {
    let side = |from: (f32, f32), to: (f32, f32)| {
        (to.0 - from.0) * (point.1 - from.1) - (to.1 - from.1) * (point.0 - from.0)
    };
    let (first, second, third) = (side(a, b), side(b, c), side(c, a));
    // All three cross products on the same side of zero, counting zero as either — a pointer
    // exactly on the edge of the corridor is still in it.
    let negative = first < 0.0 || second < 0.0 || third < 0.0;
    let positive = first > 0.0 || second > 0.0 || third > 0.0;
    !(negative && positive)
}

#[cfg(test)]
mod tests {
    use zgui::geom::{Device, DevicePx, Point, Rect, Size};

    use super::{heading_toward, inside};

    /// A point in device pixels.
    fn at(x: f32, y: f32) -> Point<DevicePx, Device> {
        Point::new(DevicePx(x), DevicePx(y))
    }

    /// A submenu opened to the right of a trigger at (100, 100).
    fn submenu_on_the_right() -> Rect<DevicePx, Device> {
        Rect::new(
            at(200.0, 100.0),
            Size::new(DevicePx(160.0), DevicePx(200.0)),
        )
    }

    #[test]
    fn a_pointer_cutting_the_corner_toward_the_submenu_is_on_its_way() {
        // The path a hand actually draws: out of the trigger and diagonally down-right, over the
        // items below it. Every one of those items would otherwise steal the highlight.
        let submenu = submenu_on_the_right();
        for step in [(140.0, 150.0), (170.0, 200.0), (190.0, 280.0)] {
            assert!(
                heading_toward(at(100.0, 110.0), at(step.0, step.1), submenu),
                "{step:?} is inside the corridor to the submenu"
            );
        }
    }

    #[test]
    fn a_pointer_walking_down_the_parent_menu_is_not() {
        // The case the corridor must not swallow: the user has given up on the submenu and is
        // choosing something else, and the submenu has to close.
        let submenu = submenu_on_the_right();
        assert!(!heading_toward(at(100.0, 110.0), at(100.0, 400.0), submenu));
        assert!(!heading_toward(at(100.0, 110.0), at(120.0, 500.0), submenu));
    }

    #[test]
    fn a_submenu_that_opened_to_the_left_has_a_corridor_the_other_way() {
        // Near the right edge of the window a submenu flips, and a corridor that only ever pointed
        // right would make the flipped one unreachable.
        let submenu = Rect::new(at(0.0, 100.0), Size::new(DevicePx(160.0), DevicePx(200.0)));
        assert!(heading_toward(at(300.0, 110.0), at(220.0, 180.0), submenu));
        assert!(!heading_toward(at(300.0, 110.0), at(300.0, 400.0), submenu));
    }

    #[test]
    fn the_edge_of_the_corridor_counts_as_inside_it() {
        // A pointer sampled exactly on the boundary is a pointer on its way there, and treating it
        // as outside makes the corridor flicker along its own edge.
        let apex = (0.0, 0.0);
        assert!(inside(apex, (10.0, 0.0), (10.0, 10.0), (10.0, 5.0)));
        assert!(inside(apex, (10.0, 0.0), (10.0, 10.0), (5.0, 0.0)));
        assert!(inside(apex, (10.0, 0.0), (10.0, 10.0), (5.0, 4.0)));
        assert!(
            !inside(apex, (10.0, 0.0), (10.0, 10.0), (5.0, 6.0)),
            "past the far edge is past it"
        );
    }
}
