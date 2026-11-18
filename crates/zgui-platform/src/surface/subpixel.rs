//! How the colour stripes of the display under a surface are arranged.

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
