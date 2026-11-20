//! The frozen form: the one break a paragraph has, without the engine's layout.
//!
//! A paragraph whose runs may not wrap, that holds no inline box and whose runs all go left to
//! right breaks only where its text forces a break. Its lines hold the same glyphs at every width.
//! Only the offset that alignment and indent give each line moves. So the engine's layout is read
//! once, after shaping, into this form, and dropped.
//!
//! # What the form holds
//!
//! - For each line: its trailing white space, its current alignment offset, and its runs.
//! - For each engine run on a line: its advance and its clusters. A cluster is a byte range, an
//!   offset from the start of the run and an advance.
//! - For each style-uniform glyph run: the face, the size, the synthesis, the brush, where it
//!   starts on the line, and its glyphs. A glyph is an id, the engine's own offset, the height it
//!   is drawn at and an advance.
//!
//! The line boxes come from the paragraph's last break, which the shaped layout holds.
//!
//! # Bit for bit
//!
//! Every position is computed with the operations of the engine's own reading, in the same order,
//! so the frozen and the live form give equal bits. A value that no request moves is stored as the
//! engine computed it. A glyph's position and a run's start move with the alignment offset, so they
//! are computed again from it at each visit, as the engine computes them.
//!
//! # When the breaker gives the same lines at every width
//!
//! The engine's breaker can break a line that may not wrap in one case only: at the first cluster
//! of the paragraph it reads the wrap mode of no cluster before it, which is `wrap`, so a space
//! there that does not fit ends a line. A paragraph whose first cluster is a space is therefore
//! not frozen. Every line but the last must end at a forced break; [`freeze`] checks it.
//!
//! # What it cannot answer
//!
//! A banded request, and a request with inline boxes. The shaper shapes such a paragraph again
//! into the live form first (`TextThawed`).

use core::ops::Range;
use std::sync::Arc;

use smallvec::SmallVec;
use zgui_geom::{CssPx, Size};
use zgui_text::{
    BreakRequest, BrokenParagraph, Brush, ClusterGeometry, ClusterRun, ShapedGlyph, ShapedRun,
    TextDirection, TextGeometry,
};
use zgui_text_style::TextAlign;

use crate::shape::brush::SlotBrush;
use crate::system::FontSystem;

/// Where the inline extent of a line that is not banded starts.
const INLINE_MIN: f32 = 0.0;

/// One paragraph's glyphs and clusters at its one break.
#[derive(Debug)]
pub(crate) struct Frozen {
    /// The lines, in order.
    lines: Box<[Line]>,
    /// The engine runs of every line, line after line.
    runs: Box<[Run]>,
    /// The style-uniform glyph runs of every line, line after line.
    pieces: Box<[Piece]>,
    /// The glyphs of every piece.
    glyphs: Box<[Glyph]>,
    /// The clusters of every run.
    clusters: Box<[Cluster]>,
    /// Device pixels per CSS pixel, kept to shape the paragraph again.
    scale: f32,
}

/// One line.
#[derive(Debug)]
struct Line {
    /// The advance of the white space at its end.
    trailing: f32,
    /// The engine's alignment offset at the current break.
    offset: f32,
    /// Its engine runs.
    runs: Range<u32>,
    /// Its glyph runs.
    pieces: Range<u32>,
}

/// One engine run of a line.
#[derive(Debug)]
struct Run {
    /// Its advance on the line.
    advance: f32,
    /// Its clusters, in logical order, without the ones the directional prefix holds.
    clusters: Range<u32>,
}

/// One style-uniform glyph run of a line.
#[derive(Debug)]
struct Piece {
    /// The file the face lives in.
    font: parley::FontData,
    /// The point in a variable face's design space.
    coords: Box<[i16]>,
    /// The size, in device pixels.
    size: f32,
    /// Whether the engine emboldens the glyphs.
    embolden: bool,
    /// How far the engine shears the glyphs, in degrees.
    skew: f32,
    /// The brush slot.
    brush: Brush,
    /// Where it starts on the line, before alignment.
    start: f32,
    /// Where its engine run's text starts, in the caller's string.
    text_start: u32,
    /// Its glyphs, in visual order.
    glyphs: Range<u32>,
}

