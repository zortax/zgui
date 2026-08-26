//! The CSS line boxes, computed on top of what a shaper broke.
//!
//! A shaper sizes a line from the first text run on it plus a maximum over what else is there, and
//! it splits one leading evenly above and below. CSS sizes a line from the strut and from every
//! inline-level box on it, each contributing its own half-leading and each *after* its
//! `vertical-align` shift. The two agree only when nothing is shifted and every run shares one line
//! height, so the block-axis geometry is computed here rather than read.
//!
//! # Why this walks runs and not glyphs
//!
//! Everything above is a question about the runs on a line and the boxes on it — never about where
//! an individual glyph landed. Walking the positioned items instead would cut every run into
//! style-uniform pieces and touch every glyph on the page, which measures several times what the
//! line break itself costs. The item walk happens only where it is the only answer: finding where
//! an atomic inline was placed, which nothing else reports, and which a context holding no atomic
//! never asks for.

use core::ops::Range;

use zgui_text::{BrokenParagraph, InlineBoxGeometry, InlineBoxPlacement, StrutMetrics, StyledRun};

use crate::inline::strut::Extents;

/// One line of an inline formatting context, in the units layout works in.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBox {
    /// The byte range of the generated string on this line.
    pub text: Range<usize>,
    /// The top edge, measured down from the top of the context's content box.
    pub top: f32,
    /// How far the line reaches either side of its baseline.
    pub extents: Extents,
    /// The advance the line's content occupies.
    pub width: f32,
    /// How far in from the context's start edge the content begins.
    pub offset: f32,
    /// Where the line is cut off, when it reaches past its box and the box marks the cut.
    ///
    /// Absent for every line that fits, which is nearly all of them, and absent for a box whose
    /// `text-overflow` is `clip` — the overflow is already not painted and nothing is written where
    /// it was cut. Painting reads this; layout never does, because an ellipsis changes no geometry.
    pub ellipsis: Option<crate::inline::ellipsis::LineEllipsis>,
}

impl LineBox {
    /// The baseline, measured down from the top of the context's content box.
    pub fn baseline(&self) -> f32 {
        self.top + self.extents.above
    }

    /// The line box's height.
    pub fn height(&self) -> f32 {
        self.extents.height()
    }
}

/// Computes every line box of a broken paragraph, and stacks them.
///
/// `runs` and `run_extents` are parallel; `boxes` is the geometry every atomic inline was broken
/// with and `placements` says which line each of them landed on.
pub fn compute(
    broken: &BrokenParagraph,
    runs: &[StyledRun],
    run_extents: &[Extents],
    strut: &StrutMetrics,
    boxes: &[InlineBoxGeometry],
    placements: &[InlineBoxPlacement],
) -> Vec<LineBox> {
    let mut out = Vec::with_capacity(broken.geometry.lines.len());
    let mut top = 0.0;
    for (index, line) in broken.geometry.lines.iter().enumerate() {
        let mut extents = Extents::of(strut);
        for (run, run_extent) in runs.iter().zip(run_extents) {
            if overlaps(&run.text, &line.text) {
                extents = extents.union(*run_extent);
            }
        }
        for placement in placements.iter().filter(|placed| placed.line == index) {
            let Some(geometry) = boxes.iter().find(|box_| box_.id == placement.id) else {
                continue;
            };
            extents = extents.union(Extents {
                above: geometry.shaper_height().0,
                below: geometry.below_baseline().0,
            });
        }
        out.push(LineBox {
            text: line.text.clone(),
            top,
            extents,
            width: line.width.0,
            offset: line.offset.0,
            ellipsis: None,
        });
        top += extents.height();
    }
    out
}

/// Whether a run has any of its bytes on a line.
///
/// An empty run — one whose every character collapsed away — is on the line it sits at the start
/// of, because it still contributes its own font to that line's height.
fn overlaps(run: &Range<usize>, line: &Range<usize>) -> bool {
    if run.is_empty() {
        return line.contains(&run.start) || line.start == run.start;
    }
    run.start < line.end && line.start < run.end
}

