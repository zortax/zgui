//! The inline formatting context: one measure call, four steps, one of them ever expensive.
//!
//! A call does the same four things in the same order every time. Lay out whatever is on the line
//! that is not text, because its size and its alignment are inputs to everything after. Shape the
//! paragraph, or find it already shaped. Break it at the width the algorithm is currently proposing.
//! Compute the CSS line boxes on top of what came back, and stack them.
//!
//! Only the second step costs anything, and it happens once per distinct content — which is what
//! makes a width change, of which there are many per layout, cost a break rather than a shape.
//!
//! # The two passes that are not the first one
//!
//! `vertical-align: top` and `bottom` are resolved against the line box, which does not exist until
//! everything else on the line has been placed, so a line carrying one is broken again with the
//! shift the first pass revealed. Floats do the same thing one level up: the width each line may
//! take depends on which floats it is level with, which depends on where the previous lines ended.
//! Both are bounded loops rather than fixpoints left to settle.

use taffy::{AvailableSpace, BlockContext, RequestedAxis, RunMode, Size};
use zgui_dom::side::BoxKey;
use zgui_geom::CssPx;
use zgui_text::{BreakRequest, LineBand, LineBands, ParagraphContent};

use crate::inline::content::Role;
use crate::inline::lines::LineBox;
use crate::inline::resolved::{InlineResolution, Placement};
use crate::inline::vertical_align::scale_strut;
use crate::inline::{boxes, ellipsis, floats, lines, strut};
use crate::measure::{MeasureContent, Measured};
use crate::tree::LayoutTree;

/// How many times the line-relative alignments may be re-resolved before the answer is taken as it
/// is.
///
/// One pass settles the ordinary case: the first break places everything else, the line box follows
/// from that, and the shift follows from the line box. More than one is needed only when a line
/// carries both a `top`-aligned box and a `bottom`-aligned one, because each of them deepens the
/// line on the side the other is measured from. That exchange only ever makes the line taller, so
/// it settles — but it settles on the box heights, and a document that mixes enough of them can
/// take longer than a frame is worth. A layout that stops one pass early is a box a pixel out; a
/// layout that does not stop is a window that never paints again.
const MAX_REALIGN_PASSES: usize = 4;

/// What the layout algorithm is asking of an inline formatting context.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ask {
    /// Dimensions already fixed.
    pub(crate) known: Size<Option<f32>>,
    /// The space available, with the box's own insets already taken off.
    pub(crate) available: Size<AvailableSpace>,
    /// Whether a size or a whole layout is wanted.
    pub(crate) run_mode: RunMode,
    /// Which axis the answer is read on.
    pub(crate) axis: RequestedAxis,
    /// Whether this answer is the one that will be kept.
    pub(crate) final_pass: bool,
}

impl Ask {
    /// The width the lines are to be broken at, or nothing for "as wide as they like".
    fn constraint(&self) -> Constraint {
        match self.known.width {
            Some(width) => Constraint::Definite(width),
            None => match self.available.width {
                AvailableSpace::Definite(width) => Constraint::Definite(width),
                AvailableSpace::MinContent => Constraint::MinContent,
                AvailableSpace::MaxContent => Constraint::MaxContent,
            },
        }
    }
}

/// How far past its width a line may reach and still be treated as fitting, in device pixels.
///
/// A box is very often sized to hold exactly one line: its width *is* the paragraph's max-content
/// width plus its own padding and border. Handing the paragraph back the space inside that box means
/// adding those insets and taking them off again, and in binary floating point the round trip does
/// not always land where it started — a shortfall of a millionth of a pixel is enough. Whether it
/// does depends on the display's scale, because that is what makes the numbers fractional at all,
/// which is why a row that reads as one line at 1.0 and at 1.5 breaks in two at 1.2.
///
/// So a line that overflows by less than a sixty-fourth of a device pixel is a line that fits. The
/// figure is not tuned to the arithmetic: it is the layout unit a browser rounds everything to, well
/// under anything a raster can show and orders of magnitude over the error being absorbed.
const BREAK_TOLERANCE: f32 = 1.0 / 64.0;

/// Which inline-axis question is being asked.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Constraint {
    /// A definite width.
    Definite(f32),
    /// The narrowest the content can be.
    MinContent,
    /// The widest it would like to be.
    MaxContent,
}

