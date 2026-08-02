//! Breaking a paragraph whose lines do not all have the same width.

use zgui_text::LineBands;

use crate::shape::brush::SlotBrush;

/// Breaks every line, giving each the width and start offset its own band allows.
///
/// The engine breaks a whole paragraph into one width in a single call, which is the right shape
/// for text in a rectangle and the wrong one as soon as anything is floated beside it: the lines
/// level with the float are narrower and start further in, and the lines below it are not. So this
/// drives the breaker a line at a time instead, setting the next line's geometry before asking for
/// it.
///
/// The paragraph-level width stays the widest any line may be, because the engine requires each
/// line's width not to exceed it.
pub(crate) fn break_banded(
    layout: &mut parley::Layout<SlotBrush>,
    max_advance: Option<f32>,
    bands: LineBands<'_>,
) {
    let widest = bands
        .as_slice()
        .iter()
        .map(|band| band.max_advance.0)
        .fold(max_advance.unwrap_or(0.0), f32::max);
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(widest);
    let mut index = 0;
    loop {
        if let Some(band) = bands.at(index) {
            let state = breaker.state_mut();
            state.set_line_x(band.offset.0);
            state.set_line_max_advance(band.max_advance.0.min(widest));
        }
        if breaker.break_next().is_none() {
            break;
        }
        index += 1;
    }
    breaker.finish();
}
