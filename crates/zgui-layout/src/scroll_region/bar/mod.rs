//! The bars a scrollport shows in the gutter it reserved.
//!
//! A scroll container that reserved a gutter has a strip of its padding box that its content was
//! moved out of. As in CSS, the strip is between the inner border edge and the padding: the bar
//! touches the border, and the padding stays on the content's side of the bar. Two things go
//! there. A *track* fills the whole strip whenever the gutter exists at all, and it is what catches
//! a press that pages the content. Its paint is the theme's to decide; the default paint draws
//! nothing there. A *thumb* sits on the track only when the content really can move, is as long as
//! the fraction of the scrollport that is visible, and sits where the scroll offset puts it.
//!
//! Every rectangle here is derived from numbers layout has already produced: the padding box, the
//! inner rectangle — the padding box less its padding — and the content box. The inner rectangle
//! and the content box differ by exactly the gutter. Deriving the thickness from their difference
//! rather than from the scrollbar width a second time keeps a track edge on the same number as
//! the edge it meets at a fractional scale, where two roundings of the same quantity part company
//! and leave a seam.

pub mod live;
pub mod place;
pub mod travel;

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::axis::Axis;
use crate::fragment::{FragmentKind, ScrollbarPart};
use crate::tree::store::ResolvedLayout;

/// One scrollport, as everything about its bars is derived from.
///
/// Held by value and built per question rather than cached anywhere: three of its four fields are
/// this frame's fragment geometry and the fourth is a scroll offset, which changes many times a
/// second and is deliberately not layout state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scrollport {
    /// The padding box: the inner border edge, which the bars touch.
    pub padding_box: Rect<DevicePx, Device>,
    /// The padding box less the padding: the content box plus whatever gutters were reserved.
    pub inner: Rect<DevicePx, Device>,
    /// The rectangle the content is laid out in, which is `inner` less the gutters.
    pub content_box: Rect<DevicePx, Device>,
    /// How far the content reaches inside it.
    pub content: Size<DevicePx, Device>,
    /// How far it has been scrolled, which may be past the end while an edge is displaced.
    pub offset: Point<DevicePx, Device>,
    /// The device scale factor, in device pixels per CSS pixel.
    pub scale: f32,
}

impl Scrollport {
    /// How thick the bar running along `axis` is, and nothing when that axis reserved no gutter.
    ///
    /// A bar that runs *down* the page is reserved out of the width, so the vertical bar's
    /// thickness is the width the content lost. This is the one place that has to be the right way
    /// round, and it is got right here rather than at each caller.
    pub fn thickness(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Vertical => self.inner.size.width.0 - self.content_box.size.width.0,
            Axis::Horizontal => self.inner.size.height.0 - self.content_box.size.height.0,
        }
    }

    /// Whether `axis` reserved a gutter, and therefore has a track.
    pub fn reserves(&self, axis: Axis) -> bool {
        self.thickness(axis) > 0.0
    }

    /// The padding between the content box and the bar on `axis`.
    ///
    /// The right padding for the vertical bar and the bottom padding for the horizontal one. Zero
    /// exactly when the box has no padding there, so a bar on an unpadded box starts on the same
    /// number the content box ends on.
    pub fn bar_padding(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Vertical => self.padding_box.right().0 - self.inner.right().0,
            Axis::Horizontal => self.padding_box.bottom().0 - self.inner.bottom().0,
        }
    }

    /// How much of the content is visible along `axis`.
    pub fn visible(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Vertical => self.content_box.size.height.0,
            Axis::Horizontal => self.content_box.size.width.0,
        }
    }

    /// How far the content reaches along `axis`.
    pub fn reach(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Vertical => self.content.height.0,
            Axis::Horizontal => self.content.width.0,
        }
    }

    /// The largest offset `axis` can be scrolled to, which is never negative.
    pub fn limit(&self, axis: Axis) -> f32 {
        (self.reach(axis) - self.visible(axis)).max(0.0)
    }

    /// How far `axis` is scrolled, clamped to what the content allows.
    ///
    /// Clamped rather than taken as written, because a container displaced past its end by an
    /// elastic gesture keeps following the gesture while its bar must go on showing the position
    /// the content actually has — a thumb that slid out of its own track would be the only thing on
    /// screen reporting an offset nothing else believes.
    pub fn at(&self, axis: Axis) -> f32 {
        let raw = match axis {
            Axis::Vertical => self.offset.y.0,
            Axis::Horizontal => self.offset.x.0,
        };
        raw.clamp(0.0, self.limit(axis))
    }
}

/// How much a content size may exceed its scrollport before the axis counts as scrollable.
///
/// The same tolerance the gutter decision uses, and for the same reason: both numbers come out of
/// one accumulation of floating-point additions, so a box whose content exactly fills it can report
/// a content size a fraction of a pixel larger. A thumb drawn for that is full length, cannot be
/// moved, and is drawn in every container in the document.
pub const EPSILON: f32 = 1.0 / 64.0;

