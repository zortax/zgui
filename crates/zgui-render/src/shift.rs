//! Moving pixels that are already composed, instead of composing them again.
//!
//! A scroll moves every pixel of a scrollport, so the damage it raises is the whole port — and a
//! renderer that owns a persistent composed target already *has* those pixels, one whole-pixel
//! translation away from where they now belong. Shifting them and drawing only the strip the shift
//! left undefined is what turns a scroll frame from the cost of the port into the cost of the
//! travel.
//!
//! The saving is not mainly the copy. The emit walk is gated on damage intersection, so narrowing
//! the damage narrows the walk, the replays and the draw-order inserts with it — which on the
//! reference scroll workload is 77 % of the frame. See `docs/perf/scroll-frame.md`.
//!
//! # What this type does not decide
//!
//! Whether a shift is *allowed*. That depends on what else is drawn over the region, on whether
//! anything reads through it, and on whether the movement was rigid and whole-pixel — none of which
//! a renderer can see. The caller decides, and hands one of these over only for a frame it has
//! already established is that frame.

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};

/// A whole-pixel translation of a region of the composed target.
///
/// `by` is where the pixels are going, in the same sense as the damage: a list scrolled downwards
/// by three pixels moves its content upwards, so `by` is `(0, -3)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollShift {
    /// The region whose pixels move, in device pixels, already clipped to the surface.
    ///
    /// Nothing outside it is touched, and nothing from outside it is moved *in*: the region is the
    /// scrollport, which clips its own content, so what lies beyond the edge is not the content's
    /// to bring with it.
    pub port: Rect<i32, Device>,
    /// How far they move, in whole device pixels.
    ///
    /// Whole because a fraction of a pixel cannot be copied — resampling a composed frame is not
    /// the same picture, and it would move glyph coverage off the subpixel phase it was rasterised
    /// at. The caller refuses the shift rather than rounding it.
    pub by: (i32, i32),
}

impl ScrollShift {
    /// The region as it is after the move: where the copied pixels land.
    ///
    /// The intersection with the port, because a shift carries content off the edge rather than
    /// out of it.
    #[must_use]
    pub fn destination(&self) -> Option<Rect<i32, Device>> {
        let moved = Rect::new(
            Point::new(
                self.port.origin.x + self.by.0,
                self.port.origin.y + self.by.1,
            ),
            self.port.size,
        );
        moved.intersection(self.port)
    }

    /// Where the copied pixels come from.
    ///
    /// The same rectangle as [`ScrollShift::destination`] moved back, which is what a copy needs and
    /// is not simply the port: a shift that shows content off the bottom reads from lower down than
    /// it writes.
    #[must_use]
    pub fn source(&self) -> Option<Rect<i32, Device>> {
        self.destination().map(|to| {
            Rect::new(
                Point::new(to.origin.x - self.by.0, to.origin.y - self.by.1),
                to.size,
            )
        })
    }

    /// The parts of the port the shift leaves undefined, which the caller owes as damage.
    ///
    /// Up to two: a horizontal band and a vertical one, taken so that they do not overlap each
    /// other. A shift that moves the port clean off itself exposes all of it, and the whole port is
    /// returned as the single band.
    ///
    /// ```
    /// use zgui_geom::{Device, Point, Rect, Size};
    /// use zgui_render::ScrollShift;
    ///
    /// let port: Rect<i32, Device> = Rect::new(Point::new(0, 0), Size::new(100, 100));
    /// let shift = ScrollShift { port, by: (0, -10) };
    /// let mut exposed = Vec::new();
    /// shift.for_each_exposed(|band| exposed.push(band));
    /// assert_eq!(exposed.len(), 1, "a vertical scroll exposes one band");
    /// assert_eq!(exposed[0], Rect::new(Point::new(0, 90), Size::new(100, 10)));
    /// ```
    pub fn for_each_exposed(&self, mut band: impl FnMut(Rect<i32, Device>)) {
        let mut out = |rect: Rect<i32, Device>| {
            if !rect.is_empty() {
                band(rect);
            }
        };
        let Some(kept) = self.destination() else {
            // Nothing survives the move, so every pixel of the port is owed.
            out(self.port);
            return;
        };

        // The horizontal band: whatever the vertical movement uncovered, across the whole width.
        let (top, bottom) = (
            self.port.origin.y,
            self.port.origin.y + self.port.size.height,
        );
        if kept.origin.y > top {
            out(Rect::new(
                self.port.origin,
                Size::new(self.port.size.width, kept.origin.y - top),
            ));
        }
        let kept_bottom = kept.origin.y + kept.size.height;
        if kept_bottom < bottom {
            out(Rect::new(
                Point::new(self.port.origin.x, kept_bottom),
                Size::new(self.port.size.width, bottom - kept_bottom),
            ));
        }

        // The vertical band, taken only over the rows the horizontal band did not already claim, so
        // that the two are disjoint and a damage set of four rectangles is not spent on two.
        let (left, right) = (
            self.port.origin.x,
            self.port.origin.x + self.port.size.width,
        );
        if kept.origin.x > left {
            out(Rect::new(
                Point::new(left, kept.origin.y),
                Size::new(kept.origin.x - left, kept.size.height),
            ));
        }
        let kept_right = kept.origin.x + kept.size.width;
        if kept_right < right {
            out(Rect::new(
                Point::new(kept_right, kept.origin.y),
                Size::new(right - kept_right, kept.size.height),
            ));
        }
    }

