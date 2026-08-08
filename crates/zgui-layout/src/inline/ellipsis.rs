//! `text-overflow`: what a line that does not fit its box is cut off with.
//!
//! # Where the decision belongs
//!
//! An ellipsis changes no layout. The specification is explicit about it: the string is rendered
//! *over* the content, the lines keep the widths they were broken at, and a box's height stays what
//! it would have been. So the whole of this is a decision about which of a finished line's clusters
//! are painted — one breaking pass, and neither text key involved.
//!
//! It is nonetheless decided here, while the line is laid out, rather than where the line is
//! painted. Two reasons, and the first is the sharper one:
//!
//! * a shaped paragraph's *clusters* are the only thing that says where a character boundary is on
//!   the screen, and they are asked of the engine's laid-out form — which a probe pass may not
//!   move. The kept pass is the only pass entitled to ask;
//! * the painter has a fragment and a line number and no text at all. Glyph runs carry glyph
//!   identifiers and positions, and no byte offsets, so nothing downstream could work out which
//!   glyphs to leave out.
//!
//! What is recorded is therefore a cutoff — a coordinate on a cluster boundary — per line, plus one
//! shaped ellipsis for the whole context. The painter tightens the line's clip to the cutoff and
//! draws the ellipsis at it.
//!
//! # Why the cut is a coordinate
//!
//! The specification says characters that would only partly overflow are hidden entirely, and the
//! unit that has a boundary is the cluster: a ligature is one cluster of several characters and a
//! caret may only be placed either side of it. A cutoff on a cluster boundary expresses that
//! exactly, in the one vocabulary that survives to the painter. Filtering *glyphs* falls short of
//! it: a glyph's ink may reach past its own advance, so dropping glyphs by position cuts the ink of
//! a visible cluster or keeps the ink of a hidden one.

use zgui_css::ComputedStyle;
use zgui_css::parity::Support;
use zgui_css::register_properties;
use zgui_css::values::text::{TextOverflow, TextOverflowSide};
use zgui_dom::side::BoxKey;
use zgui_text::{ParagraphKey, StyledRun};

use crate::fragment::ParagraphId;
use crate::inline::lines::LineBox;
use crate::measure::MeasureContent;

register_properties! {
    text_overflow => Support::Implemented("zgui-layout::inline::ellipsis"),
}

/// One shaped mark, drawn where a line was cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipsisMark {
    /// The paragraph the mark's glyphs are a paragraph of.
    pub paragraph: ParagraphId,
    /// The key those glyphs are held under.
    pub key: ParagraphKey,
    /// How wide the mark is, in the units the lines are measured in.
    pub width: f32,
}

/// The shaped marks a whole inline formatting context cuts its lines off with.
///
/// One per side rather than one per line, because every line of a block is cut off with the same
/// string in the same style: the property is the *block container's*, so there is one answer for all
/// of them and shaping it once is the whole of the cost. Two sides rather than one, because the
/// two-value form of the property names them separately and may name different strings.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EllipsisSource {
    /// What a line cut off at its start is marked with.
    pub start: Option<EllipsisMark>,
    /// What a line cut off at its end is marked with.
    pub end: Option<EllipsisMark>,
}

impl EllipsisSource {
    /// The mark one line uses, by which end it was cut at.
    pub fn mark(&self, at_start: bool) -> Option<EllipsisMark> {
        if at_start { self.start } else { self.end }
    }

    /// Whether neither side marks anything, in which case there is nothing to record.
    pub fn is_empty(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }

    /// Every paragraph the marks name, which is what has to be retained and released.
    pub fn paragraphs(&self) -> impl Iterator<Item = ParagraphId> + '_ {
        [self.start, self.end]
            .into_iter()
            .flatten()
            .map(|mark| mark.paragraph)
    }
}

/// Where one line is cut off, and which end it is cut off at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEllipsis {
    /// The cluster boundary the line is cut at, measured from the content box's start edge.
    ///
    /// Everything on the far side of it is not painted, and the ellipsis is drawn at it. For a line
    /// cut off at its end that is the right-hand edge of what survives; for one cut off at its
    /// start it is the left-hand edge.
    pub cutoff: f32,
    /// Whether the cut is at the line's start rather than its end.
    pub at_start: bool,
}