/// Lays out one inline formatting context.
pub(crate) fn compute<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    ask: Ask,
    block: Option<&mut BlockContext<'_>>,
) -> Measured {
    let generated = crate::inline::content_of(tree, key);
    let scale = tree.device().scale;
    let basis = match ask.constraint() {
        Constraint::Definite(width) => Some(width),
        _ => None,
    };
    let root_strut = scale_strut(tree.content().strut(&generated.root), scale);

    // Everything on the line that is not a glyph, at the size and the alignment it currently has.
    let mut boxes = boxes::resolve(tree, &generated, &root_strut, ask.available, basis);
    let run_extents = strut::of_runs(tree.content(), &generated.runs, scale);

    let content = ParagraphContent {
        text: &generated.text,
        map: &generated.map,
        runs: &generated.runs,
        boxes: &boxes.geometry,
        paragraph: &generated.paragraph,
        scale,
    };
    let paragraph_key = generated.key(&content);
    let summary = tree.content().shape_keyed(paragraph_key, &content);

    // An inline-axis intrinsic probe is a question about the glyphs alone. Answering it from the
    // shaped result is not an optimisation of the general path: breaking at a candidate width would
    // both cost a pass and overwrite the lines the context is currently holding.
    if let (RunMode::ComputeSize, RequestedAxis::Horizontal, Constraint::MinContent) =
        (ask.run_mode, ask.axis, ask.constraint())
    {
        return probe(tree, key, summary.widths.min.0);
    }
    if let (RunMode::ComputeSize, RequestedAxis::Horizontal, Constraint::MaxContent) =
        (ask.run_mode, ask.axis, ask.constraint())
    {
        return probe(tree, key, summary.widths.max.0);
    }

    let max_advance = match ask.constraint() {
        Constraint::Definite(width) => Some(CssPx(width + BREAK_TOLERANCE)),
        Constraint::MinContent => Some(summary.widths.min),
        Constraint::MaxContent => None,
    };

    // A pass whose answer is not the one that will be kept, on a context with nothing on its lines
    // that has to be placed twice and no float to band around, is one break and one line-box
    // computation and nothing else. Answered here so that it writes nothing: it neither moves the
    // shaper's laid-out form — which is what the paragraph's glyphs are drawn from — nor replaces
    // the resolution the last kept pass left behind. Both of those would be a measurement taken at
    // a candidate width becoming what the box is painted as.
    if !ask.final_pass && !boxes.needs_line_box() && !floats::any_floats(block.as_deref()) {
        let request = BreakRequest {
            runs: &generated.runs,
            boxes: &boxes.geometry,
            paragraph: &generated.paragraph,
            max_advance,
            indent_basis: max_advance,
            bands: LineBands::NONE,
            probe: true,
        };
        let broken = tree.content().break_lines(summary.key, &request);
        let computed = lines::compute(
            &broken,
            &generated.runs,
            &run_extents,
            &root_strut,
            &boxes.geometry,
            &broken.boxes,
        );
        return Measured {
            size: Size {
                width: lines::width(&computed),
                height: lines::height(&computed),
            },
            first_baseline: computed.first().map(LineBox::baseline),
            last_baseline: computed.last().map(LineBox::baseline),
        };
    }

    let mut bands: Vec<LineBand> = Vec::new();
    let mut broken;
    let mut computed: Vec<LineBox>;
    let mut passes = 0;
    let mut realignments = 0;
    loop {
        let request = BreakRequest {
            runs: &generated.runs,
            boxes: &boxes.geometry,
            paragraph: &generated.paragraph,
            max_advance,
            indent_basis: max_advance,
            bands: LineBands::new(&bands),
            probe: false,
        };
        broken = tree.content().break_lines(summary.key, &request);
        computed = lines::compute(
            &broken,
            &generated.runs,
            &run_extents,
            &root_strut,
            &boxes.geometry,
            &broken.boxes,
        );
        passes += 1;

        // A box aligned with the line box's own edges could not be placed before the line box
        // existed. Now that it does, the shift is resolved and the affected lines are broken again.
        if realignments < MAX_REALIGN_PASSES
            && boxes.needs_line_box()
            && realign_to_lines(&mut boxes, &broken, &computed)
        {
            realignments += 1;
            continue;
        }
        let Some(block) = block.as_deref() else {
            break;
        };
        // A probe with no width to break into has no bands either: what a float leaves free is a
        // question about a width, and answering it against nothing would squeeze every line to
        // zero rather than report how wide the content wants to be.
        let Some(available) = max_advance else {
            break;
        };
        if !floats::any_floats(Some(block)) || passes >= floats::MAX_BAND_PASSES {
            break;
        }
        let (top, left) = content_origin(tree, key);
        let width = available.0;
        let next = floats::bands(block, top, left, width, &computed);
        if !floats::differ(&bands, &next) {
            break;
        }
        bands = next;
    }

    let placements = place(
        tree,
        key,
        &generated,
        &boxes,
        &computed,
        &broken,
        ask.final_pass,
    );
    // Only the kept pass, and only against a definite width. A probe is answered from the recalled
    // break rather than from the engine's laid-out form, so its clusters are not the ones this
    // context is currently holding — and an intrinsic probe has no box edge to overflow in the
    // first place.
    let ellipsis = match (ask.final_pass, ask.constraint()) {
        (true, Constraint::Definite(width)) => mark_overflowing_lines(
            tree,
            key,
            &generated,
            summary.key,
            &mut computed,
            broken.geometry.is_rtl,
            width,
        ),
        _ => Default::default(),
    };
    let paragraph = tree.intern_paragraph(summary.key);
    let resolution = InlineResolution {
        paragraph,
        key: summary.key,
        lines: computed,
        placements,
        is_rtl: broken.geometry.is_rtl,
        map: generated.map.clone(),
        sources: generated.sources.clone(),
        ellipsis,
    };
    let measured = Measured {
        size: Size {
            width: lines::width(&resolution.lines),
            height: lines::height(&resolution.lines),
        },
        first_baseline: resolution.first_baseline(),
        last_baseline: resolution.last_baseline(),
    };
    tree.set_inline_resolution(key, resolution);
    measured
}

