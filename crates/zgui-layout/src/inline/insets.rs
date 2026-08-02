//! One box's own margins, borders and padding, resolved for the line it sits on.
//!
//! Percentages on all three resolve against the *inline* size of the containing block, on both
//! axes — that is what the specification says rather than an approximation of it — so one basis is
//! enough and it is the width the context is being laid out in.

use taffy::{Rect, ResolveOrZero};
use zgui_dom::side::BoxKey;

use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// One box's resolved margins, padding and borders.
pub(crate) fn frame_of<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    basis: Option<f32>,
) -> (Rect<f32>, Rect<f32>, Rect<f32>) {
    let style = tree.style_of(key);
    let calc = |value: *const (), basis: f32| {
        crate::style::calc::resolve_in(tree.calc_arena(), value, basis)
    };
    (
        taffy::CoreStyle::margin(&style).resolve_or_zero(basis, calc),
        taffy::CoreStyle::padding(&style).resolve_or_zero(basis, calc),
        taffy::CoreStyle::border(&style).resolve_or_zero(basis, calc),
    )
}

/// The width each edge of a nested inline box occupies: its margin, border and padding on that
/// side.
///
/// Returned as a pair rather than a rectangle because only the inline axis takes up room on a line:
/// an inline box's vertical padding is painted and does not move anything.
pub(crate) fn edges_of<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    basis: Option<f32>,
) -> (f32, f32) {
    let style = tree.style_of(key);
    let calc = |value: *const (), basis: f32| {
        crate::style::calc::resolve_in(tree.calc_arena(), value, basis)
    };
    let margin = taffy::CoreStyle::margin(&style).resolve_or_zero(basis, calc);
    let padding = taffy::CoreStyle::padding(&style).resolve_or_zero(basis, calc);
    let border = taffy::CoreStyle::border(&style).resolve_or_zero(basis, calc);
    (
        margin.left + padding.left + border.left,
        margin.right + padding.right + border.right,
    )
}
