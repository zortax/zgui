//! Breaking already shaped glyphs into lines.

use parley::{Alignment, AlignmentOptions, IndentOptions};
use zgui_profile::{Counter, counter};
use zgui_text::{BreakRequest, BrokenParagraph, Plan, ShapedParagraph};
use zgui_text_style::TextAlign;

use crate::shape::engine::ShapedLayout;
use crate::shape::{bands, boxes, lines};

/// Breaks a shaped paragraph at the requested width, or reports a pass it has already taken.
///
/// The question of whether a pass is owed is asked once, in one place, and answered by the shaped
/// result itself — so a caller cannot report a cheap pass and take an expensive one, or the
/// reverse. Two of the three answers cost nothing: the glyphs already reflect the request, or a
/// probe is asking about a width some earlier pass already measured. Which is what makes a layout
/// engine's repeated width probes cost nothing at all rather than costing a break each.
///
/// An owed pass costs little as well when its width is one at which the engine's breaker takes
/// the decisions of the pass before it: the layout already holds those lines (see
/// [`reach`](crate::shape::reach)).
pub(crate) fn break_lines(
    shaped: &mut ShapedParagraph<ShapedLayout>,
    request: &BreakRequest<'_>,
) -> BrokenParagraph {
    match shaped.plan_break(request) {
        Plan::Reflected => return shaped.engine.last.clone(),
        Plan::Recalled(broken) => return broken.clone(),
        Plan::Owed => {}
    }
    let has_boxes = !request.boxes.is_empty();
    let banded = !request.bands.is_empty();
    let indent = request.indent();
    let alignment = alignment(request.paragraph.align);
    let justified = alignment == Alignment::Justify;
    let options = IndentOptions {
        each_line: request.paragraph.indent.each_line,
        hanging: request.paragraph.indent.hanging,
    };
    let max_advance = request.max_advance.map(|width| width.0);
    let engine = &mut shaped.engine;
    let prefix = engine.prefix;
    let wraps = engine.wraps;

    let layout = &mut engine.layout;
    let boxes_moved = boxes::push_geometry(layout, request.boxes);

    // A pass whose width keeps every comparison of the engine's breaker gives the lines the layout
    // already holds, so the engine is not asked to break again. See `reach`. A paragraph that may
    // not wrap and holds boxes is broken at a width of its own, below.
    let covered = !banded
        && matches!(alignment, Alignment::Start | Alignment::Left)
        && !layout.is_rtl()
        && (wraps || !has_boxes);
    if covered
        && !boxes_moved
        && engine
            .reach
            .holds(max_advance, indent.0, options, has_boxes)
    {
        counter::bump(Counter::TextBreaksSkipped);
        counter::bump(Counter::TextLinesKept);
        let broken = engine.last.clone();
        shaped.remember(request.key(), broken.clone());
        return broken;
    }

    // Set unconditionally, including back to zero: an indent left over from a previous request
    // would otherwise survive a re-style that removed it.
    layout.set_text_indent(indent.0, options);
    if !banded {
        let mut max_advance = max_advance;
        // The engine marks a break after every inline box, whatever the wrap mode. A line that
        // may not wrap and overflows would break back at such a box, so it is broken at a width
        // that holds its longest line. The line keeps its own width, and whatever cuts it off
        // still sees it overflow the box.
        if !wraps && has_boxes {
            layout.break_all_lines(None);
            max_advance = max_advance.map(|width| width.max(layout.full_width()));
        }
        layout.break_all_lines(max_advance);
    } else {
        bands::break_banded(
            layout,
            request.max_advance.map(|width| width.0),
            request.bands,
        );
    }
    layout.align(
        alignment,
        AlignmentOptions {
            align_when_overflowing: false,
        },
    );

    // A width step in a drag mostly gives the lines of the step before it. The engine has broken
    // the glyphs again here, so painting reads the new break. The reported lines and box
    // placements are kept when they are equal bit for bit, which saves the walk over every glyph
    // that placing the boxes costs.
    let kept = !banded
        && !justified
        && !boxes_moved
        && !engine.items.is_empty()
        && lines::unchanged(layout, &engine.last, &engine.items, has_boxes, prefix);
    let broken = if kept {
        counter::bump(Counter::TextLinesKept);
        engine.last.clone()
    } else {
        let broken = lines::read(layout, has_boxes, prefix);
        engine.last = broken.clone();
        broken
    };
    if banded || justified {
        engine.items.clear();
    } else if !kept {
        lines::items(layout, &mut engine.items);
    }
    if justified {
        engine.reach.forget();
    } else {
        engine.reach.measure(
            layout,
            covered,
            engine.rebroken,
            max_advance,
            indent.0,
            options,
            has_boxes,
        );
        engine.rebroken |= covered;
    }
    // Filed under the key the pass was taken for, which is the one `plan_break` just adopted, so a
    // later probe at the same width is answered from here without moving the laid-out form.
    shaped.remember(request.key(), broken.clone());
    broken
}

/// The engine's spelling of an alignment.
///
/// `start` and `end` are resolved against the paragraph's own base direction by the engine, which
/// is why forcing that direction correctly matters to alignment and not only to reading order: a
/// paragraph whose base level came out wrong reorders its content correctly and then aligns it to
/// the wrong edge.
fn alignment(align: TextAlign) -> Alignment {
    match align {
        TextAlign::Start => Alignment::Start,
        TextAlign::End => Alignment::End,
        TextAlign::Left => Alignment::Left,
        TextAlign::Right => Alignment::Right,
        TextAlign::Center => Alignment::Center,
        TextAlign::Justify => Alignment::Justify,
    }
}