/// One glyph.
#[derive(Debug)]
struct Glyph {
    /// The engine's offset of the glyph from its pen position.
    x: f32,
    /// The distance from the top of the line box down to the glyph's baseline.
    y: f32,
    /// The pen advance.
    advance: f32,
    /// The glyph's index in its face.
    id: u16,
}

/// One cluster.
#[derive(Debug)]
struct Cluster {
    /// The first byte, in the caller's string.
    start: u16,
    /// The byte after the last.
    end: u16,
    /// The distance from the start of the run.
    offset: f32,
    /// The advance.
    advance: f32,
}

/// The answer to one request.
pub(crate) struct Laid {
    /// The lines and the box they fill.
    pub(crate) broken: BrokenParagraph,
    /// Whether `broken` is the last break, kept.
    pub(crate) kept: bool,
    /// The engine's alignment offset of each line.
    offsets: SmallVec<[f32; 4]>,
}

/// Reads a freshly shaped layout, broken once without a width and not aligned, into the frozen
/// form.
///
/// `None` when the layout can break in more than one way, holds an inline box or a right-to-left
/// run, or is too long for 16-bit offsets. The caller checks that no run may wrap.
pub(crate) fn freeze(
    layout: &parley::Layout<SlotBrush>,
    prefix: usize,
    scale: f32,
) -> Option<Frozen> {
    if layout.is_rtl() || !layout.inline_boxes().is_empty() {
        return None;
    }
    let starts_with_space = layout.get(0).and_then(|line| {
        let run = line.runs().next()?;
        let first = run.clusters().next()?;
        Some(first.is_space_or_nbsp())
    });
    if starts_with_space == Some(true) {
        return None;
    }
    let count = layout.len();
    let mut lines = Vec::with_capacity(count);
    let mut runs = Vec::new();
    let mut pieces = Vec::new();
    let mut glyphs = Vec::new();
    let mut clusters = Vec::new();
    for (index, line) in layout.lines().enumerate() {
        if index + 1 < count && line.break_reason() != parley::BreakReason::Explicit {
            return None;
        }
        let metrics = *line.metrics();
        debug_assert!(
            metrics.offset == 0.0 && metrics.inline_min_coord == INLINE_MIN,
            "a layout aligned or banded before it is frozen"
        );
        let top = crate::shape::lines::top(&metrics);

        let first_run = runs.len();
        for run in line.runs() {
            if run.is_rtl() {
                return None;
            }
            let first_cluster = clusters.len();
            let mut before = 0.0;
            for cluster in run.clusters() {
                let width = cluster.advance();
                let text = cluster.text_range();
                let leading = before;
                before += width;
                if text.end <= prefix {
                    continue;
                }
                clusters.push(Cluster {
                    start: u16::try_from(text.start.max(prefix) - prefix).ok()?,
                    end: u16::try_from(text.end - prefix).ok()?,
                    offset: leading,
                    advance: width,
                });
            }
            runs.push(Run {
                advance: run.advance(),
                clusters: span(first_cluster, clusters.len())?,
            });
        }

        let first_piece = pieces.len();
        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                return None;
            };
            let first_glyph = glyphs.len();
            let baseline = glyph_run.baseline();
            glyphs.extend(glyph_run.glyphs().map(|glyph| Glyph {
                x: glyph.x,
                y: glyph.y + baseline - top,
                advance: glyph.advance,
                id: glyph.id as u16,
            }));
            if glyphs.len() == first_glyph {
                continue;
            }
            let run = glyph_run.run();
            let synthesis = run.synthesis();
            pieces.push(Piece {
                font: run.font().clone(),
                coords: run.normalized_coords().into(),
                size: run.font_size(),
                embolden: synthesis.embolden(),
                skew: synthesis.skew().unwrap_or(0.0),
                brush: glyph_run.style().brush.0,
                start: glyph_run.offset(),
                text_start: u32::try_from(run.text_range().start.saturating_sub(prefix))
                    .unwrap_or(u32::MAX),
                glyphs: span(first_glyph, glyphs.len())?,
            });
        }

        lines.push(Line {
            trailing: metrics.trailing_whitespace,
            offset: metrics.offset,
            runs: span(first_run, runs.len())?,
            pieces: span(first_piece, pieces.len())?,
        });
    }
    Some(Frozen {
        lines: lines.into_boxed_slice(),
        runs: runs.into_boxed_slice(),
        pieces: pieces.into_boxed_slice(),
        glyphs: glyphs.into_boxed_slice(),
        clusters: clusters.into_boxed_slice(),
        scale,
    })
}

