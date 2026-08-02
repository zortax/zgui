//! How far a scroll asked to move, once it is known what it was aimed at.
//!
//! A notched wheel reports lines and a trackpad reports pixels, and one of those two cannot be
//! converted into the other without knowing how tall a line is on the element being scrolled. The
//! conversion therefore takes that height rather than assuming one: a magic constant here is a
//! wheel that moves three lines of the wrong size on every document, and it is invisible because
//! it always moves *something*.

use zgui_geom::{Css, CssPx, Device, DevicePx, Scale, Size};
use zgui_vocab::ScrollDelta;

/// How far one line and one page are on the element a scroll is aimed at.
///
/// Both are properties of the scroll target rather than of the device: a line is that element's
/// own used line height, and a page is its visible extent less one line of overlap, which is what
/// keeps a line of context on screen across a page-sized jump.
///
/// ```
/// use zgui_geom::CssPx;
/// use zgui_input::ScrollUnits;
///
/// let units = ScrollUnits::for_scrollport(CssPx(20.0), CssPx(400.0));
/// assert_eq!(units.line, CssPx(20.0));
/// assert_eq!(units.page, CssPx(380.0));
///
/// // A scrollport shorter than a line still pages by at least one line, never by nothing.
/// let tiny = ScrollUnits::for_scrollport(CssPx(20.0), CssPx(12.0));
/// assert_eq!(tiny.page, CssPx(20.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollUnits {
    /// How tall one line is on the scroll target, in CSS pixels.
    pub line: CssPx,
    /// How far one page is on the scroll target, in CSS pixels.
    pub page: CssPx,
}

impl ScrollUnits {
    /// Units stated outright.
    pub const fn new(line: CssPx, page: CssPx) -> Self {
        Self { line, page }
    }

    /// Units for a scrollport `scrollport` CSS pixels along the scrolled axis whose text is `line`
    /// tall.
    pub fn for_scrollport(line: CssPx, scrollport: CssPx) -> Self {
        let page = (scrollport.0 - line.0).max(line.0);
        Self {
            line,
            page: CssPx(page),
        }
    }
}

/// How far this scroll asked to move, in CSS pixels.
///
/// ```
/// use zgui_geom::{CssPx, Size};
/// use zgui_input::ScrollUnits;
/// use zgui_input::normalize::scroll::to_css;
/// use zgui_vocab::ScrollDelta;
///
/// let units = ScrollUnits::for_scrollport(CssPx(20.0), CssPx(400.0));
/// let three_notches = ScrollDelta::Lines { x: 0.0, y: -3.0 };
/// assert_eq!(to_css(three_notches, units).height, CssPx(-60.0));
///
/// // A device that already reports pixels is not scaled by a line height it never used.
/// let swipe = ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(-7.5)));
/// assert_eq!(to_css(swipe, units).height, CssPx(-7.5));
/// ```
pub fn to_css(delta: ScrollDelta, units: ScrollUnits) -> Size<CssPx, Css> {
    delta.to_pixels(units.line)
}

/// The same, in the device pixels the scroll offsets are kept in.
pub fn to_device(
    delta: ScrollDelta,
    units: ScrollUnits,
    scale: Scale<Css, Device>,
) -> Size<DevicePx, Device> {
    let css = to_css(delta, units);
    Size::new(
        DevicePx(css.width.0 * scale.get()),
        DevicePx(css.height.0 * scale.get()),
    )
}

/// How far a whole-page scroll moves, in CSS pixels.
///
/// Separate from [`to_css`] because a page is not a delta a device reports: it is what the page
/// keys ask for, and what they ask for is a property of the element they are aimed at.
pub fn page(units: ScrollUnits, pages: f32) -> CssPx {
    CssPx(units.page.0 * pages)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, DevicePx, Scale, Size};
    use zgui_vocab::ScrollDelta;

    use super::{ScrollUnits, page, to_css, to_device};

    #[test]
    fn a_line_delta_is_measured_against_the_targets_own_line_height() {
        let delta = ScrollDelta::Lines { x: 0.0, y: -3.0 };
        let small = to_css(delta, ScrollUnits::new(CssPx(16.0), CssPx(400.0)));
        let large = to_css(delta, ScrollUnits::new(CssPx(32.0), CssPx(400.0)));
        assert_eq!(small.height, CssPx(-48.0));
        assert_eq!(large.height, CssPx(-96.0));
        assert_ne!(
            small, large,
            "the two documents scroll different distances for the same notch, which is the whole \
             reason the height is an argument"
        );
    }

    #[test]
    fn a_pixel_delta_reaches_the_device_through_the_scale_alone() {
        let delta = ScrollDelta::Pixels(Size::new(CssPx(2.0), CssPx(-10.0)));
        let moved = to_device(
            delta,
            ScrollUnits::new(CssPx(16.0), CssPx(400.0)),
            Scale::new(2.0),
        );
        assert_eq!(moved, Size::new(DevicePx(4.0), DevicePx(-20.0)));
    }

    #[test]
    fn a_page_is_the_scrollport_less_one_line_of_overlap() {
        let units = ScrollUnits::for_scrollport(CssPx(20.0), CssPx(500.0));
        assert_eq!(page(units, 1.0), CssPx(480.0));
        assert_eq!(page(units, -2.0), CssPx(-960.0));
    }
}
