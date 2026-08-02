//! Where a track and a thumb sit.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::axis::Axis;
use crate::scroll_region::bar::{Scrollport, travel};

/// The whole strip the bar on `axis` occupies.
///
/// The vertical bar takes the square where the two gutters meet and the horizontal one stops short
/// of it. One of them has to: two strips that both ran the full length would paint over each other
/// in that square, and two that both stopped short would leave it — which is exactly the corner a
/// full-window scrim is reported as missing.
///
/// An axis with no gutter answers with an empty rectangle rather than a hairline, so nothing has to
/// ask twice before drawing.
pub fn track(port: &Scrollport, axis: Axis) -> Rect<DevicePx, Device> {
    if !port.reserves(axis) {
        return Rect::ZERO;
    }
    let (inner, content) = (port.inner, port.content_box);
    match axis {
        Axis::Vertical => Rect::from_corners(
            Point::new(content.right(), inner.top()),
            Point::new(inner.right(), inner.bottom()),
        ),
        Axis::Horizontal => Rect::from_corners(
            Point::new(inner.left(), content.bottom()),
            Point::new(content.right(), inner.bottom()),
        ),
    }
}

/// Where the thumb on `axis` sits, and nothing when the content cannot move.
///
/// The thumb travels over the part of the track the content box spans, never into the corner: an
/// end-stop a person cannot reach by scrolling would leave the last screenful of a document
/// unreachable by dragging.
pub fn thumb(port: &Scrollport, axis: Axis) -> Rect<DevicePx, Device> {
    let travel = travel::of(port, axis);
    if travel.is_still() {
        return Rect::ZERO;
    }
    let track = track(port, axis);
    let start = travel.start + travel.free() * travel.fraction(port.at(axis));
    match axis {
        Axis::Vertical => Rect::new(
            Point::new(track.left(), DevicePx(start)),
            Size::new(track.size.width, DevicePx(travel.thumb)),
        ),
        Axis::Horizontal => Rect::new(
            Point::new(DevicePx(start), track.top()),
            Size::new(DevicePx(travel.thumb), track.size.height),
        ),
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Rect, Size};

    use super::{thumb, track};
    use crate::axis::Axis;
    use crate::scroll_region::bar::Scrollport;
    use crate::scroll_region::bar::tests::tall;

    /// A port with both gutters reserved, three hundred by two hundred inside.
    fn both() -> Scrollport {
        Scrollport {
            inner: Rect::new(
                Point::new(DevicePx(10.0), DevicePx(20.0)),
                Size::new(DevicePx(300.0), DevicePx(200.0)),
            ),
            content_box: Rect::new(
                Point::new(DevicePx(10.0), DevicePx(20.0)),
                Size::new(DevicePx(285.0), DevicePx(185.0)),
            ),
            content: Size::new(DevicePx(570.0), DevicePx(370.0)),
            offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
        }
    }

    #[test]
    fn the_two_tracks_tile_the_gutter_with_no_gap_and_no_overlap() {
        let port = both();
        let down = track(&port, Axis::Vertical);
        let across = track(&port, Axis::Horizontal);
        assert_eq!(down.left(), DevicePx(295.0));
        assert_eq!(down.right(), DevicePx(310.0));
        assert_eq!(down.top(), DevicePx(20.0));
        assert_eq!(
            down.bottom(),
            DevicePx(220.0),
            "the vertical bar owns the corner"
        );
        assert_eq!(across.left(), DevicePx(10.0));
        assert_eq!(
            across.right(),
            down.left(),
            "and the horizontal one stops where it begins"
        );
        assert_eq!(across.top(), DevicePx(205.0));
        assert_eq!(across.bottom(), DevicePx(220.0));
        assert!(!down.intersects(across));
        assert_eq!(
            down.union(across).union(port.content_box),
            port.inner,
            "the content and the two tracks cover the whole inner rectangle"
        );
    }

    #[test]
    fn a_thumb_is_as_long_as_the_fraction_that_is_visible_and_starts_where_the_offset_puts_it() {
        let mut port = tall();
        let at_top = thumb(&port, Axis::Vertical);
        assert_eq!(at_top.top(), DevicePx(0.0));
        assert_eq!(at_top.size.height, DevicePx(100.0), "half of it is visible");
        assert_eq!(at_top.left(), DevicePx(185.0));
        assert_eq!(at_top.size.width, DevicePx(15.0));

        port.offset = Point::new(DevicePx(0.0), DevicePx(200.0));
        let at_end = thumb(&port, Axis::Vertical);
        assert_eq!(
            at_end.bottom(),
            DevicePx(200.0),
            "the end of the content is the end of the track"
        );
    }

    #[test]
    fn a_gutter_with_nothing_to_scroll_has_a_track_and_no_thumb() {
        let mut port = tall();
        port.content = Size::new(DevicePx(185.0), DevicePx(200.0));
        assert!(!track(&port, Axis::Vertical).is_empty());
        assert!(thumb(&port, Axis::Vertical).is_empty());
    }

    #[test]
    fn a_track_the_content_does_not_end_flush_with_still_has_no_seam() {
        // A fractionally scaled window: the inner rectangle is on the grid and the content box is
        // whatever is left after a gutter that is not, so the two edges must be the same number
        // rather than two roundings of one.
        let port = Scrollport {
            inner: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(1201.0), DevicePx(801.0)),
            ),
            content_box: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(1182.25), DevicePx(801.0)),
            ),
            content: Size::new(DevicePx(1182.25), DevicePx(4000.0)),
            offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
        };
        let down = track(&port, Axis::Vertical);
        assert_eq!(down.left(), port.content_box.right());
        assert_eq!(down.right(), port.inner.right());
    }
}