/// A range of indices, in 32 bits.
fn span(start: usize, end: usize) -> Option<Range<u32>> {
    Some(u32::try_from(start).ok()?..u32::try_from(end).ok()?)
}

/// A range of 32-bit indices, as a slice index.
fn at(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

impl Frozen {
    /// Whether the frozen form can answer `request`.
    pub(crate) fn answers(request: &BreakRequest<'_>) -> bool {
        request.bands.is_empty() && request.boxes.is_empty()
    }

    /// Device pixels per CSS pixel, as shaped.
    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// The lines `request` gives, from `last`, the lines of the current break.
    ///
    /// The caller makes sure the form [`answers`](Frozen::answers) the request.
    pub(crate) fn lay_out(&self, request: &BreakRequest<'_>, last: &BrokenParagraph) -> Laid {
        let geometry = &last.geometry;
        let indent = request.indent().0;
        let options = request.paragraph.indent;
        // The engine indents the first line and, for `each-line`, each line after a forced break.
        // Every line after the first starts after one (see `freeze`).
        let indent_of = |index: usize| {
            if (index == 0 || options.each_line) ^ options.hanging {
                indent
            } else {
                0.0
            }
        };

        // The engine's own arithmetic, in its order: the layout width first, because a line broken
        // without a width aligns within it.
        let mut width = 0.0_f32;
        for (index, (line, kept)) in self.lines.iter().zip(&geometry.lines).enumerate() {
            let extent = INLINE_MIN + kept.width.0 + indent_of(index).max(0.0);
            width = width.max(extent - line.trailing);
        }
        let max_advance = request.max_advance.map_or(f32::MAX, |width| width.0);
        let mut offsets = SmallVec::with_capacity(self.lines.len());
        for (index, (line, kept)) in self.lines.iter().zip(&geometry.lines).enumerate() {
            let indent = indent_of(index);
            let mut inline_max = INLINE_MIN + max_advance;
            if max_advance >= f32::MAX && inline_max >= f32::MAX {
                inline_max = width;
            }
            let free = (inline_max - INLINE_MIN) - indent - kept.width.0 + line.trailing;
            let mut offset = indent;
            // A line that overflows keeps the start edge. A justified line ends at a forced break
            // or at the end, so it keeps the start edge too.
            let fits = matches!(
                free.partial_cmp(&0.0),
                Some(core::cmp::Ordering::Greater) | None
            );
            if fits {
                match request.paragraph.align {
                    TextAlign::End | TextAlign::Right => offset += free,
                    TextAlign::Center => offset += free * 0.5,
                    TextAlign::Start | TextAlign::Left | TextAlign::Justify => {}
                }
            }
            offsets.push(offset);
        }

        let kept = width.to_bits() == geometry.size.width.0.to_bits()
            && offsets
                .iter()
                .zip(&geometry.lines)
                .all(|(offset, line)| (offset + INLINE_MIN).to_bits() == line.offset.0.to_bits());
        let broken = if kept {
            last.clone()
        } else {
            let mut lines = geometry.lines.clone();
            for (line, offset) in lines.iter_mut().zip(&offsets) {
                line.offset = CssPx(offset + INLINE_MIN);
            }
            BrokenParagraph {
                geometry: Arc::new(TextGeometry {
                    lines,
                    size: Size::new(CssPx(width), geometry.size.height),
                    is_rtl: false,
                }),
                boxes: SmallVec::new(),
            }
        };
        Laid {
            broken,
            kept,
            offsets,
        }
    }

    /// Makes `laid` the current break, which glyphs and clusters are read from.
    pub(crate) fn adopt(&mut self, laid: &Laid) {
        for (line, offset) in self.lines.iter_mut().zip(&laid.offsets) {
            line.offset = *offset;
        }
    }

    /// Visits the style-uniform runs of one line, positioned against the line box's top-left
    /// corner. See [`glyphs`](crate::shape::glyphs).
    pub(crate) fn visit_line(
        &self,
        fonts: &FontSystem,
        line: u16,
        visit: &mut dyn FnMut(ShapedRun<'_>),
    ) {
        let Some(line) = self.lines.get(line as usize) else {
            return;
        };
        let alignment = line.offset;
        let left = alignment + INLINE_MIN;
        let mut glyphs: Vec<ShapedGlyph> = Vec::new();
        for piece in &self.pieces[at(&line.pieces)] {
            glyphs.clear();
            let mut pen = (piece.start + INLINE_MIN) + alignment;
            glyphs.extend(self.glyphs[at(&piece.glyphs)].iter().map(|glyph| {
                let x = glyph.x + pen;
                pen += glyph.advance;
                ShapedGlyph {
                    glyph: glyph.id,
                    x: x - left,
                    y: glyph.y,
                }
            }));
            visit(crate::shape::glyphs::run_of(
                fonts,
                &piece.font,
                &piece.coords,
                piece.size,
                piece.embolden,
                piece.skew,
                piece.brush,
                &glyphs,
                piece.text_start,
            ));
        }
    }

    /// Visits the direction-uniform cluster runs of one line, in the order they are drawn. See
    /// [`clusters`](crate::shape::clusters).
    pub(crate) fn visit_clusters(&self, line: u16, visit: &mut dyn FnMut(ClusterRun<'_>)) {
        let Some(line) = self.lines.get(line as usize) else {
            return;
        };
        let alignment = line.offset;
        // The engine measures a run's start from the alignment offset across the runs before it.
        let mut edge = alignment;
        let mut clusters: Vec<ClusterGeometry> = Vec::new();
        for run in &self.runs[at(&line.runs)] {
            let start = edge - alignment;
            edge += run.advance;
            if run.clusters.is_empty() {
                continue;
            }
            clusters.clear();
            clusters.extend(self.clusters[at(&run.clusters)].iter().map(|cluster| {
                ClusterGeometry {
                    text: usize::from(cluster.start)..usize::from(cluster.end),
                    offset: CssPx(cluster.offset),
                    advance: CssPx(cluster.advance),
                }
            }));
            visit(ClusterRun {
                direction: TextDirection::LeftToRight,
                start: CssPx(start),
                clusters: &clusters,
            });
        }
    }

    /// The heap bytes the form holds.
    pub(crate) fn heap_bytes(&self) -> usize {
        core::mem::size_of_val(&*self.lines)
            + core::mem::size_of_val(&*self.runs)
            + core::mem::size_of_val(&*self.pieces)
            + core::mem::size_of_val(&*self.glyphs)
            + core::mem::size_of_val(&*self.clusters)
            + self
                .pieces
                .iter()
                .map(|piece| core::mem::size_of_val(&*piece.coords))
                .sum::<usize>()
    }
}
