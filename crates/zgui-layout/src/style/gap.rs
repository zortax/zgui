//! `row-gap` and `column-gap`, which flex and grid read identically.

use taffy::LengthPercentage;
use taffy::prelude::TaffyZero;
use zgui_css::values::size::GapValue;

use crate::style::convert::length;

/// One gap, with the `normal` keyword resolved.
///
/// `normal` is zero for both flex and grid; it means something else only for multi-column layout,
/// which does not read this.
pub(crate) fn gap_value(
    value: &GapValue,
    scale: f32,
    calc: &mut impl crate::style::calc::InternCalc,
) -> LengthPercentage {
    match value {
        GapValue::Normal => LengthPercentage::ZERO,
        GapValue::LengthPercentage(inner) => length::length_percentage(&inner.0, scale, calc),
    }
}