/// The height every line together occupies.
pub fn height(lines: &[LineBox]) -> f32 {
    lines.iter().map(LineBox::height).sum()
}

/// The width the lines together occupy, measured from the context's start edge.
pub fn width(lines: &[LineBox]) -> f32 {
    lines
        .iter()
        .map(|line| line.offset + line.width)
        .fold(0.0_f32, f32::max)
}

/// The line one geometry was placed on, and how far below the top of the context its own top edge
/// sits.
///
/// The shaper's `y` is the distance from the line's baseline to the box's declared top edge, and
/// that declared edge *is* the real one once the alignment shift has been folded into the height.
/// So the box's top is this line box's baseline less the same distance — the block-axis position
/// comes from the line box computed here and never from the shaper's own stacking.
pub fn placement(
    lines: &[LineBox],
    placement: &InlineBoxPlacement,
    geometry: &InlineBoxGeometry,
) -> Option<(f32, f32)> {
    let line = lines.get(placement.line)?;
    Some((
        placement.origin.x.0,
        line.baseline() - geometry.shaper_height().0,
    ))
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Size};
    use zgui_text::{BrokenParagraph, LineGeometry, StrutMetrics, TextGeometry};

    use crate::inline::strut::Extents;

    use super::{compute, height};

    /// Two lines of a paragraph whose shaper reported a 10 px line height for both.
    fn broken() -> BrokenParagraph {
        BrokenParagraph {
            geometry: std::sync::Arc::new(TextGeometry {
                lines: vec![
                    LineGeometry {
                        text: 0..5,
                        top: CssPx(0.0),
                        baseline: CssPx(8.0),
                        height: CssPx(10.0),
                        width: CssPx(40.0),
                        offset: CssPx(0.0),
                    },
                    LineGeometry {
                        text: 5..9,
                        top: CssPx(10.0),
                        baseline: CssPx(18.0),
                        height: CssPx(10.0),
                        width: CssPx(30.0),
                        offset: CssPx(0.0),
                    },
                ],
                size: Size::<CssPx, Css>::new(CssPx(40.0), CssPx(20.0)),
                is_rtl: false,
            }),
            boxes: Default::default(),
        }
    }

    /// A strut 12 above and 4 below.
    fn strut() -> StrutMetrics {
        StrutMetrics {
            font_ascent: CssPx(12.0),
            font_descent: CssPx(4.0),
            line_height: CssPx(16.0),
            x_height: CssPx(8.0),
            font_size: CssPx(16.0),
        }
    }

    #[test]
    fn the_lines_are_stacked_from_our_own_extents_and_not_from_the_shapers() {
        let lines = compute(&broken(), &[], &[], &strut(), &[], &[]);
        assert_eq!(lines.len(), 2);
        // The shaper said ten pixels a line; CSS says sixteen, because the strut's descent is part
        // of the line box and the shaper's number left it out.
        assert_eq!(lines[0].height(), 16.0);
        assert_eq!(lines[0].top, 0.0);
        assert_eq!(lines[0].baseline(), 12.0);
        assert_eq!(lines[1].top, 16.0);
        assert_eq!(lines[1].baseline(), 28.0);
        assert_eq!(height(&lines), 32.0);
    }

    #[test]
    fn a_taller_run_raises_only_the_line_it_is_on() {
        use std::sync::Arc;
        use zgui_text::StyledRun;
        use zgui_text_style::TextStyle;

        let runs = vec![StyledRun {
            text: 5..9,
            style: Arc::new(TextStyle::initial()),
            brush: zgui_scene::PaintSlot(0),
        }];
        let extents = vec![Extents {
            above: 30.0,
            below: 6.0,
        }];
        let lines = compute(&broken(), &runs, &extents, &strut(), &[], &[]);
        assert_eq!(
            lines[0].height(),
            16.0,
            "the first line has no such run on it"
        );
        assert_eq!(lines[1].height(), 36.0);
        assert_eq!(lines[1].top, 16.0);
        assert_eq!(lines[1].baseline(), 46.0);
    }
}