/// Which pieces of scrollbar one box draws, in the order they are drawn.
///
/// Track before thumb on each axis, so the thumb is drawn over its own groove; horizontal before
/// vertical, which is only a tie-break and is fixed so that a box keeps its fragments' names across
/// frames.
///
/// A box that reserved no gutter draws nothing, which is every box in a document with nothing to
/// scroll.
pub fn kinds(layout: &ResolvedLayout) -> SmallVec<[FragmentKind; 4]> {
    let mut kinds = SmallVec::new();
    let content_box = layout.content_box();
    for axis in Axis::BOTH {
        let thickness = match axis {
            Axis::Vertical => layout.scrollbar_size.width.0,
            Axis::Horizontal => layout.scrollbar_size.height.0,
        };
        if thickness <= 0.0 {
            continue;
        }
        kinds.push(FragmentKind::Scrollbar {
            axis,
            part: ScrollbarPart::Track,
        });
        let (visible, reach) = match axis {
            Axis::Vertical => (content_box.size.height.0, layout.content_size.height.0),
            Axis::Horizontal => (content_box.size.width.0, layout.content_size.width.0),
        };
        if reach > visible + EPSILON {
            kinds.push(FragmentKind::Scrollbar {
                axis,
                part: ScrollbarPart::Thumb,
            });
        }
    }
    kinds
}

/// The rectangle one piece of one bar occupies.
pub fn rect(port: &Scrollport, axis: Axis, part: ScrollbarPart) -> Rect<DevicePx, Device> {
    match part {
        ScrollbarPart::Track => place::track(port, axis),
        ScrollbarPart::Thumb => place::thumb(port, axis),
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Rect, Size};

    use super::{Scrollport, kinds};
    use crate::axis::Axis;

    /// A two-hundred-pixel port with a fifteen-pixel gutter down its right, holding twice its own
    /// height of content.
    pub(super) fn tall() -> Scrollport {
        Scrollport {
            padding_box: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(200.0), DevicePx(200.0)),
            ),
            inner: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(200.0), DevicePx(200.0)),
            ),
            content_box: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(185.0), DevicePx(200.0)),
            ),
            content: Size::new(DevicePx(185.0), DevicePx(400.0)),
            offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
            scale: 1.0,
        }
    }

    #[test]
    fn the_vertical_bars_thickness_is_the_width_the_content_lost() {
        let port = tall();
        assert_eq!(port.thickness(Axis::Vertical), 15.0);
        assert_eq!(port.thickness(Axis::Horizontal), 0.0);
        assert!(port.reserves(Axis::Vertical));
        assert!(!port.reserves(Axis::Horizontal));
    }

    #[test]
    fn an_offset_past_the_end_is_reported_at_the_end() {
        let mut port = tall();
        port.offset = Point::new(DevicePx(0.0), DevicePx(340.0));
        assert_eq!(port.limit(Axis::Vertical), 200.0);
        assert_eq!(port.at(Axis::Vertical), 200.0);
        port.offset = Point::new(DevicePx(0.0), DevicePx(-40.0));
        assert_eq!(port.at(Axis::Vertical), 0.0);
    }

    #[test]
    fn a_reserved_gutter_draws_a_track_and_only_scrollable_content_draws_a_thumb() {
        use crate::fragment::{FragmentKind, ScrollbarPart};
        use crate::tree::store::ResolvedLayout;

        let mut layout = ResolvedLayout {
            size: Size::new(DevicePx(200.0), DevicePx(200.0)),
            content_size: Size::new(DevicePx(185.0), DevicePx(400.0)),
            scrollbar_size: Size::new(DevicePx(15.0), DevicePx(0.0)),
            ..ResolvedLayout::default()
        };
        assert_eq!(
            kinds(&layout).as_slice(),
            &[
                FragmentKind::Scrollbar {
                    axis: Axis::Vertical,
                    part: ScrollbarPart::Track
                },
                FragmentKind::Scrollbar {
                    axis: Axis::Vertical,
                    part: ScrollbarPart::Thumb
                },
            ]
        );

        // The same gutter, holding content that fits: the strip is still filled, and nothing in it
        // can be dragged.
        layout.content_size = Size::new(DevicePx(185.0), DevicePx(200.0));
        assert_eq!(
            kinds(&layout).as_slice(),
            &[FragmentKind::Scrollbar {
                axis: Axis::Vertical,
                part: ScrollbarPart::Track
            }]
        );

        layout.scrollbar_size = Size::new(DevicePx(0.0), DevicePx(0.0));
        assert!(kinds(&layout).is_empty(), "no gutter, no bar");
    }
}
