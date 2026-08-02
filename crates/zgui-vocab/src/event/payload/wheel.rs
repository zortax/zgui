//! Scrolling produced by a wheel, a trackpad or a scroll gesture.

use zgui_geom::{Css, CssPx, Point, Size};

use crate::event::payload::pointer::{PointerId, PointerKind};

/// How far a scroll asked to move, in the units the device reported.
///
/// The two units are not interchangeable and converting one to the other needs information this
/// type does not have. A notched wheel reports whole lines, and how far a line is depends on the
/// line height of the thing being scrolled; a trackpad reports pixels directly. Keeping the unit
/// on the value is what stops a magic constant from being invented at the point of use.
///
/// # Which way is positive
///
/// **A positive delta moves the scroll offset right and down.** It reveals content further right
/// and further down, exactly as a larger `scrollTop` does, so the content itself travels *up and
/// left* across the screen.
///
/// Several windowing libraries state the opposite convention, describing where the *content* goes
/// rather than where the offset goes. Converting into this one is a backend's job and is done once,
/// at the platform seam, because a convention assumed on one side of a boundary and documented on
/// the other is a convention that survives exactly until somebody reads only one side.
///
/// ```
/// use zgui_vocab::ScrollDelta;
///
/// let wheel = ScrollDelta::Lines { x: 0.0, y: -3.0 };
/// let trackpad = ScrollDelta::Pixels(zgui_geom::Size::new(
///     zgui_geom::CssPx(0.0),
///     zgui_geom::CssPx(-42.0),
/// ));
/// assert!(wheel.is_lines());
/// assert!(!trackpad.is_lines());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ScrollDelta {
    /// Whole lines, as a notched wheel reports them. Positive moves the offset right and down.
    Lines {
        /// Lines along the inline axis.
        x: f32,
        /// Lines along the block axis.
        y: f32,
    },
    /// CSS pixels, as a continuous surface reports them. Positive moves the offset right and down.
    Pixels(Size<CssPx, Css>),
}

impl ScrollDelta {
    /// Whether the delta is measured in lines.
    pub const fn is_lines(self) -> bool {
        matches!(self, Self::Lines { .. })
    }

    /// The delta in CSS pixels, given how far one line is on the element being scrolled.
    ///
    /// The line height has to come from the element's own resolved style, which is why it is an
    /// argument rather than a constant.
    ///
    /// ```
    /// use zgui_geom::CssPx;
    /// use zgui_vocab::ScrollDelta;
    ///
    /// let delta = ScrollDelta::Lines { x: 0.0, y: -3.0 };
    /// assert_eq!(delta.to_pixels(CssPx(16.0)).height, CssPx(-48.0));
    /// ```
    pub fn to_pixels(self, line_height: CssPx) -> Size<CssPx, Css> {
        match self {
            Self::Lines { x, y } => Size::new(CssPx(x * line_height.0), CssPx(y * line_height.0)),
            Self::Pixels(pixels) => pixels,
        }
    }
}

/// Where a scroll sits in a continuous gesture.
///
/// A trackpad scroll has a beginning, a middle and an end, and momentum after the fingers lift. A
/// notched wheel has none of that and reports every notch as [`ScrollPhase::Discrete`]. Anything
/// that latches onto a scroll — an overscroll bounce, a scroll-linked animation, a chained
/// container — needs the difference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ScrollPhase {
    /// One self-contained scroll, with no gesture around it.
    #[default]
    Discrete,
    /// The gesture has begun.
    Started,
    /// The gesture is continuing under the user's control.
    Moved,
    /// The user has let go and the platform is continuing the scroll.
    Momentum,
    /// The gesture is over.
    Ended,
}

/// What a scroll event carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    /// How far the scroll asked to move.
    pub delta: ScrollDelta,
    /// Where the scroll sits in a continuous gesture.
    pub phase: ScrollPhase,
    /// Where the pointer was, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
    /// Which pointer produced the scroll.
    pub id: PointerId,
    /// What kind of device produced the scroll.
    pub kind: PointerKind,
}

#[cfg(test)]
mod tests {
    use super::{ScrollDelta, ScrollPhase, WheelEvent};
    use crate::event::payload::pointer::{PointerId, PointerKind};
    use zgui_geom::{CssPx, Point, Size};

    #[test]
    fn lines_become_pixels_only_when_a_line_height_is_supplied() {
        let lines = ScrollDelta::Lines { x: 1.0, y: -3.0 };
        assert_eq!(
            lines.to_pixels(CssPx(20.0)),
            Size::new(CssPx(20.0), CssPx(-60.0))
        );
    }

    #[test]
    fn a_pixel_delta_passes_through_whatever_line_height_is_given() {
        let pixels = ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(-13.5)));
        assert_eq!(pixels.to_pixels(CssPx(16.0)), pixels.to_pixels(CssPx(99.0)));
    }

    #[test]
    fn a_wheel_notch_is_discrete_by_default() {
        let event = WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
            phase: ScrollPhase::default(),
            position: Point::new(CssPx(0.0), CssPx(0.0)),
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        };
        assert_eq!(event.phase, ScrollPhase::Discrete);
        assert!(event.delta.is_lines());
    }
}
