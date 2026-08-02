//! The strip of a line's width that the floats beside it leave free.
//!
//! A floated box is placed by the block algorithm, against the block boxes around it. What it does
//! to the *lines* beside it is not the block algorithm's business and not a shaper's either: a
//! shaper breaks a paragraph into one width, and the whole point of a float is that the lines level
//! with it are narrower than the ones below it. So the loop is here — ask what is free at the
//! height each line has reached, break with those widths, and ask again if the answer moved the
//! lines.
//!
//! # Why it can converge and why it is bounded anyway
//!
//! A line's width decides its content, its content decides its height, and its height decides which
//! floats the *next* line meets. That is a fixpoint, and it normally settles in one extra pass
//! because the second pass' bands are the ones the first pass' line heights asked for. It is
//! bounded regardless: a pathological document can oscillate, and a layout pass that does not
//! terminate is worse than one that stops at a width a browser would also have accepted.

use taffy::BlockContext;
use zgui_geom::CssPx;
use zgui_text::LineBand;

use crate::inline::lines::LineBox;

/// How many times the bands may be recomputed before the answer is taken as it is.
pub const MAX_BAND_PASSES: usize = 3;

/// Whether any float can affect this context at all.
///
/// Answered before anything is computed, because a context with no float beside it must take the
/// unbanded path exactly: a band list that merely happens to be the full width would still make
/// every break a different question from the same break without one.
pub fn any_floats(block: Option<&BlockContext<'_>>) -> bool {
    block.is_some_and(BlockContext::has_floats)
}

/// The band each of `lines` breaks into, given the floats around the context.
///
/// `top` and `left` are where the content the lines fill begins, measured from the box's own
/// border edges, and `width` is the width that content would have with nothing floated beside it.
/// A line meets every float its own height overlaps, so each line is asked about twice — at its top
/// edge and just above its bottom edge — and takes whichever answer is narrower.
pub fn bands(
    block: &BlockContext<'_>,
    top: f32,
    left: f32,
    width: f32,
    lines: &[LineBox],
) -> Vec<LineBand> {
    let mut out = Vec::with_capacity(lines.len().max(1));
    for line in lines {
        let head = slot(block, top + line.top);
        let foot = slot(block, top + line.top + (line.height() - 1.0).max(0.0));
        let offset = head.0.max(foot.0);
        let available = head.1.min(foot.1).min(width);
        out.push(LineBand {
            offset: CssPx((offset - left).max(0.0)),
            max_advance: CssPx(available.max(0.0)),
        });
    }
    if out.is_empty() {
        out.push(LineBand::full(CssPx(width)));
    }
    out
}

/// The offset and width free at one height, in the box's own coordinates.
fn slot(block: &BlockContext<'_>, y: f32) -> (f32, f32) {
    let slot = block.find_content_slot(y, taffy::Clear::None, None);
    (slot.x, slot.width)
}

/// Whether two band lists ask for different breaks.
///
/// Compared exactly rather than with a tolerance: a band that moved by a hundredth of a pixel is a
/// different width to break into, and accepting it would leave the lines disagreeing with the
/// floats they were computed against.
pub fn differ(before: &[LineBand], after: &[LineBand]) -> bool {
    before.len() != after.len() || before.iter().zip(after).any(|(one, two)| one != two)
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;
    use zgui_text::LineBand;

    use super::differ;

    #[test]
    fn a_band_list_differs_when_any_band_or_the_count_does() {
        let wide = LineBand::full(CssPx(200.0));
        let narrow = LineBand {
            offset: CssPx(60.0),
            max_advance: CssPx(140.0),
        };
        assert!(!differ(&[wide, narrow], &[wide, narrow]));
        assert!(differ(&[wide, narrow], &[wide]));
        assert!(differ(&[wide, narrow], &[wide, wide]));
    }
}
