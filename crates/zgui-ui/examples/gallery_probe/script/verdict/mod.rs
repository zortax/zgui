//! A handful of questions about the shipped gallery, each answered from the window on the screen.
//!
//! What separates these from the rest of the run is what they are allowed to conclude from. Every
//! other part asks the document where a control is and whether it answered; these ask whether the
//! *picture* is right, and a picture is only evidence about a component if the rectangle it is
//! judged over came out of the laid-out document rather than being chosen afterwards by eye. So each
//! part writes the rectangles down beside the captures, in device pixels from the window's own
//! corner, and leaves the reading of the pixels to whatever is looking at the files.
//!
//! Each is a section of its own because each ends in real time passing — a caret's blink, a pulse's
//! period, a toast's exit — and the loop has to be handed back between them.

pub(crate) mod caret;
pub(crate) mod handle;
pub(crate) mod list;
pub(crate) mod mark;
pub(crate) mod modal;
pub(crate) mod pulse;
pub(crate) mod toasts;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};

use crate::stage::Stage;
use crate::stage::census::{Census, Seen};

/// The parts, in the order they are run.
pub(crate) fn sections() -> Vec<crate::script::Section> {
    let mut sections: Vec<crate::script::Section> = vec![
        ("vd-mark", mark::run as fn(&mut Stage<'_>)),
        ("vd-caret", caret::run),
        ("vd-pulse", pulse::run),
    ];
    sections.extend(core::iter::repeat_n(
        ("vd-toasts", toasts::chunk as fn(&mut Stage<'_>)),
        toasts::STEPS,
    ));
    sections.push(("vd-modal", modal::run));
    sections.push(("vd-list", list::run));
    sections.push(("vd-handle", handle::run));
    sections
}

/// `rect` with `margin` device pixels added on every side.
///
/// What a picture of a small control has to be cropped to. A crop of exactly a sixteen-pixel box
/// cannot show whether its ink stops at the edge or runs past it, and "the mark is off to the left"
/// is precisely a claim about the relationship between the ink and the edge.
pub(crate) fn grown(rect: Rect<DevicePx, Device>, margin: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(
            DevicePx(rect.origin.x.0 - margin),
            DevicePx(rect.origin.y.0 - margin),
        ),
        Size::new(
            DevicePx(rect.size.width.0 + margin * 2.0),
            DevicePx(rect.size.height.0 + margin * 2.0),
        ),
    )
}

/// The largest laid-out node inside `panel` whose text begins with `label`, which is one row of it.
///
/// The largest rather than the smallest: the smallest is the label's own text node, and the row is
/// the box that holds both that word and the controls beside it.
pub(crate) fn row_of(
    census: &Census,
    panel: Rect<DevicePx, Device>,
    label: &str,
) -> Option<Rect<DevicePx, Device>> {
    census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.starts_with(label) && node.area() > 0.0)
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
}

/// Every laid-out node whose text is exactly `text`, one per place it appears.
///
/// Several nested nodes carry one string — the text node, the control around it, and every wrapper
/// up to the first holding something else — so a count of matching nodes is not a count of things on
/// the screen. Grouping by where the box is turns one into the other, and the largest box at each
/// place is the outermost node of that group, which is the thing itself rather than its label.
pub(crate) fn one_per_place<'a>(census: &'a Census, text: &str) -> Vec<&'a Seen> {
    let mut found: Vec<&Seen> = Vec::new();
    for node in census.nodes.iter().filter(|node| node.text == text) {
        if node.area() <= 0.0 {
            continue;
        }
        let here = node.rect.map(|rect| (rect.origin.x.0, rect.origin.y.0));
        match found.iter_mut().find(|other| {
            other
                .rect
                .map(|rect| (rect.origin.x.0, rect.origin.y.0))
                .zip(here)
                .is_some_and(|(left, right)| {
                    (left.0 - right.0).abs() < 4.0 && (left.1 - right.1).abs() < 4.0
                })
        }) {
            Some(existing) if existing.area() < node.area() => *existing = node,
            Some(_) => {}
            None => found.push(node),
        }
    }
    found
}