/// Marks every line of a context that reaches past its box, and shapes what marks them.
///
/// The whole of `text-overflow`'s cost falls here, and nearly all of it is skipped: a box that does
/// not clip its inline axis returns at the first test, and one whose lines all fit returns at the
/// second. Only a context that is genuinely cut off shapes a mark or walks a cluster.
fn mark_overflowing_lines<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    generated: &crate::inline::content::Generated,
    paragraph: zgui_text::ParagraphKey,
    lines: &mut [LineBox],
    is_rtl: bool,
    available: f32,
) -> ellipsis::EllipsisSource {
    let empty = ellipsis::EllipsisSource::default();
    let Some(governing) = ellipsis::governing(tree.structure(), key) else {
        return empty;
    };
    let style = tree.structure().node(governing).style.clone();
    if !ellipsis::clips_inline_axis(&style) {
        return empty;
    }
    let overflowing = ellipsis::any_overflows(lines, available, BREAK_TOLERANCE);
    if overflowing.is_none() {
        return empty;
    }
    let sides = ellipsis::sides_of(&style, is_rtl);
    let mut source = ellipsis::EllipsisSource::default();
    if overflowing.start {
        source.start = shape_mark(tree, generated, sides.start.text());
    }
    if overflowing.end {
        source.end = shape_mark(tree, generated, sides.end.text());
    }
    if source.is_empty() {
        return empty;
    }
    ellipsis::annotate(
        tree.content(),
        paragraph,
        lines,
        &source,
        available,
        BREAK_TOLERANCE,
    );
    source
}

/// Shapes one mark in the context's own root style, and interns the paragraph it became.
///
/// The root style rather than a run's, because the mark says the *box* cut its content: a run
/// inside it may be bold or another size, and the ellipsis is not part of that run. Shaping is
/// keyed on the content exactly as every other paragraph is, so a hundred labels sharing one style
/// share one shaped ellipsis between them.
fn shape_mark<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    generated: &crate::inline::content::Generated,
    text: Option<&str>,
) -> Option<ellipsis::EllipsisMark> {
    let text = text?;
    if text.is_empty() {
        return None;
    }
    let runs = ellipsis::runs(text, generated.root.clone(), generated.root_brush);
    let map = zgui_text::TextMap::new();
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &generated.paragraph,
        scale: tree.device().scale,
    };
    let summary = tree.content().shape(&content);
    // Broken once and kept, so the engine's laid-out form holds the mark's single line — which is
    // what the painter pulls glyphs from. Every frame after this one recalls it and pays nothing.
    let request = BreakRequest {
        runs: &runs,
        boxes: &[],
        paragraph: &generated.paragraph,
        max_advance: None,
        indent_basis: None,
        bands: LineBands::NONE,
        probe: false,
    };
    tree.content().break_lines(summary.key, &request);
    let paragraph = tree.intern_paragraph(summary.key);
    Some(ellipsis::EllipsisMark {
        paragraph,
        key: summary.key,
        width: summary.widths.max.0,
    })
}

