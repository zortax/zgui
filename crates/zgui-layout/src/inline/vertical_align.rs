//! `vertical-align`, expressed through the one lever a shaper offers.
//!
//! A shaper places an inline box with its bottom edge on the baseline: the top edge lands at
//! `baseline - height`. The only way to move the box is therefore to lie about its height, so the
//! height handed over is the distance from the baseline to the box's *top* edge after the shift,
//! and the real geometry is carried beside it. Everything that needs the real box — the line box,
//! painting, hit testing — reads that instead.
//!
//! # The two values this cannot reach that way
//!
//! `top` and `bottom` align with the line box's own edges, and the line box is not known until
//! every other box on it has been placed. Their shift therefore cannot be baked into a height
//! before breaking, which is what the rest of the scheme does; they are resolved afterwards and
//! cost a second breaking pass over the lines that carry one.

use zgui_css::ComputedStyle;
use zgui_css::values::text::{AlignmentBaseline, BaselineShift, BaselineShiftKeyword};
use zgui_geom::CssPx;
use zgui_text::StrutMetrics;

/// The fraction of the font size a superscript is raised by.
///
/// No face metric for this reaches us: a shaper reports ascent, descent, leading, x-height and
/// cap-height, and neither the superscript nor the subscript offset the face's own tables may
/// carry. The constant is what the major engines use when the table is absent.
pub const SUPER_FRACTION: f32 = 0.34;

/// The fraction of the font size a subscript is lowered by, for the same reason.
pub const SUB_FRACTION: f32 = 0.20;

/// How one atomic inline is aligned against the line it sits on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alignment {
    /// The box's own baseline sits on the line's baseline.
    Baseline,
    /// Raised by the parent's superscript offset.
    Super,
    /// Lowered by the parent's subscript offset.
    Sub,
    /// The box's top edge meets the top of the parent's content area.
    TextTop,
    /// Its bottom edge meets the bottom of the parent's content area.
    TextBottom,
    /// Its midpoint meets the baseline raised by half the parent's x-height.
    Middle,
    /// Raised by an explicit distance.
    Length(f32),
    /// Its top edge meets the top of the line box.
    Top,
    /// Its bottom edge meets the bottom of the line box.
    Bottom,
}