/// Which end or ends of a line a block cuts off, and with what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sides {
    /// What the inline-start end is cut off with.
    pub start: Side,
    /// What the inline-end end is cut off with.
    pub end: Side,
}

/// What one end of a line is cut off with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Side {
    /// Nothing is drawn; the overflow is simply not painted.
    Clip,
    /// A string is drawn where the content was cut.
    String(String),
}

impl Side {
    /// The string this side draws, if it draws one.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Clip => None,
            Self::String(text) => Some(text),
        }
    }
}

/// The two ends of a line, resolved against the paragraph's own direction.
///
/// The one-value form of the property names the *end* side and leaves the start clipped, and the
/// two-value form names the left and the right. Which physical end each of those is depends on the
/// paragraph's base direction, which is why the resolution happens here: the direction is a fact
/// about the broken paragraph, and the lowering has yet to see one.
pub fn sides_of(style: &ComputedStyle, is_rtl: bool) -> Sides {
    let value: &TextOverflow = &style.get_text().text_overflow;
    let (start, end) = if value.sides_are_logical {
        (&value.first, &value.second)
    } else if is_rtl {
        (&value.second, &value.first)
    } else {
        (&value.first, &value.second)
    };
    Sides {
        start: side(start),
        end: side(end),
    }
}

/// One side of the property, lowered.
fn side(value: &TextOverflowSide) -> Side {
    match value {
        TextOverflowSide::Clip => Side::Clip,
        TextOverflowSide::Ellipsis => Side::String("\u{2026}".to_owned()),
        TextOverflowSide::String(text) => Side::String(text.to_string()),
    }
}

/// Whether a box cuts its lines off at all, which is what makes `text-overflow` apply.
///
/// The property applies to a block container whose inline-axis overflow is not `visible` — an
/// ellipsis is drawn where content was *cut*, and content that is allowed to spill out of its box
/// was never cut. `overflow: visible` on the inline axis is therefore the whole of the test, and it
/// is asked per axis rather than per box because a box may scroll one way and spill the other.
pub fn clips_inline_axis(style: &ComputedStyle) -> bool {
    // Horizontal writing modes are the only ones this engine lays out, so the inline axis is the
    // horizontal one. A vertical mode would read `overflow_y` here, and the box tree would have to
    // have laid the lines out down the page first.
    style.get_box().overflow_x != zgui_css::values::size::OverflowValue::Visible
}

/// Whether any line reaches past either edge of the width it was broken at.
///
/// The question that keeps the property cheap: a label is given `text-overflow: ellipsis` against
/// the day its text is too long, and on every other day this is one comparison per line and nothing
/// is shaped, measured or recorded.
pub(crate) fn any_overflows(lines: &[LineBox], available: f32, tolerance: f32) -> Overflowing {
    let mut overflowing = Overflowing::default();
    for line in lines {
        overflowing.start |= overflows_start(line, tolerance);
        overflowing.end |= overflows_end(line, available, tolerance);
    }
    overflowing
}

/// Which ends of a context's lines reach past the box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Overflowing {
    /// Some line reaches past the inline-start edge.
    pub(crate) start: bool,
    /// Some line reaches past the inline-end edge.
    pub(crate) end: bool,
}

impl Overflowing {
    /// Whether neither end overflows.
    pub(crate) fn is_none(self) -> bool {
        !self.start && !self.end
    }
}

/// Whether one line reaches past the inline-end edge of the width it was broken at.
fn overflows_end(line: &LineBox, available: f32, tolerance: f32) -> bool {
    line.offset + line.width > available + tolerance
}

/// Whether one line begins before the inline-start edge.
///
/// A line does that when something put it there — a negative `text-indent`, or an end-aligned line
/// wider than its box — and it is cut off at that end by the same box overflow, so it is marked at
/// that end too.
fn overflows_start(line: &LineBox, tolerance: f32) -> bool {
    line.offset < -tolerance
}

