//! How the colour stripes of the display under a surface are arranged.

use crate::monitor::MonitorTransform;

/// The arrangement of the colour stripes of the display a surface is shown on.
///
/// Text with one coverage for each colour channel is correct only on a display with three
/// horizontal stripes for each pixel, in a known order. A backend states what its platform
/// reports. A layout this type has no variant for is [`SubpixelLayout::None`]: vertical stripes,
/// a turned output, and a surface on outputs that disagree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SubpixelLayout {
    /// The platform does not say.
    #[default]
    Unknown,
    /// The display has no horizontal stripes, or the system draws its own text with one coverage
    /// for each pixel.
    None,
    /// Red, green, blue, from left to right.
    HorizontalRgb,
    /// Blue, green, red, from left to right.
    HorizontalBgr,
}

impl SubpixelLayout {
    /// This layout as a surface sees it on an output that turns its picture by `transform`.
    ///
    /// A half turn and a mirror each reverse the order of the stripes. A quarter turn makes the
    /// stripes vertical, which is [`SubpixelLayout::None`].
    ///
    /// ```
    /// use zgui_platform::{MonitorTransform, SubpixelLayout};
    ///
    /// let panel = SubpixelLayout::HorizontalRgb;
    /// assert_eq!(panel.turned(MonitorTransform::Normal), SubpixelLayout::HorizontalRgb);
    /// assert_eq!(panel.turned(MonitorTransform::Rotated180), SubpixelLayout::HorizontalBgr);
    /// assert_eq!(panel.turned(MonitorTransform::Rotated90), SubpixelLayout::None);
    /// ```
    pub fn turned(self, transform: MonitorTransform) -> Self {
        let reversed = match self {
            Self::HorizontalRgb => Self::HorizontalBgr,
            Self::HorizontalBgr => Self::HorizontalRgb,
            other => return other,
        };
        match transform {
            MonitorTransform::Normal | MonitorTransform::Flipped180 => self,
            MonitorTransform::Rotated180 | MonitorTransform::Flipped => reversed,
            _ => Self::None,
        }
    }

    /// The layout of a surface that is on every output in `layouts`.
    ///
    /// An output of unknown layout has no say. Outputs that state different layouts give
    /// [`SubpixelLayout::None`], because no order is correct on all of them.
    ///
    /// ```
    /// use zgui_platform::SubpixelLayout::{self, HorizontalBgr, HorizontalRgb, Unknown};
    ///
    /// assert_eq!(SubpixelLayout::across([]), Unknown);
    /// assert_eq!(SubpixelLayout::across([Unknown, HorizontalRgb]), HorizontalRgb);
    /// assert_eq!(SubpixelLayout::across([HorizontalRgb, HorizontalBgr]), SubpixelLayout::None);
    /// ```
    pub fn across(layouts: impl IntoIterator<Item = Self>) -> Self {
        let mut stated = layouts
            .into_iter()
            .filter(|layout| *layout != Self::Unknown);
        let Some(first) = stated.next() else {
            return Self::Unknown;
        };
        if stated.all(|layout| layout == first) {
            first
        } else {
            Self::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubpixelLayout::{self, HorizontalBgr, HorizontalRgb, Unknown};
    use crate::monitor::MonitorTransform;

    #[test]
    fn a_turn_reverses_the_stripes_or_makes_them_vertical() {
        for (transform, seen) in [
            (MonitorTransform::Normal, HorizontalRgb),
            (MonitorTransform::Rotated180, HorizontalBgr),
            (MonitorTransform::Flipped, HorizontalBgr),
            (MonitorTransform::Flipped180, HorizontalRgb),
            (MonitorTransform::Rotated90, SubpixelLayout::None),
            (MonitorTransform::Rotated270, SubpixelLayout::None),
            (MonitorTransform::Flipped90, SubpixelLayout::None),
            (MonitorTransform::Flipped270, SubpixelLayout::None),
        ] {
            assert_eq!(HorizontalRgb.turned(transform), seen, "{transform:?}");
        }
        assert_eq!(
            HorizontalBgr.turned(MonitorTransform::Rotated180),
            HorizontalRgb
        );
        for layout in [Unknown, SubpixelLayout::None] {
            assert_eq!(layout.turned(MonitorTransform::Rotated90), layout);
            assert_eq!(layout.turned(MonitorTransform::Rotated180), layout);
        }
    }

    #[test]
    fn outputs_that_disagree_have_no_layout() {
        assert_eq!(SubpixelLayout::across([Unknown, Unknown]), Unknown);
        assert_eq!(
            SubpixelLayout::across([HorizontalBgr, Unknown, HorizontalBgr]),
            HorizontalBgr
        );
        assert_eq!(
            SubpixelLayout::across([HorizontalRgb, SubpixelLayout::None]),
            SubpixelLayout::None
        );
        assert_eq!(
            SubpixelLayout::across([SubpixelLayout::None]),
            SubpixelLayout::None
        );
    }
}