    /// Puts every exposed band into `damage`.
    ///
    /// What a caller that accepted a shift owes: the shift moves the pixels it can and this is the
    /// rest of the port, which nothing has drawn yet.
    pub fn expose_into(&self, damage: &mut DamageSet) {
        self.for_each_exposed(|band| damage.absorb(band));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands(shift: &ScrollShift) -> Vec<Rect<i32, Device>> {
        let mut out = Vec::new();
        shift.for_each_exposed(|band| out.push(band));
        out
    }

    fn port() -> Rect<i32, Device> {
        Rect::new(Point::new(10, 20), Size::new(100, 200))
    }

    #[test]
    fn a_downward_scroll_exposes_the_bottom_band() {
        let shift = ScrollShift {
            port: port(),
            by: (0, -30),
        };
        let exposed = bands(&shift);
        assert_eq!(exposed.len(), 1);
        assert_eq!(
            exposed[0],
            Rect::new(Point::new(10, 190), Size::new(100, 30)),
            "the band is the height of the travel, at the trailing edge",
        );
        assert_eq!(
            shift.source(),
            Some(Rect::new(Point::new(10, 50), Size::new(100, 170))),
            "read from below where it is written",
        );
        assert_eq!(
            shift.destination(),
            Some(Rect::new(Point::new(10, 20), Size::new(100, 170))),
        );
    }

    #[test]
    fn an_upward_scroll_exposes_the_top_band() {
        let exposed = bands(&ScrollShift {
            port: port(),
            by: (0, 30),
        });
        assert_eq!(exposed.len(), 1);
        assert_eq!(
            exposed[0],
            Rect::new(Point::new(10, 20), Size::new(100, 30))
        );
    }

    #[test]
    fn a_diagonal_scroll_exposes_two_disjoint_bands() {
        let shift = ScrollShift {
            port: port(),
            by: (-10, -30),
        };
        let exposed = bands(&shift);
        assert_eq!(exposed.len(), 2);
        let (a, b) = (exposed[0], exposed[1]);
        assert_eq!(
            a.intersection(b),
            None,
            "the two bands must not overlap, or a damage set of four spends two on one region",
        );
        let covered: i32 = exposed.iter().map(|r| r.size.width * r.size.height).sum();
        let kept = shift.destination().expect("some of it survives");
        assert_eq!(
            covered + kept.size.width * kept.size.height,
            port().size.width * port().size.height,
            "what was kept and what was exposed is exactly the port",
        );
    }

    #[test]
    fn a_travel_past_the_whole_port_exposes_all_of_it() {
        let shift = ScrollShift {
            port: port(),
            by: (0, -500),
        };
        assert_eq!(shift.destination(), None);
        assert_eq!(bands(&shift), vec![port()]);
    }

    #[test]
    fn no_movement_exposes_nothing() {
        let shift = ScrollShift {
            port: port(),
            by: (0, 0),
        };
        assert_eq!(shift.destination(), Some(port()));
        assert!(bands(&shift).is_empty());
    }
}