/// Marks every line that does not fit, given the marks the context is cutting with.
///
/// One pass over the clusters of each overflowing line, and nothing at all over the rest.
pub(crate) fn annotate<C: MeasureContent>(
    content: &C,
    key: ParagraphKey,
    lines: &mut [LineBox],
    source: &EllipsisSource,
    available: f32,
    tolerance: f32,
) {
    for (index, line) in lines.iter_mut().enumerate() {
        let at_start = overflows_start(line, tolerance);
        if !(at_start || overflows_end(line, available, tolerance)) {
            continue;
        }
        // `clip`, which is the initial value, has no mark on this side: the box's own overflow
        // already stops the content being drawn, and nothing is written where it was cut.
        let Some(mark) = source.mark(at_start) else {
            continue;
        };
        let cutoff = cut(
            content,
            key,
            u16::try_from(index).unwrap_or(u16::MAX),
            line,
            available,
            mark.width,
            at_start,
        );
        line.ellipsis = Some(LineEllipsis { cutoff, at_start });
    }
}

/// Where one line is cut, on the cluster boundary nearest the edge that still leaves room.
///
/// Walked in the clusters' own order and compared by position, so a line whose runs change
/// direction is cut where the *screen* says rather than where the string does — which is what a
/// reader sees, and the only answer a mixed-direction line has.
fn cut<C: MeasureContent>(
    content: &C,
    key: ParagraphKey,
    line: u16,
    box_: &LineBox,
    available: f32,
    mark: f32,
    at_start: bool,
) -> f32 {
    // The budgets are in the line's own coordinates, which begin at its leading edge: the line may
    // have been indented or aligned, so the edge it overflows belongs to the box.
    let end_budget = available - box_.offset - mark;
    let start_budget = mark - box_.offset;

    let mut cutoff = if at_start { f32::INFINITY } else { 0.0 };
    content.visit_clusters(key, line, &mut |run| {
        for cluster in run.clusters {
            let leading = run.start.0 + cluster.offset.0;
            let trailing = leading + cluster.advance.0;
            if at_start {
                // The first boundary at or past where the mark ends. A cluster straddling the edge
                // goes with the hidden half, which is the specification's "partly overflowing
                // characters are hidden entirely".
                if leading >= start_budget {
                    cutoff = cutoff.min(leading);
                }
            } else if trailing <= end_budget {
                cutoff = cutoff.max(trailing);
            }
        }
    });
    if at_start && !cutoff.is_finite() {
        // Nothing survived: the whole line is behind the mark, and the cut is at its far edge.
        cutoff = box_.width;
    }
    box_.offset + cutoff.max(0.0)
}

/// A fingerprint of what one line draws that its own rectangle does not describe.
///
/// Zero for a line that is not cut off, which keeps the ordinary case comparing exactly as it did
/// before. For one that is, it names the boundary and the end — the two things a repaint has to
/// notice, and the two that can change while the line box stays where it was.
pub fn line_hash(line: &LineBox) -> u64 {
    match line.ellipsis {
        None => 0,
        Some(LineEllipsis { cutoff, at_start }) => {
            // Not zero for a cut at the very origin, so "no ellipsis" and "an ellipsis here" are
            // different answers.
            u64::from(cutoff.to_bits()) << 1 | u64::from(at_start) | 1 << 33
        }
    }
}

/// Builds the one-run content the ellipsis string is shaped as.
///
/// The owning block's own text style, because that is what the property's own box is drawn in — a
/// run inside the block may be bold or another size, and the mark that says content was cut belongs
/// to the box that cut it.
pub(crate) fn runs(
    text: &str,
    style: std::sync::Arc<zgui_text_style::TextStyle>,
    brush: zgui_text::Brush,
) -> Vec<StyledRun> {
    vec![StyledRun {
        text: 0..text.len(),
        style,
        brush,
    }]
}

/// The box whose `text-overflow` governs one inline formatting context.
///
/// An anonymous block wrapping a run of inline content is not an element and has the initial value
/// of every property, so the governing box is the nearest real one — which is the element the
/// author wrote the declaration on.
pub(crate) fn governing(store: &crate::tree::store::LayoutStore, key: BoxKey) -> Option<BoxKey> {
    let node = store.get(key)?;
    if node.kind.is_anonymous() {
        return node.parent;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::{Side, Sides};

    /// A side that draws nothing says so, and one that draws a string hands it over.
    #[test]
    fn only_a_string_side_marks_where_the_content_was_cut() {
        let sides = Sides {
            start: Side::Clip,
            end: Side::String("\u{2026}".to_owned()),
        };
        assert_eq!(sides.start.text(), None);
        assert_eq!(sides.end.text(), Some("\u{2026}"));
    }
}