impl Alignment {
    /// Whether resolving this needs the finished line box, and so a second breaking pass.
    pub fn needs_line_box(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// The alignment one box's style asks for.
///
/// The three longhands `vertical-align` expands to are read together, because the legacy keywords
/// land across all of them: `middle`, `text-top` and `text-bottom` choose a baseline to align to,
/// `sub`, `super`, `top`, `bottom` and a length shift the box away from it, and `baseline` is every
/// one of them at its initial value. A style that sets both — which `vertical-align: middle 4px`
/// does — is one alignment plus one shift, and both are honoured.
///
/// `line_height` is the aligned box's own resolved line height, which a percentage shift is
/// measured against, in the units layout works in.
pub fn of(style: &ComputedStyle, line_height: f32, scale: f32) -> Alignment {
    let group = style.get_box();
    let base = match group.alignment_baseline {
        AlignmentBaseline::TextTop => Some(Alignment::TextTop),
        AlignmentBaseline::TextBottom => Some(Alignment::TextBottom),
        AlignmentBaseline::Middle => Some(Alignment::Middle),
        _ => None,
    };
    let shift = match &group.baseline_shift {
        BaselineShift::Keyword(BaselineShiftKeyword::Sub) => Some(Alignment::Sub),
        BaselineShift::Keyword(BaselineShiftKeyword::Super) => Some(Alignment::Super),
        BaselineShift::Keyword(BaselineShiftKeyword::Top) => Some(Alignment::Top),
        BaselineShift::Keyword(BaselineShiftKeyword::Bottom) => Some(Alignment::Bottom),
        BaselineShift::Keyword(BaselineShiftKeyword::Center) => Some(Alignment::Middle),
        BaselineShift::Length(length) => {
            let raised = length
                .to_length()
                .map(|length| length.px() * scale)
                .or_else(|| {
                    length
                        .to_percentage()
                        .map(|fraction| fraction.0 * line_height)
                })
                .unwrap_or(0.0);
            (raised != 0.0).then_some(Alignment::Length(raised))
        }
    };
    // A keyword shift outranks the baseline it would otherwise be measured from, because `top` and
    // `bottom` are not relative to any baseline at all and `sub`/`super` are the legacy spelling of
    // a whole alignment rather than an adjustment to one.
    match (base, shift) {
        (_, Some(shift @ (Alignment::Top | Alignment::Bottom))) => shift,
        (None, Some(shift)) => shift,
        (Some(base), _) => base,
        (None, None) => Alignment::Baseline,
    }
}

/// The shift one alignment resolves to, positive upwards.
///
/// `ascent` is the distance from the box's own baseline to its top margin edge and `height` is its
/// whole margin box, both in the units layout works in; `strut` is the establishing block's, which
/// is what "the parent's content area" and "the parent's x-height" mean.
pub fn resolve(alignment: Alignment, ascent: f32, height: f32, strut: &StrutMetrics) -> f32 {
    let descent = height - ascent;
    match alignment {
        Alignment::Baseline => 0.0,
        Alignment::Super => SUPER_FRACTION * strut.font_size.0,
        Alignment::Sub => -SUB_FRACTION * strut.font_size.0,
        Alignment::TextTop => strut.font_ascent.0 - ascent,
        Alignment::TextBottom => descent - strut.font_descent.0,
        Alignment::Middle => height / 2.0 + strut.x_height.0 / 2.0 - ascent,
        Alignment::Length(raised) => raised,
        // Resolved against the line box once it exists; until then the box sits on the baseline,
        // which is where the first of the two breaking passes puts it.
        Alignment::Top | Alignment::Bottom => 0.0,
    }
}

/// The shift that puts a box's edge on the line box's own edge.
///
/// `above` and `below` are the line box's extents either side of its baseline. A `top`-aligned box
/// has its top edge on the line's top edge, so the distance from the baseline to its top edge is
/// the whole of `above`; a `bottom`-aligned one reaches `below` beneath the baseline.
pub fn resolve_against_line(
    alignment: Alignment,
    ascent: f32,
    height: f32,
    above: f32,
    below: f32,
) -> f32 {
    match alignment {
        Alignment::Top => above - ascent,
        Alignment::Bottom => height - below - ascent,
        _ => 0.0,
    }
}

/// Scales a strut measured in CSS pixels into the units layout works in.
pub fn scale_strut(strut: StrutMetrics, scale: f32) -> StrutMetrics {
    StrutMetrics {
        font_ascent: CssPx(strut.font_ascent.0 * scale),
        font_descent: CssPx(strut.font_descent.0 * scale),
        line_height: CssPx(strut.line_height.0 * scale),
        x_height: CssPx(strut.x_height.0 * scale),
        font_size: CssPx(strut.font_size.0 * scale),
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;
    use zgui_text::StrutMetrics;

    use super::{Alignment, resolve, resolve_against_line};

    /// A 16 px face with a 20 px line box: ascent 12, descent 4, two either side of them.
    fn strut() -> StrutMetrics {
        StrutMetrics {
            font_ascent: CssPx(12.0),
            font_descent: CssPx(4.0),
            line_height: CssPx(20.0),
            x_height: CssPx(8.0),
            font_size: CssPx(16.0),
        }
    }

    #[test]
    fn each_keyword_moves_the_box_the_distance_it_names() {
        // A 30 px box whose own baseline is 10 px above its bottom edge.
        let (ascent, height) = (20.0, 30.0);
        assert_eq!(resolve(Alignment::Baseline, ascent, height, &strut()), 0.0);
        assert_eq!(
            resolve(Alignment::Super, ascent, height, &strut()),
            16.0 * 0.34
        );
        assert_eq!(
            resolve(Alignment::Sub, ascent, height, &strut()),
            -16.0 * 0.20
        );
        // Top edge on the content area's top: the box reaches `ascent` above the baseline and the
        // content area reaches 12, so it drops by 8.
        assert_eq!(resolve(Alignment::TextTop, ascent, height, &strut()), -8.0);
        // Bottom edge on the content area's bottom: 10 below the baseline against the strut's 4.
        assert_eq!(
            resolve(Alignment::TextBottom, ascent, height, &strut()),
            6.0
        );
        // Midpoint on the baseline plus half the x-height: 15 + 4 - 20.
        assert_eq!(resolve(Alignment::Middle, ascent, height, &strut()), -1.0);
        assert_eq!(
            resolve(Alignment::Length(3.5), ascent, height, &strut()),
            3.5
        );
    }

    #[test]
    fn the_line_relative_keywords_are_zero_until_the_line_box_is_known() {
        for alignment in [Alignment::Top, Alignment::Bottom] {
            assert!(alignment.needs_line_box());
            assert_eq!(resolve(alignment, 20.0, 30.0, &strut()), 0.0);
        }
        for alignment in [Alignment::Baseline, Alignment::Middle, Alignment::Sub] {
            assert!(!alignment.needs_line_box());
            assert_eq!(resolve_against_line(alignment, 20.0, 30.0, 40.0, 10.0), 0.0);
        }
        // A 30 px box with a 20 px ascent on a line reaching 40 above and 10 below its baseline:
        // aligned to the top it starts 40 above the baseline, so it is raised by 20.
        assert_eq!(
            resolve_against_line(Alignment::Top, 20.0, 30.0, 40.0, 10.0),
            20.0
        );
        // Aligned to the bottom its lowest edge is 10 below the baseline, so its top edge is 20
        // above it, which is exactly its own ascent: no shift.
        assert_eq!(
            resolve_against_line(Alignment::Bottom, 20.0, 30.0, 40.0, 10.0),
            0.0
        );
    }
}
