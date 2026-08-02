//! Where a glyph's tile is drawn, and the phase it was rasterised for.
//!
//! # Why the two are one type
//!
//! A glyph's position along the baseline is fractional, and a rasteriser cannot be asked for an
//! unbounded set of positions. The answer everywhere is to split the position in two: a whole pixel
//! the tile is drawn at, and a quantised fraction the outline is shifted by *before* it is turned
//! into coverage. The two halves are only meaningful together — the tile carries the fraction in its
//! pixels, so drawing it anywhere but at the matching whole pixel puts the ink somewhere the shaper
//! never asked for.
//!
//! Computing them separately is how they come apart. Taking the phase by rounding the fraction and
//! the pixel by flooring the position disagrees for every position whose fraction rounds up to a
//! whole pixel: the phase says *no shift* while the floor says *the pixel below*, and the glyph
//! lands very nearly a whole pixel to the left of where it belongs. Along a line of proportional
//! text that is one letter crowding its neighbour and the next opening a gap, which is why the split
//! is performed once, here, and both halves are read off one value.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::glyph::key::SubpixelOffset;

/// One glyph's horizontal position, split into the pixel it is drawn at and the phase it is
/// rasterised for.
///
/// The two always describe the same quantised position: `pen + offset.to_pixels()` is the position
/// asked for, rounded to the nearest subpixel step, and is never further than half a step from it.
///
/// ```
/// use zgui_text::{PenPosition, SubpixelOffset};
///
/// let flush = PenPosition::of(12.0);
/// assert_eq!((flush.pen(), flush.offset()), (12.0, SubpixelOffset(0)));
///
/// // A fraction that rounds up to the next pixel moves the pen, rather than wrapping the phase
/// // back to zero and leaving the pen behind.
/// let nearly = PenPosition::of(12.9);
/// assert_eq!((nearly.pen(), nearly.offset()), (13.0, SubpixelOffset(0)));
/// assert_eq!(nearly.quantised(), 13.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenPosition {
    /// The whole pixel the tile's left edge is measured from.
    pen: f32,
    /// The fraction of a pixel the outline was shifted by before it was rasterised.
    offset: SubpixelOffset,
}

impl PenPosition {
    /// Splits one absolute horizontal position.
    ///
    /// The position is the glyph's origin on the baseline in absolute device pixels — everything
    /// that moves the run, the line box's own left edge included, has to be in it. Splitting a
    /// position relative to something that is itself at a fraction of a pixel takes the phase of the
    /// wrong number and leaves the tile straddling two pixels of the surface.
    pub fn of(position: f32) -> Self {
        let steps = f32::from(SubpixelOffset::STEPS);
        // Quantise first, then split, so that the two halves cannot disagree about which pixel the
        // position belongs to.
        let quantised = (position * steps).round() / steps;
        let pen = quantised.floor();
        let step = ((quantised - pen) * steps).round() as u8;
        Self {
            pen,
            offset: SubpixelOffset(step % SubpixelOffset::STEPS),
        }
    }

    /// The whole pixel the glyph's tile is drawn from.
    pub fn pen(self) -> f32 {
        self.pen
    }

    /// The phase the glyph is rasterised at, which is what its cache key carries.
    pub fn offset(self) -> SubpixelOffset {
        self.offset
    }

    /// The position the two halves stand for together.
    pub fn quantised(self) -> f32 {
        self.pen + self.offset.to_pixels()
    }

    /// Where the pixels of an image rasterised for this position land on the surface.
    ///
    /// `baseline` is the absolute vertical position of the baseline the glyph sits on; it is
    /// rounded here, because there is no vertical phase and a tile drawn at half a pixel is a tile
    /// resampled. `placement` is the top-left corner of the image relative to the glyph's origin,
    /// measured rightwards and *upwards*, so the top edge is subtracted rather than added.
    ///
    /// The fraction of the position is already in the image's pixels and is deliberately not added
    /// again here; the rectangle this returns therefore falls on whole device pixels in both axes,
    /// which is the only way a coverage tile reaches the surface unresampled.
    ///
    /// ```
    /// use zgui_geom::{DevicePx, Point, Size};
    /// use zgui_text::PenPosition;
    ///
    /// let bounds = PenPosition::of(12.9).bounds(
    ///     30.2,
    ///     Point::new(DevicePx(1.0), DevicePx(8.0)),
    ///     Size::new(6, 9),
    /// );
    /// assert_eq!(bounds.origin, Point::new(DevicePx(14.0), DevicePx(22.0)));
    /// assert_eq!(bounds.size, Size::new(DevicePx(6.0), DevicePx(9.0)));
    /// ```
    pub fn bounds(
        self,
        baseline: f32,
        placement: Point<DevicePx, Device>,
        size: Size<u32, Device>,
    ) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(
                DevicePx(self.pen + placement.x.0),
                DevicePx(baseline.round() - placement.y.0),
            ),
            Size::new(DevicePx(size.width as f32), DevicePx(size.height as f32)),
        )
    }
}

#[cfg(test)]
mod tests;
