//! `line-height`, kept in the form it was authored in.

use zgui_geom::CssPx;

use crate::key::digest::Digest;

/// How tall each line box is, as authored.
///
/// The three forms are kept apart rather than resolved to a length, because two of them cannot be
/// resolved until a face is known: `normal` is the face's own preferred line spacing, and a
/// multiple is of the font size in the run that face was chosen for. Resolving early would pick
/// one face's answer for a paragraph that uses several.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// `line-height: normal` — whatever spacing the face itself asks for.
    Normal,
    /// A unitless multiple of the font size.
    Number(f32),
    /// An absolute length.
    Length(CssPx),
}

impl LineHeight {
    /// The resolved height for a face at `font_size` whose own preferred spacing is `metrics`.
    ///
    /// ```
    /// use zgui_geom::CssPx;
    /// use zgui_text_style::LineHeight;
    ///
    /// let normal = LineHeight::Normal;
    /// assert_eq!(normal.resolve(CssPx(16.0), CssPx(18.4)), CssPx(18.4));
    /// assert_eq!(LineHeight::Number(1.5).resolve(CssPx(16.0), CssPx(18.4)), CssPx(24.0));
    /// ```
    pub fn resolve(self, font_size: CssPx, face_line_height: CssPx) -> CssPx {
        match self {
            Self::Normal => face_line_height,
            Self::Number(multiple) => CssPx(font_size.0 * multiple),
            Self::Length(length) => length,
        }
    }

    /// Mixes the value into a digest.
    pub(crate) fn hash_into(self, digest: &mut Digest) {
        match self {
            Self::Normal => digest.push_tag(0),
            Self::Number(multiple) => {
                digest.push_tag(1);
                digest.push_f32(multiple);
            }
            Self::Length(length) => {
                digest.push_tag(2);
                digest.push_length(length);
            }
        }
    }
}
