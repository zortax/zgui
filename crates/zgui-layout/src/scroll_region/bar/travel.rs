//! How far a thumb can move, and what a position in that range means.
//!
//! Everything a scrollbar does beyond being drawn is one conversion and its inverse: an offset into
//! the content becomes a position in the track, and a position in the track becomes an offset. They
//! are stated once, here, because two implementations of a mapping that is nearly right in both
//! directions is a thumb that drifts away from the pointer over a long drag.

use crate::axis::Axis;
use crate::scroll_region::bar::Scrollport;

/// How short a thumb may become, in device pixels.
///
/// A thumb sized purely by the visible fraction of a very long document is two pixels tall and
/// cannot be grabbed. Clamping it changes what the rest of the track means — the thumb no longer
/// travels the track's full length — which is why the travel is computed as its own number rather
/// than assumed to be the track less nothing.
pub const MIN_THUMB: f32 = 20.0;

/// The range a thumb moves in, along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Travel {
    /// Where the range begins, in the same space as the track.
    pub start: f32,
    /// How long it is: the part of the track the content box spans.
    pub length: f32,
    /// How long the thumb itself is.
    pub thumb: f32,
    /// The largest offset the content can be scrolled to.
    pub limit: f32,
}

impl Travel {
    /// How far the thumb's near edge can move: the range less the thumb's own length.
    pub fn free(&self) -> f32 {
        (self.length - self.thumb).max(0.0)
    }

    /// Whether nothing can move, and there is therefore no thumb to draw or to drag.
    pub fn is_still(&self) -> bool {
        self.limit <= 0.0 || self.free() <= 0.0
    }

    /// Where in its range a thumb showing `offset` sits, from zero at the top to one at the end.
    pub fn fraction(&self, offset: f32) -> f32 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        (offset / self.limit).clamp(0.0, 1.0)
    }

    /// The offset a thumb whose near edge is at `at` is showing.
    ///
    /// The exact inverse of [`Travel::fraction`] scaled by the limit, so a drag that puts the thumb
    /// back where it started puts the content back where it started too.
    pub fn offset_at(&self, at: f32) -> f32 {
        let free = self.free();
        if free <= 0.0 {
            return 0.0;
        }
        ((at - self.start) / free).clamp(0.0, 1.0) * self.limit
    }

    /// Where the thumb's near edge is when the content is at `offset`.
    pub fn thumb_at(&self, offset: f32) -> f32 {
        self.start + self.free() * self.fraction(offset)
    }
}

/// The range the thumb on `axis` moves in.
pub fn of(port: &Scrollport, axis: Axis) -> Travel {
    let track = super::place::track(port, axis);
    let (start, length) = match axis {
        Axis::Vertical => (track.top().0, port.visible(axis)),
        Axis::Horizontal => (track.left().0, port.visible(axis)),
    };
    let reach = port.reach(axis);
    let visible = port.visible(axis);
    let thumb = if reach <= 0.0 || visible >= reach {
        length
    } else {
        (length * (visible / reach)).clamp(MIN_THUMB.min(length), length)
    };
    Travel {
        start,
        length,
        thumb,
        limit: (reach - visible).max(0.0),
    }
}

/// Where a press on the track at `at` puts the offset: one screenful towards the press.
///
/// Paging rather than jumping, which is what a track press does everywhere else and what makes the
/// track useful for reading rather than only for seeking. The direction is decided against the
/// thumb's own edges, so a press between the thumb and the end of the track always moves the
/// content that way however small the remaining gap is.
pub fn paged(port: &Scrollport, axis: Axis, at: f32) -> f32 {
    let travel = of(port, axis);
    let held = port.at(axis);
    let near = travel.thumb_at(held);
    let by = port.visible(axis);
    let to = if at < near { held - by } else { held + by };
    to.clamp(0.0, travel.limit)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Size};

    use super::{MIN_THUMB, of, paged};
    use crate::axis::Axis;
    use crate::scroll_region::bar::tests::tall;

    #[test]
    fn the_range_is_the_track_less_the_thumb_and_the_two_conversions_are_inverses() {
        let travel = of(&tall(), Axis::Vertical);
        assert_eq!(travel.length, 200.0);
        assert_eq!(travel.thumb, 100.0);
        assert_eq!(travel.free(), 100.0);
        assert_eq!(travel.limit, 200.0);
        for offset in [0.0, 37.0, 199.0, 200.0] {
            let there = travel.thumb_at(offset);
            assert!(
                (travel.offset_at(there) - offset).abs() < 1e-3,
                "{offset} came back as {}",
                travel.offset_at(there)
            );
        }
    }

    #[test]
    fn a_very_long_document_still_has_a_thumb_that_can_be_grabbed() {
        let mut port = tall();
        port.content = Size::new(DevicePx(185.0), DevicePx(400_000.0));
        let travel = of(&port, Axis::Vertical);
        assert_eq!(travel.thumb, MIN_THUMB);
        assert_eq!(travel.free(), 180.0);
        // And the whole of the content is still reachable, because the range shrank with the thumb.
        assert!((travel.offset_at(travel.start + travel.free()) - travel.limit).abs() < 1e-3);
    }

    #[test]
    fn a_press_above_the_thumb_pages_back_and_one_below_it_pages_on() {
        let mut port = tall();
        port.offset = Point::new(DevicePx(0.0), DevicePx(100.0));
        // The thumb runs from 50 to 150 with the content halfway down.
        assert_eq!(paged(&port, Axis::Vertical, 10.0), 0.0, "a screenful back");
        assert_eq!(
            paged(&port, Axis::Vertical, 190.0),
            200.0,
            "a screenful on, clamped to the end"
        );
    }

    #[test]
    fn a_thumb_that_fills_its_track_cannot_be_moved() {
        let mut port = tall();
        port.content = Size::new(DevicePx(185.0), DevicePx(200.0));
        let travel = of(&port, Axis::Vertical);
        assert!(travel.is_still());
        assert_eq!(travel.offset_at(120.0), 0.0);
    }
}
