//! The offset that brings a rectangle into a scrollport.
//!
//! Every argument is in one space — the space the fragment pass composed both rectangles in, which
//! already has the container's current offset applied. That is what makes the answer a *delta* plus
//! the current offset rather than an absolute position derived from content coordinates nobody
//! keeps.

use zgui_geom::{Device, DevicePx, Point, Rect};

/// Where in the scrollport a target should end up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Move as little as possible: a target already visible does not move at all.
    #[default]
    Nearest,
    /// Put the target's start edge at the scrollport's start edge.
    Start,
    /// Put the target's end edge at the scrollport's end edge.
    End,
    /// Put the target in the middle.
    Center,
}

/// The offset `container` must be scrolled to for `target` to be visible under `align`.
///
/// Unclamped: what the container can actually reach is the region's business, and clamping here
/// would answer a question this function is not the authority on.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scroll::into_view::{Align, offset_for};
///
/// let port = Rect::<DevicePx, Device>::new(
///     Point::new(DevicePx(0.0), DevicePx(0.0)),
///     Size::new(DevicePx(200.0), DevicePx(100.0)),
/// );
/// // A row 40 pixels below the bottom edge.
/// let row = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(120.0)),
///     Size::new(DevicePx(200.0), DevicePx(20.0)),
/// );
/// let at = Point::new(DevicePx(0.0), DevicePx(0.0));
/// assert_eq!(offset_for(at, row, port, Align::Nearest).y, DevicePx(40.0));
/// assert_eq!(offset_for(at, row, port, Align::Start).y, DevicePx(120.0));
/// ```
pub fn offset_for(
    at: Point<DevicePx, Device>,
    target: Rect<DevicePx, Device>,
    port: Rect<DevicePx, Device>,
    align: Align,
) -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(
            at.x.0
                + axis(
                    target.left().0,
                    target.right().0,
                    port.left().0,
                    port.right().0,
                    align,
                ),
        ),
        DevicePx(
            at.y.0
                + axis(
                    target.top().0,
                    target.bottom().0,
                    port.top().0,
                    port.bottom().0,
                    align,
                ),
        ),
    )
}

/// How far the offset must move along one axis.
fn axis(target_start: f32, target_end: f32, port_start: f32, port_end: f32, align: Align) -> f32 {
    match align {
        Align::Start => target_start - port_start,
        Align::End => target_end - port_end,
        Align::Center => (target_start + target_end) / 2.0 - (port_start + port_end) / 2.0,
        Align::Nearest => {
            // A target larger than the port is aligned to its start, which is what shows the part
            // of it anyone asking to see it means: the beginning.
            if target_start < port_start || target_end - target_start > port_end - port_start {
                target_start - port_start
            } else if target_end > port_end {
                target_end - port_end
            } else {
                0.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::{Align, offset_for};

    fn port() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(200.0), DevicePx(100.0)),
        )
    }

    fn row(top: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(top)),
            Size::new(DevicePx(200.0), DevicePx(height)),
        )
    }

    fn at(y: f32) -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(y))
    }

    #[test]
    fn something_already_visible_does_not_move_at_all() {
        assert_eq!(
            offset_for(at(300.0), row(20.0, 20.0), port(), Align::Nearest),
            at(300.0)
        );
    }

    #[test]
    fn nearest_moves_the_shorter_way_at_each_edge() {
        assert_eq!(
            offset_for(at(0.0), row(-30.0, 20.0), port(), Align::Nearest).y,
            DevicePx(-30.0)
        );
        assert_eq!(
            offset_for(at(0.0), row(110.0, 20.0), port(), Align::Nearest).y,
            DevicePx(30.0)
        );
    }

    #[test]
    fn the_three_placements_put_it_where_they_say() {
        assert_eq!(
            offset_for(at(0.0), row(150.0, 20.0), port(), Align::Start).y,
            DevicePx(150.0)
        );
        assert_eq!(
            offset_for(at(0.0), row(150.0, 20.0), port(), Align::End).y,
            DevicePx(70.0)
        );
        assert_eq!(
            offset_for(at(0.0), row(150.0, 20.0), port(), Align::Center).y,
            DevicePx(110.0)
        );
    }

    #[test]
    fn something_taller_than_the_port_is_shown_from_its_beginning() {
        // Aligning the far edge instead would scroll past the whole of it to show the end, which is
        // never what "bring this into view" means for a section header or a long paragraph.
        assert_eq!(
            offset_for(at(0.0), row(40.0, 400.0), port(), Align::Nearest).y,
            DevicePx(40.0)
        );
    }

    #[test]
    fn the_answer_is_relative_to_where_the_container_already_is() {
        assert_eq!(
            offset_for(at(500.0), row(110.0, 20.0), port(), Align::Nearest).y,
            DevicePx(530.0)
        );
    }
}
