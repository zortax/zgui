//! Reading the line boxes out of a broken layout.

use smallvec::SmallVec;
use zgui_geom::{CssPx, Point, Size};
use zgui_text::{BrokenParagraph, InlineBoxPlacement, LineGeometry, TextGeometry};

use crate::shape::brush::SlotBrush;

/// Where a line starts, with a negative leading kept.
///
/// The engine clamps a negative leading to zero for `block_min_coord`. CSS keeps it: a line height
/// tighter than the face puts the glyphs outside the line box, above and below alike. The engine
/// quantizes, so it rounds the ascent and the descent and gives the smaller half of the leading to
/// the space above, and this repeats that arithmetic.
pub(crate) fn top(metrics: &parley::LineMetrics) -> f32 {
    let ascent = metrics.ascent.round();
    let descent = metrics.descent.round();
    let above = ((metrics.line_height - (ascent + descent)) * 0.5).floor();
    metrics.baseline - ascent - above
}

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
        lines.push(line_box(&line, prefix));
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

/// One line's box, in the caller's string.
fn line_box(line: &parley::Line<'_, SlotBrush>, prefix: usize) -> LineGeometry {
    let metrics = line.metrics();
    LineGeometry {
        text: without_prefix(line.text_range(), prefix),
        top: CssPx(top(metrics)),
        baseline: CssPx(metrics.baseline),
        height: CssPx(metrics.line_height),
        width: CssPx(metrics.advance),
        offset: CssPx(metrics.offset + metrics.inline_min_coord),
    }
}

/// Writes how many items each line of `layout` holds into `out`.
pub(crate) fn items(layout: &parley::Layout<SlotBrush>, out: &mut SmallVec<[u32; 4]>) {
    out.clear();
    out.extend(
        layout
            .lines()
            .map(|line| u32::try_from(line.len()).unwrap_or(u32::MAX)),
    );
}

/// Whether [`read`] of the freshly broken `layout` would return `last`, bit for bit.
///
/// The check is exact only when the caller makes sure that:
///
/// - neither this pass nor the pass of `last` is banded;
/// - neither pass is justified, so every cluster keeps its shaped advance;
/// - the inline boxes have the widths and heights the pass of `last` used.
///
/// The engine places a box from the line's offset, its baseline and the advances of the items
/// before it on the line. The line boxes hold the offset and the baseline. Equal text ranges and
/// equal item counts per line (`items`, from the pass of `last`) put the same items, boxes
/// included, on each line.
///
/// `has_boxes` is whether this pass reads the boxes; it must agree with `last`.
pub(crate) fn unchanged(
    layout: &parley::Layout<SlotBrush>,
    last: &BrokenParagraph,
    items: &[u32],
    has_boxes: bool,
    prefix: usize,
) -> bool {
    let geometry = &last.geometry;
    if geometry.lines.len() != layout.len()
        || items.len() != layout.len()
        || has_boxes == last.boxes.is_empty()
        || geometry.is_rtl != layout.is_rtl()
        || !same_bits(geometry.size.width, CssPx(layout.width()))
        || !same_bits(geometry.size.height, CssPx(layout.height()))
    {
        return false;
    }
    layout
        .lines()
        .zip(&geometry.lines)
        .zip(items)
        .all(|((line, kept), &count)| {
            same_line(&line_box(&line, prefix), kept)
                && u32::try_from(line.len()).ok() == Some(count)
        })
}

/// Whether two line boxes are equal bit for bit.
fn same_line(a: &LineGeometry, b: &LineGeometry) -> bool {
    a.text == b.text
        && same_bits(a.top, b.top)
        && same_bits(a.baseline, b.baseline)
        && same_bits(a.height, b.height)
        && same_bits(a.width, b.width)
        && same_bits(a.offset, b.offset)
}

/// Whether two lengths are equal bit for bit, which tells `0.0` from `-0.0`.
fn same_bits(a: CssPx, b: CssPx) -> bool {
    a.0.to_bits() == b.0.to_bits()
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
