//! The metrics one resolved face reports at one size.

use zgui_geom::CssPx;

/// Font metrics for one resolved face at one size, in CSS pixels.
///
/// These are what the font-relative CSS units resolve against: `ex` against
/// [`x_height`](FaceMetrics::x_height), `ch` against
/// [`zero_advance`](FaceMetrics::zero_advance), `cap` against
/// [`cap_height`](FaceMetrics::cap_height), `ic` against [`ic_width`](FaceMetrics::ic_width).
///
/// Four of the seven are optional in a specific sense: `None` means *this face does not carry the
/// metric*, not "unknown" and not "zero". A unit whose metric is absent falls back to a fraction of
/// the font size, which is what makes the distinction matter — reporting zero would collapse every
/// `ex` length in the document instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceMetrics {
    /// Height of a lowercase `x`, when the face declares one.
    pub x_height: Option<CssPx>,
    /// Advance width of the digit zero, when the face has one.
    pub zero_advance: Option<CssPx>,
    /// Height of a capital letter, when the face declares one.
    pub cap_height: Option<CssPx>,
    /// Advance width of the CJK water ideograph, when the face has one.
    pub ic_width: Option<CssPx>,
    /// Distance from the baseline to the top of the face's content area.
    ///
    /// Not optional: every face has one, and a line box cannot be built without it.
    pub ascent: CssPx,
    /// How far a first-level superscript or subscript is scaled down, as a fraction.
    pub script_percent: Option<f32>,
    /// How far a second-level script is scaled down, as a fraction.
    pub script_script_percent: Option<f32>,
}

impl FaceMetrics {
    /// The x-height, falling back to the fraction of the font size used when a face declares none.
    pub fn x_height_or_fallback(&self, size: CssPx) -> CssPx {
        self.x_height.unwrap_or(CssPx(size.0 * X_HEIGHT_FALLBACK))
    }

    /// The zero advance, falling back to half the font size, or to all of it when the text is set
    /// upright.
    ///
    /// `upright` is not the same question as vertical text: a vertical writing mode that rotates
    /// its glyphs still measures a `0` across its own advance, and only text whose glyphs stay
    /// upright measures it down the column instead.
    pub fn zero_advance_or_fallback(&self, size: CssPx, upright: bool) -> CssPx {
        self.zero_advance.unwrap_or(if upright {
            size
        } else {
            CssPx(size.0 * ZERO_ADVANCE_FALLBACK)
        })
    }

    /// The cap height, falling back to the face's ascent rather than to a fraction of the size.
    ///
    /// The ascent is what a face that declares no cap height is measured by, so it takes no `size`
    /// argument at all: the answer is already a property of the face at the size it was queried at.
    pub fn cap_height_or_fallback(&self) -> CssPx {
        self.cap_height.unwrap_or(self.ascent)
    }

    /// The ideograph advance, which falls back to the full font size — an ideograph is square.
    pub fn ic_width_or_fallback(&self, size: CssPx) -> CssPx {
        self.ic_width.unwrap_or(size)
    }
}

/// The fraction of the font size `ex` resolves to when the face declares no x-height.
pub const X_HEIGHT_FALLBACK: f32 = 0.5;

/// The fraction of the font size `ch` resolves to when the face has no digit zero and its glyphs
/// are not set upright.
pub const ZERO_ADVANCE_FALLBACK: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::{FaceMetrics, X_HEIGHT_FALLBACK, ZERO_ADVANCE_FALLBACK};
    use zgui_geom::CssPx;

    /// A face that declares nothing but the one metric every face has.
    fn bare(ascent: CssPx) -> FaceMetrics {
        FaceMetrics {
            ascent,
            ..FaceMetrics::default()
        }
    }

    #[test]
    fn an_absent_metric_falls_back_and_a_present_one_is_used_as_declared() {
        let size = CssPx(20.0);
        let bare = bare(CssPx(18.0));
        assert_eq!(
            bare.x_height_or_fallback(size),
            CssPx(20.0 * X_HEIGHT_FALLBACK)
        );
        assert_eq!(
            bare.zero_advance_or_fallback(size, false),
            CssPx(20.0 * ZERO_ADVANCE_FALLBACK)
        );
        assert_eq!(bare.ic_width_or_fallback(size), size);

        let declared = FaceMetrics {
            x_height: Some(CssPx(9.0)),
            zero_advance: Some(CssPx(11.0)),
            cap_height: Some(CssPx(14.0)),
            ic_width: Some(CssPx(19.0)),
            ..bare
        };
        assert_eq!(declared.x_height_or_fallback(size), CssPx(9.0));
        assert_eq!(declared.zero_advance_or_fallback(size, false), CssPx(11.0));
        assert_eq!(declared.zero_advance_or_fallback(size, true), CssPx(11.0));
        assert_eq!(declared.cap_height_or_fallback(), CssPx(14.0));
        assert_eq!(declared.ic_width_or_fallback(size), CssPx(19.0));
    }

    #[test]
    fn an_undeclared_cap_height_is_the_ascent_and_not_a_fraction_of_the_size() {
        // The two are deliberately far apart, so a fallback that reached for the size instead of
        // the face would not coincidentally agree.
        let metrics = bare(CssPx(18.0));
        assert_eq!(metrics.cap_height_or_fallback(), CssPx(18.0));
        assert_ne!(metrics.cap_height_or_fallback(), CssPx(20.0 * 0.8));
    }

    #[test]
    fn an_undeclared_zero_advance_is_a_whole_em_only_when_the_glyphs_stay_upright() {
        let metrics = bare(CssPx(18.0));
        let size = CssPx(20.0);
        assert_eq!(metrics.zero_advance_or_fallback(size, true), size);
        assert_eq!(metrics.zero_advance_or_fallback(size, false), CssPx(10.0));
    }
}
