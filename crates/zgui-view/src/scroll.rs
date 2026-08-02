//! Where a scroll container is, and how to ask it to move.

use zgui_geom::{Device, DevicePx, Point, Size};

/// A scroll container's position, content extent and visible extent.
///
/// All three travel together because none of them means anything alone: an offset is only
/// interpretable against the extent it is an offset into, and "am I at the bottom" is a question
/// about all three.
///
/// ```
/// use zgui_geom::{DevicePx, Point, Size};
/// use zgui_view::ScrollPosition;
///
/// let position = ScrollPosition {
///     offset: Point::new(DevicePx(0.0), DevicePx(120.0)),
///     content_size: Size::new(DevicePx(400.0), DevicePx(1000.0)),
///     scrollport: Size::new(DevicePx(400.0), DevicePx(880.0)),
/// };
/// assert!(position.is_at_end_vertically());
/// ```
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct ScrollPosition {
    /// How far the content has been scrolled, from its start.
    pub offset: Point<DevicePx, Device>,
    /// The full extent of the scrolled content.
    pub content_size: Size<DevicePx, Device>,
    /// The extent that is visible.
    pub scrollport: Size<DevicePx, Device>,
}

impl ScrollPosition {
    /// How far this container can still be scrolled, in each axis, never below zero.
    pub fn remaining(self) -> Size<DevicePx, Device> {
        Size::new(
            DevicePx(
                (self.content_size.width.0 - self.scrollport.width.0 - self.offset.x.0).max(0.0),
            ),
            DevicePx(
                (self.content_size.height.0 - self.scrollport.height.0 - self.offset.y.0).max(0.0),
            ),
        )
    }

    /// Whether the content is scrolled as far down as it goes.
    pub fn is_at_end_vertically(self) -> bool {
        self.remaining().height.0 <= f32::EPSILON
    }

    /// Whether the content is scrolled as far right as it goes.
    pub fn is_at_end_horizontally(self) -> bool {
        self.remaining().width.0 <= f32::EPSILON
    }
}

/// Where a scroll should end up.
#[derive(Copy, Clone, PartialEq, Debug)]
#[non_exhaustive]
pub enum ScrollTarget {
    /// Bring the node itself into view, moving as little as possible.
    IntoView,
    /// Bring the node into view and put it at the start of the scrollport.
    IntoViewStart,
    /// Bring the node into view and put it at the end of the scrollport.
    IntoViewEnd,
    /// Bring the node into view and centre it.
    IntoViewCenter,
    /// Scroll this container to an absolute offset.
    Offset(Point<DevicePx, Device>),
    /// Scroll this container by a relative amount.
    By(Point<DevicePx, Device>),
}

/// Whether a scroll animates or jumps.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum ScrollBehavior {
    /// Move immediately, in one frame.
    #[default]
    Instant,
    /// Animate to the destination.
    Smooth,
}

#[cfg(test)]
mod tests {
    use super::ScrollPosition;
    use zgui_geom::{DevicePx, Point, Size};

    fn position(offset: f32, content: f32, port: f32) -> ScrollPosition {
        ScrollPosition {
            offset: Point::new(DevicePx(0.0), DevicePx(offset)),
            content_size: Size::new(DevicePx(port), DevicePx(content)),
            scrollport: Size::new(DevicePx(port), DevicePx(port)),
        }
    }

    #[test]
    fn the_remainder_is_zero_at_the_end_and_positive_before_it() {
        assert_eq!(
            position(120.0, 1000.0, 880.0).remaining().height,
            DevicePx(0.0)
        );
        assert_eq!(
            position(0.0, 1000.0, 880.0).remaining().height,
            DevicePx(120.0)
        );
    }

    #[test]
    fn a_container_shorter_than_its_port_is_already_at_the_end() {
        let short = position(0.0, 100.0, 880.0);
        assert!(short.is_at_end_vertically());
        assert_eq!(short.remaining().height, DevicePx(0.0));
    }

    #[test]
    fn the_horizontal_axis_is_answered_independently() {
        let wide = ScrollPosition {
            offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
            content_size: Size::new(DevicePx(2000.0), DevicePx(100.0)),
            scrollport: Size::new(DevicePx(500.0), DevicePx(100.0)),
        };
        assert!(!wide.is_at_end_horizontally());
        assert!(wide.is_at_end_vertically());
    }
}
