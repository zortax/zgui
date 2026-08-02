//! A scrolling element's offset changing.

use zgui_geom::{Css, CssPx, Point, Size};

/// What a scroll event carries: where the content now sits and how much of it there is.
///
/// All three are here because a scroll handler almost always needs the ratio rather than the raw
/// offset — how far down the content is, whether the end has been reached — and computing that
/// from the offset alone requires reading geometry the handler is not allowed to read while
/// handling an event.
///
/// A scroll is reported after the offset has already been applied, so a handler that repositions
/// something in response is in time to be drawn in the same frame rather than one behind.
///
/// ```
/// use zgui_geom::{CssPx, Point, Size};
/// use zgui_vocab::ScrollEvent;
///
/// let event = ScrollEvent {
///     offset: Point::new(CssPx(0.0), CssPx(400.0)),
///     content_size: Size::new(CssPx(300.0), CssPx(1_000.0)),
///     scrollport: Size::new(CssPx(300.0), CssPx(600.0)),
/// };
/// assert!(event.is_at_end_vertically());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    /// How far the content has been scrolled from its start, in CSS pixels.
    pub offset: Point<CssPx, Css>,
    /// How large the scrolled content is.
    pub content_size: Size<CssPx, Css>,
    /// How large the window onto that content is.
    pub scrollport: Size<CssPx, Css>,
}

impl ScrollEvent {
    /// How far the content can be scrolled along each axis before it runs out.
    pub fn scrollable(&self) -> Size<CssPx, Css> {
        Size::new(
            CssPx((self.content_size.width.0 - self.scrollport.width.0).max(0.0)),
            CssPx((self.content_size.height.0 - self.scrollport.height.0).max(0.0)),
        )
    }

    /// Whether the content is scrolled as far down as it goes.
    ///
    /// Content that fits entirely inside its window is at its end, which is what an
    /// infinite-scroll trigger has to treat as "load more" rather than "wait".
    pub fn is_at_end_vertically(&self) -> bool {
        self.offset.y.0 >= self.scrollable().height.0
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollEvent;
    use zgui_geom::{CssPx, Point, Size};

    fn event(offset: f32, content: f32, port: f32) -> ScrollEvent {
        ScrollEvent {
            offset: Point::new(CssPx(0.0), CssPx(offset)),
            content_size: Size::new(CssPx(100.0), CssPx(content)),
            scrollport: Size::new(CssPx(100.0), CssPx(port)),
        }
    }

    #[test]
    fn the_scrollable_extent_never_goes_negative() {
        assert_eq!(event(0.0, 200.0, 500.0).scrollable().height, CssPx(0.0));
        assert_eq!(event(0.0, 900.0, 500.0).scrollable().height, CssPx(400.0));
    }

    #[test]
    fn content_that_fits_is_already_at_its_end() {
        assert!(event(0.0, 200.0, 500.0).is_at_end_vertically());
        assert!(!event(399.0, 900.0, 500.0).is_at_end_vertically());
        assert!(event(400.0, 900.0, 500.0).is_at_end_vertically());
    }
}
