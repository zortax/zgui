//! The axes a face is selected and instanced along, beyond its family.

use crate::key::digest::Digest;

/// How far a face leans.
///
/// `italic` carries no angle in CSS, but face selection is a distance along an axis rather than a
/// keyword match, so it resolves to the angle the axis is conventionally instanced at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontSlant {
    /// `font-style: normal` — upright.
    Upright,
    /// `font-style: italic` — a face drawn with italic letterforms, or synthesised.
    Italic,
    /// `font-style: oblique <angle>` — a slant of this many degrees, positive leaning forward.
    Oblique(f32),
}

impl FontSlant {
    /// The slant in degrees, which is the form a variable face's `slnt` axis takes.
    pub fn degrees(self) -> f32 {
        match self {
            Self::Upright => 0.0,
            Self::Italic => DEFAULT_OBLIQUE_DEGREES,
            Self::Oblique(degrees) => degrees,
        }
    }

    /// Mixes the slant into a digest.
    pub(crate) fn hash_into(self, digest: &mut Digest) {
        match self {
            Self::Upright => digest.push_tag(0),
            Self::Italic => digest.push_tag(1),
            Self::Oblique(degrees) => {
                digest.push_tag(2);
                digest.push_f32(degrees);
            }
        }
    }
}

/// The slant `italic` stands in for when a face is instanced along an angle axis.
pub const DEFAULT_OBLIQUE_DEGREES: f32 = 14.0;

/// One `font-variation-settings` entry: a variable-font axis and where to sit on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontVariation {
    /// The four-character axis tag, packed big-endian so that `wght` sorts as it reads.
    pub tag: u32,
    /// The coordinate on that axis, in the axis's own units.
    pub value: f32,
}

/// One `font-feature-settings` entry: an OpenType feature and its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontFeature {
    /// The four-character feature tag, packed big-endian.
    pub tag: u32,
    /// The feature's value; zero is off and one is on for a boolean feature.
    pub value: u32,
}

/// Packs a four-character tag into the integer form both settings use.
///
/// ```
/// use zgui_text_style::tag;
///
/// assert_eq!(tag(b"wght"), u32::from_be_bytes(*b"wght"));
/// ```
pub const fn tag(bytes: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*bytes)
}
