//! Breaking already shaped glyphs into lines.

use parley::{Alignment, AlignmentOptions, IndentOptions};
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
    let indent = request.indent();
    let alignment = alignment(request.paragraph.align);
    let prefix = shaped.engine.prefix;
    let layout = &mut shaped.engine.layout;

    boxes::push_geometry(layout, request.boxes);
    // Set unconditionally, including back to zero: an indent left over from a previous request
    // would otherwise survive a re-style that removed it.
    layout.set_text_indent(
        indent.0,
        IndentOptions {
            each_line: request.paragraph.indent.each_line,
            hanging: request.paragraph.indent.hanging,
        },
    );
    if request.bands.is_empty() {
        layout.break_all_lines(request.max_advance.map(|width| width.0));
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

    let broken = lines::read(layout, has_boxes, prefix);
    shaped.engine.last = broken.clone();
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
