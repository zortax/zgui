//! `row-gap` and `column-gap`, which flex and grid read identically.

use taffy::prelude::TaffyZero;
use taffy::{LengthPercentage, Size};
use zgui_css::values::size::GapValue;

use crate::style::StyleRef;
use crate::style::convert::length;

/// The gaps between a container's items, with the column gap on the inline axis.
///
/// `normal` is zero for both flex and grid; it means something else only for multi-column layout,
/// which does not read this.
pub fn gap_of(style: StyleRef<'_>) -> Size<LengthPercentage> {
    let (scale, calc) = (style.scale(), style.calc());
    let position = style.position_group();
    Size {
        width: gap_value(&position.column_gap, scale, calc),
        height: gap_value(&position.row_gap, scale, calc),
    }
}

/// One gap.
fn gap_value(
    value: &GapValue,
    scale: f32,
    calc: &core::cell::RefCell<crate::style::calc::CalcArena>,
) -> LengthPercentage {
    match value {
        GapValue::Normal => LengthPercentage::ZERO,
        GapValue::LengthPercentage(inner) => length::length_percentage(&inner.0, scale, calc),
    }
}
