//! Reading the line boxes out of a broken layout.

use smallvec::SmallVec;
use zgui_geom::{CssPx, Point, Size};
use zgui_text::{BrokenParagraph, InlineBoxPlacement, LineGeometry, TextGeometry};

use crate::shape::brush::SlotBrush;

/// The lines and the box they fill, plus where the atomic inlines landed.
///
/// # Where a line actually starts
///
/// A line's start edge is two numbers, not one. The engine's alignment offset is where alignment
/// and indent put the line inside the width it was broken into; its minimum inline coordinate is
/// where that width itself begins, which is anywhere but zero as soon as the line was banded around
/// a float. Everything inside the engine that positions anything — glyph runs, inline boxes, cursor
/// geometry — adds the two, and reporting only the first leaves a line's box at the paragraph's
/// edge while its glyphs sit correctly beside the float.
///
/// # Why the two walks are not one
///
/// The engine offers two iterators over a line. One yields its runs; the other yields *positioned
/// items*, which means it cuts every run into style-uniform glyph runs and therefore walks every
/// glyph on the line. Measured over a long paragraph, computing line boxes over the second costs
/// four times what the line break itself costs, and the first costs nothing measurable.
///
/// The item walk is only *needed* when the context actually holds an atomic inline, because that
/// is the only thing whose position cannot be read any other way. So it is taken when there is one
/// and not otherwise, which for body text and for every label in a component library means never.
///
/// `prefix` is how many bytes of directional control the engine's string carries in front of the
/// caller's, and it is subtracted from every range reported here: a line range is an index into the
/// string the caller generated, which never held those bytes.
pub(crate) fn read(
    layout: &parley::Layout<SlotBrush>,
    has_boxes: bool,
    prefix: usize,
) -> BrokenParagraph {
    let mut lines = Vec::with_capacity(layout.len());
    let mut boxes: SmallVec<[InlineBoxPlacement; 2]> = SmallVec::new();
    for (index, line) in layout.lines().enumerate() {
        let metrics = line.metrics();
        lines.push(LineGeometry {
            text: without_prefix(line.text_range(), prefix),
            top: CssPx(metrics.block_min_coord),
            baseline: CssPx(metrics.baseline),
            height: CssPx(metrics.line_height),
            width: CssPx(metrics.advance),
            offset: CssPx(metrics.offset + metrics.inline_min_coord),
        });
        if has_boxes {
            collect_boxes(&line, index, &mut boxes);
        }
    }
    BrokenParagraph {
        geometry: std::sync::Arc::new(TextGeometry {
            lines,
            size: Size::new(CssPx(layout.width()), CssPx(layout.height())),
            is_rtl: layout.is_rtl(),
        }),
        boxes,
    }
}

/// One engine range, moved back into the string the caller generated.
///
/// The first line's range starts inside the directional prefix, which is a character the caller's
/// string does not have: it starts at zero there rather than at a negative offset, because the
/// first byte of the caller's text is the first byte that line covers.
fn without_prefix(range: core::ops::Range<usize>, prefix: usize) -> core::ops::Range<usize> {
    range.start.saturating_sub(prefix)..range.end.saturating_sub(prefix)
}

/// Appends the atomic inlines that landed on one line.
///
/// The corner reported is the box's *real* top-left corner. That is not a conversion: the engine
/// places an inline box with its bottom edge on the baseline, so the top edge it computes is the
/// distance the box was declared to have above the baseline — which is exactly where the real box
/// starts once the alignment shift has been folded into that declared height.
fn collect_boxes(
    line: &parley::Line<'_, SlotBrush>,
    index: usize,
    out: &mut SmallVec<[InlineBoxPlacement; 2]>,
) {
    for item in line.items() {
        let parley::PositionedLayoutItem::InlineBox(placed) = item else {
            continue;
        };
        out.push(InlineBoxPlacement {
            id: placed.id,
            origin: Point::new(CssPx(placed.x), CssPx(placed.y)),
            line: index,
        });
    }
}
