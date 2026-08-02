//! Lengths that are partly a fraction of something layout has not decided yet.

use zgui_geom::CssPx;

use crate::key::digest::Digest;

/// A length with an absolute part and a part that is a fraction of a basis.
///
/// Three text properties accept a percentage, and each measures it against something a style on its
/// own cannot know: `letter-spacing` against the font size, `word-spacing` against the advance of a
/// space in the face that was chosen, `text-indent` against the width the paragraph is being laid
/// out in. Resolving any of them here would mean inventing a basis, so the two parts are kept
/// separate and [`resolve`](LengthPercent::resolve) is called where the basis is real.
///
/// ```
/// use zgui_geom::CssPx;
/// use zgui_text_style::LengthPercent;
///
/// let indent = LengthPercent { length: CssPx(4.0), percent: 0.1 };
/// assert_eq!(indent.resolve(CssPx(600.0)), CssPx(64.0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LengthPercent {
    /// The part that is already a length.
    pub length: CssPx,
    /// The part that is a fraction of the basis, where `0.1` is ten percent.
    pub percent: f32,
}

impl LengthPercent {
    /// No length at all.
    pub const ZERO: Self = Self {
        length: CssPx::ZERO,
        percent: 0.0,
    };

    /// A plain length, with no percentage in it.
    pub const fn length(length: CssPx) -> Self {
        Self {
            length,
            percent: 0.0,
        }
    }

    /// The length this resolves to against a basis.
    pub fn resolve(self, basis: CssPx) -> CssPx {
        CssPx(self.length.0 + self.percent * basis.0)
    }

    /// Whether the value needs no basis at all, which is the overwhelmingly common case.
    pub fn is_absolute(self) -> bool {
        self.percent == 0.0
    }

    /// Mixes the value into a digest.
    pub(crate) fn hash_into(self, digest: &mut Digest) {
        digest.push_length(self.length);
        digest.push_f32(self.percent);
    }
}
