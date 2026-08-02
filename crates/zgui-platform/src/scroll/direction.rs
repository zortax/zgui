//! Which way a scroll goes, and who decided that.

use zgui_geom::{Css, CssPx, Size};

/// Whether the person's scroll-direction preference has already been applied to what arrives.
///
/// Every desktop lets somebody choose between "the content follows my fingers" and "the scrollbar
/// follows my fingers", and the choice is not a property of the mouse or of the application. What
/// differs between platforms is *where* the choice is applied — and applying it twice is the same
/// bug as never applying it, with the added property that it looks correct to whoever is testing
/// with the other preference set.
///
/// ```
/// use zgui_geom::{CssPx, Size};
/// use zgui_platform::scroll::ScrollDirection;
///
/// let down = Size::new(CssPx(0.0), CssPx(48.0));
/// assert_eq!(ScrollDirection::AsReported.apply(down), down);
/// assert_eq!(ScrollDirection::Inverted.apply(down).height, CssPx(-48.0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ScrollDirection {
    /// The desktop applied it before the event reached this program.
    ///
    /// This is the answer on every compositor and window server in ordinary use: the preference
    /// lives in the input stack — libinput on Linux, the window server on macOS, the driver on
    /// Windows — and a delta arrives already pointing the way the person asked for. A framework
    /// that flipped it again would be overriding a setting it does not own, on every machine.
    #[default]
    AsReported,
    /// The framework has to apply it, because this backend reads the device rather than the
    /// desktop.
    ///
    /// A backend with raw device access and no input stack above it reports which way the hardware
    /// turned and nothing more, and something has to turn that into which way the person wants to
    /// go. This is that something.
    Inverted,
}

impl ScrollDirection {
    /// The delta as the person asked for it.
    pub fn apply(self, delta: Size<CssPx, Css>) -> Size<CssPx, Css> {
        match self {
            Self::AsReported => delta,
            Self::Inverted => Size::new(CssPx(-delta.width.0), CssPx(-delta.height.0)),
        }
    }

    /// The same, for a delta measured in lines rather than pixels.
    pub fn apply_lines(self, x: f32, y: f32) -> (f32, f32) {
        match self {
            Self::AsReported => (x, y),
            Self::Inverted => (-x, -y),
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Size};

    use super::ScrollDirection;

    #[test]
    fn the_default_is_to_leave_a_preference_the_desktop_has_already_applied_alone() {
        assert_eq!(ScrollDirection::default(), ScrollDirection::AsReported);
    }

    #[test]
    fn inverting_twice_is_the_same_as_not_inverting() {
        let delta = Size::new(CssPx(3.0), CssPx(-17.0));
        let once = ScrollDirection::Inverted.apply(delta);
        assert_eq!(ScrollDirection::Inverted.apply(once), delta);
    }

    #[test]
    fn lines_and_pixels_are_flipped_the_same_way() {
        assert_eq!(
            ScrollDirection::Inverted.apply_lines(1.0, -3.0),
            (-1.0, 3.0)
        );
        assert_eq!(
            ScrollDirection::AsReported.apply_lines(1.0, -3.0),
            (1.0, -3.0)
        );
    }
}