/// The answer to an inline-axis intrinsic probe: one width, and whatever height the context
/// currently holds.
///
/// The height is not recomputed because the algorithm asking discards it, and computing it would
/// mean breaking at a width the context is not going to be laid out at.
fn probe<C: MeasureContent>(tree: &LayoutTree<'_, C>, key: BoxKey, width: f32) -> Measured {
    let held = tree.inline_resolution_of(key);
    Measured {
        size: Size {
            width,
            height: held.map_or(0.0, |resolution| lines::height(&resolution.lines)),
        },
        first_baseline: held.and_then(InlineResolution::first_baseline),
        last_baseline: held.and_then(InlineResolution::last_baseline),
    }
}

/// Re-resolves every box that aligns with the line box's own edges, now that the line boxes exist.
///
/// Returns whether any shift moved, which is what decides whether another breaking pass is owed:
/// a box that grew upwards may have made its line taller, and a taller line is a different line
/// box to align against.
fn realign_to_lines(
    boxes: &mut boxes::Boxes,
    broken: &zgui_text::BrokenParagraph,
    computed: &[LineBox],
) -> bool {
    let mut moved = false;
    for (geometry, alignment) in boxes.geometry.iter_mut().zip(&boxes.alignments) {
        if !alignment.needs_line_box() {
            continue;
        }
        let Some(placed) = broken.boxes.iter().find(|placed| placed.id == geometry.id) else {
            continue;
        };
        let Some(line) = computed.get(placed.line) else {
            continue;
        };
        let shift = crate::inline::vertical_align::resolve_against_line(
            *alignment,
            geometry.ascent.0,
            geometry.height.0,
            line.extents.above,
            line.extents.below,
        );
        if (shift - geometry.shift.0).abs() > f32::EPSILON {
            geometry.shift = CssPx(shift);
            moved = true;
        }
    }
    moved
}

/// Writes every atomic inline's position into the box tree, and reports where they went.
///
/// The x is the shaper's, because packing the boxes between the words is what a shaper does. The y
/// is ours: the shaper's own line stacking is not the CSS one, so the box hangs off the line box
/// computed here at exactly the distance below the baseline it was declared to have.
fn place<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    generated: &crate::inline::content::Generated,
    boxes: &boxes::Boxes,
    computed: &[LineBox],
    broken: &zgui_text::BrokenParagraph,
    write: bool,
) -> Vec<Placement> {
    let _ = key;
    let mut out = Vec::new();
    for placed in &broken.boxes {
        let Some(item) = generated.item(placed.id) else {
            continue;
        };
        let Role::Atomic(box_) = item.role else {
            continue;
        };
        let Some(geometry) = boxes.geometry.iter().find(|entry| entry.id == placed.id) else {
            continue;
        };
        let Some((x, y)) = lines::placement(computed, placed, geometry) else {
            continue;
        };
        out.push(Placement {
            box_,
            origin: (x, y),
            line: placed.line,
        });
        if write {
            let index = boxes
                .geometry
                .iter()
                .position(|entry| entry.id == placed.id)
                .expect("the geometry the placement came from");
            let frame = boxes.frames.get(index).copied().flatten();
            let state = tree.state_mut(box_);
            // The whole result, not only the position: an atomic inline is laid out by the line it
            // sits on, and no algorithm above it will write the box it came out at.
            state.unrounded.location = taffy::Point { x, y };
            if let Some(frame) = frame {
                state.unrounded.size = frame.size;
                state.unrounded.content_size = frame.size;
                state.unrounded.margin = frame.margin;
                state.unrounded.padding = frame.padding;
                state.unrounded.border = frame.border;
            }
            state.snapped = state.unrounded;
        }
    }
    out
}

/// Where the content of one box begins inside it, measured from its own border edges.
fn content_origin<C: MeasureContent>(tree: &LayoutTree<'_, C>, key: BoxKey) -> (f32, f32) {
    let layout = tree.state(key).map(|state| state.unrounded);
    let top = layout.map_or(0.0, |layout| layout.border.top + layout.padding.top);
    let left = layout.map_or(0.0, |layout| layout.border.left + layout.padding.left);
    (top, left)
}
