//! Lengths, percentages and the sizing keywords, as the layout algorithms want them.
//!
//! Two rules run through every conversion here. Percentages are fractions on both sides, so
//! nothing is ever multiplied or divided by a hundred. And every absolute length is scaled into
//! device pixels on the way through, because layout runs on the physical pixel grid rather than the
//! CSS one — which is what lets the snapping pass produce crisp edges at a fractional scale.

use taffy::prelude::TaffyAuto;
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};
use zgui_css::values::length::LengthPercentage as CssLengthPercentage;
use zgui_css::values::size::{InsetValue, MarginValue, MaxSizeValue, PaddingValue, SizeValue};

use crate::style::calc::InternCalc;

/// A length or percentage, with `calc()` handed to the interner.
pub(crate) fn length_percentage(
    value: &CssLengthPercentage,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentage {
    if let Some(length) = value.to_length() {
        LengthPercentage::length(length.px() * scale)
    } else if let Some(percentage) = value.to_percentage() {
        LengthPercentage::percent(percentage.0)
    } else {
        LengthPercentage::calc(calc.intern_calc(value))
    }
}

/// One padding side, which the grammar has already forbidden from being negative.
pub(crate) fn padding(
    value: &PaddingValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentage {
    length_percentage(&value.0, scale, calc)
}

/// A length or percentage that may also be `auto`.
pub(crate) fn optional_length_percentage(
    value: &CssLengthPercentage,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    if let Some(length) = value.to_length() {
        LengthPercentageAuto::length(length.px() * scale)
    } else if let Some(percentage) = value.to_percentage() {
        LengthPercentageAuto::percent(percentage.0)
    } else {
        LengthPercentageAuto::calc(calc.intern_calc(value))
    }
}

/// One margin side.
///
/// The anchor-positioning forms have no representation and become `auto`, which is what an
/// unresolvable margin means everywhere else in CSS.
pub(crate) fn margin(
    value: &MarginValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        MarginValue::LengthPercentage(inner) | MarginValue::AnchorContainingCalcFunction(inner) => {
            optional_length_percentage(inner, scale, calc)
        }
        MarginValue::Auto | MarginValue::AnchorSizeFunction(_) => LengthPercentageAuto::AUTO,
    }
}

/// One of `top`, `right`, `bottom` and `left`.
pub(crate) fn inset(
    value: &InsetValue,
    scale: f32,
    calc: &mut impl InternCalc,
) -> LengthPercentageAuto {
    match value {
        InsetValue::LengthPercentage(inner) | InsetValue::AnchorContainingCalcFunction(inner) => {
            optional_length_percentage(inner, scale, calc)
        }
        InsetValue::Auto | InsetValue::AnchorFunction(_) | InsetValue::AnchorSizeFunction(_) => {
            LengthPercentageAuto::AUTO
        }
    }
}

/// One border side's width, in device pixels, or zero if that side draws nothing.
///
/// A side whose style is `none` or `hidden` has no border at all, whatever width was written. The
/// computed width is the written one, so the suppression happens here — and it is not cosmetic: the
/// initial width is three pixels, so a box with no border at all would otherwise lose six pixels of
/// content box on each axis.
pub(crate) fn border_side(
    width: &zgui_css::values::border::BorderSideWidthValue,
    style: zgui_css::values::border::BorderStyleValue,
    scale: f32,
) -> LengthPercentage {
    use taffy::prelude::TaffyZero;
    match style {
        zgui_css::values::border::BorderStyleValue::None
        | zgui_css::values::border::BorderStyleValue::Hidden => LengthPercentage::ZERO,
        _ => LengthPercentage::length(width.0.to_f32_px() * scale),
    }
}

/// What a sizing keyword resolves to once the content has been measured.
///
/// The layout algorithms have no representation for `min-content`, `max-content` or `fit-content`,
/// so a size written with one of them is turned into a length before it reaches them — which is
/// only possible if the content's intrinsic sizes are already known. This is the shape of the
/// answer the intrinsic pre-pass supplies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntrinsicSizes {
    /// The narrowest the content can be, in device pixels.
    pub min: f32,
    /// The widest it would like to be, in device pixels.
    pub max: f32,
}

impl IntrinsicSizes {
    /// The same two sizes with `inset` taken off each, never below zero.
    ///
    /// A measurement is of the whole box, insets included, while a size that has to have padding
    /// and border added back to it is stated without them. This converts between the two.
    #[must_use]
    pub fn less(self, inset: f32) -> Self {
        Self {
            min: (self.min - inset).max(0.0),
            max: (self.max - inset).max(0.0),
        }
    }

    /// The `fit-content` size for an available space of `available`.
    ///
    /// `fit-content` is `min(max(min-content, available), max-content)`, so a container narrower
    /// than the content gets the content's minimum and a wider one gets the content's maximum.
    pub fn fit_content(self, available: Option<f32>) -> f32 {
        match available {
            Some(available) => available.clamp(self.min, self.min.max(self.max)),
            None => self.max,
        }
    }
}

