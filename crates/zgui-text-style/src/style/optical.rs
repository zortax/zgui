//! `font-optical-sizing`.

/// Whether a variable face's optical-size axis follows the font size.
///
/// A face with an `opsz` axis draws heavier stems and looser spacing at small sizes and the reverse
/// at display sizes. Under `auto` the axis is set to the font size in points, which is what makes
/// the same family look right at 11px and at 96px; under `none` the axis is left where it is and
/// the face is drawn at one design regardless.
///
/// The setting changes outlines *and* advances, so it belongs to the shaping half of a style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OpticalSizing {
    /// `auto` — the axis follows the font size.
    #[default]
    Auto,
    /// `none` — the axis is left alone.
    None,
}

/// The `opsz` axis tag, which is the axis [`OpticalSizing::Auto`] drives.
pub const OPTICAL_SIZE_AXIS: u32 = crate::style::face::tag(b"opsz");