/// `width`, `height`, `min-width` or `min-height`.
///
/// `stretch` and `-webkit-fill-available` become a full percentage of the containing block, which
/// is what they mean whenever the box has no margins — the case they are written for. The three
/// content keywords need `intrinsic`; without it they fall back to `auto`, which is the value they
/// would have had if the keyword had not been written.
pub(crate) fn size(
    value: &SizeValue,
    scale: f32,
    calc: &mut impl InternCalc,
    intrinsic: Option<IntrinsicSizes>,
) -> Dimension {
    match value {
        SizeValue::LengthPercentage(inner) | SizeValue::AnchorContainingCalcFunction(inner) => {
            dimension(&inner.0, scale, calc)
        }
        SizeValue::Auto | SizeValue::AnchorSizeFunction(_) => Dimension::AUTO,
        SizeValue::WebkitFillAvailable | SizeValue::Stretch => Dimension::percent(1.0),
        SizeValue::MinContent => keyword(intrinsic, |sizes| sizes.min),
        SizeValue::MaxContent => keyword(intrinsic, |sizes| sizes.max),
        SizeValue::FitContent | SizeValue::FitContentFunction(_) => {
            keyword(intrinsic, |sizes| sizes.max)
        }
    }
}

/// `max-width` or `max-height`, where `none` means "no maximum".
///
/// The layout algorithms spell "no maximum" `auto`, because an `auto` maximum resolves to nothing
/// and clamping by nothing is a no-op.
pub(crate) fn max_size(
    value: &MaxSizeValue,
    scale: f32,
    calc: &mut impl InternCalc,
    intrinsic: Option<IntrinsicSizes>,
) -> Dimension {
    match value {
        MaxSizeValue::LengthPercentage(inner)
        | MaxSizeValue::AnchorContainingCalcFunction(inner) => dimension(&inner.0, scale, calc),
        MaxSizeValue::None | MaxSizeValue::AnchorSizeFunction(_) => Dimension::AUTO,
        MaxSizeValue::WebkitFillAvailable | MaxSizeValue::Stretch => Dimension::percent(1.0),
        MaxSizeValue::MinContent => keyword(intrinsic, |sizes| sizes.min),
        MaxSizeValue::MaxContent => keyword(intrinsic, |sizes| sizes.max),
        MaxSizeValue::FitContent | MaxSizeValue::FitContentFunction(_) => {
            keyword(intrinsic, |sizes| sizes.max)
        }
    }
}

/// A length or percentage as a dimension.
fn dimension(value: &CssLengthPercentage, scale: f32, calc: &mut impl InternCalc) -> Dimension {
    if let Some(length) = value.to_length() {
        Dimension::length(length.px() * scale)
    } else if let Some(percentage) = value.to_percentage() {
        Dimension::percent(percentage.0)
    } else {
        Dimension::calc(calc.intern_calc(value))
    }
}

/// A content keyword's resolved length, or `auto` if the content has not been measured.
fn keyword(
    intrinsic: Option<IntrinsicSizes>,
    pick: impl FnOnce(IntrinsicSizes) -> f32,
) -> Dimension {
    intrinsic.map_or(Dimension::AUTO, |sizes| Dimension::length(pick(sizes)))
}

#[cfg(test)]
mod tests {
    use taffy::Dimension;
    use taffy::prelude::TaffyAuto;
    use zgui_css::values::length::{Length, LengthPercentage as CssLengthPercentage, percent};
    use zgui_css::values::size::SizeValue;

    use crate::style::calc::CalcTable;

    use super::{IntrinsicSizes, length_percentage, size};

    fn table(scale: f32) -> CalcTable {
        let mut table = CalcTable::default();
        table.set_scale(scale);
        table
    }

    #[test]
    fn an_absolute_length_reaches_the_engine_in_device_pixels() {
        let mut calc = table(2.0);
        let value = CssLengthPercentage::new_length(Length::new(12.0));
        let converted = length_percentage(&value, 2.0, &mut calc);
        assert_eq!(converted.into_raw().value(), 24.0);
        assert_eq!(calc.live(), 0, "no calc was interned");
    }

    #[test]
    fn a_percentage_crosses_as_a_fraction_on_both_sides() {
        let mut calc = table(1.0);
        let converted = length_percentage(&percent(0.25), 1.0, &mut calc);
        assert_eq!(converted, taffy::LengthPercentage::percent(0.25));
    }

    #[test]
    fn a_content_keyword_without_a_measurement_is_auto_rather_than_zero() {
        let mut calc = table(1.0);
        assert_eq!(
            size(&SizeValue::MinContent, 1.0, &mut calc, None),
            Dimension::AUTO
        );
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        assert_eq!(
            size(&SizeValue::MinContent, 1.0, &mut calc, Some(sizes)),
            Dimension::length(30.0)
        );
        assert_eq!(
            size(&SizeValue::MaxContent, 1.0, &mut calc, Some(sizes)),
            Dimension::length(90.0)
        );
    }

    #[test]
    fn stretch_is_the_whole_containing_block() {
        let mut calc = table(1.0);
        assert_eq!(
            size(&SizeValue::Stretch, 1.0, &mut calc, None),
            Dimension::percent(1.0)
        );
        assert_eq!(
            size(&SizeValue::WebkitFillAvailable, 1.0, &mut calc, None),
            Dimension::percent(1.0)
        );
    }

    #[test]
    fn taking_the_insets_off_a_measurement_never_goes_below_zero() {
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        assert_eq!(
            sizes.less(20.0),
            IntrinsicSizes {
                min: 10.0,
                max: 70.0
            }
        );
        assert_eq!(sizes.less(200.0), IntrinsicSizes { min: 0.0, max: 0.0 });
    }

    #[test]
    fn fit_content_sits_between_the_two_intrinsic_sizes() {
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        assert_eq!(sizes.fit_content(Some(10.0)), 30.0);
        assert_eq!(sizes.fit_content(Some(60.0)), 60.0);
        assert_eq!(sizes.fit_content(Some(200.0)), 90.0);
        assert_eq!(sizes.fit_content(None), 90.0);
    }
}
