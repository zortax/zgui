//! What a checkbox has drawn inside itself.
//!
//! A checkbox carries both of the marks it can show and shows one of them: the tick and the
//! part-way dash name the same cell of its grid, and whichever is not wanted is faded out. So "the
//! mark looks offset" and "both marks are on top of each other" are the same picture, and telling
//! them apart needs the ink inside one box measured against that box's own edges.
//!
//! The boxes are found by shape rather than by what they say, because a checkbox says nothing. What
//! is written down for each is the box itself, in device pixels from the window's corner, so that
//! the ink in the capture can be weighed against the edges it is supposed to be centred between.

use zgui::geom::{Device, DevicePx, Rect};

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;
use crate::script::verdict;
use crate::stage::Stage;
use crate::stage::census::Census;

/// How much room to leave around the row in the picture, in device pixels.
const MARGIN: f32 = 6.0;

/// The smallest a control's box can be and still be one, in device pixels.
const SMALLEST: f32 = 8.0;

/// How far from square a box may be and still count as one.
const SQUARENESS: f32 = 0.4;

/// Looks at the row of checkboxes and writes down where each box is.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Checkbox") else {
        stage
            .report
            .note("Checkbox", "the Checkbox panel is not laid out");
        return;
    };
    let Some(row) = verdict::row_of(&census, panel, "states") else {
        stage
            .report
            .note("Checkbox", "the row of states is not laid out");
        return;
    };
    // Nothing hovered: a capture with the pointer resting on a control is a capture of that control
    // lit, and the ring a hover draws is ink inside the same rectangle the mark is measured in.
    stage.leave();

    let boxes = squares(&census, row);
    stage.report.check(
        "Checkbox",
        "the row shows four boxes",
        boxes.len() == 4,
        &format!(
            "{} near-square boxes: {}",
            boxes.len(),
            boxes
                .iter()
                .map(|rect| format!(
                    "{:.0},{:.0} {:.0}x{:.0}",
                    rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
                ))
                .collect::<Vec<_>>()
                .join("  ")
        ),
    );
    // In the order the gallery lays them out: on, off, part-way, disabled.
    for (index, rect) in boxes.iter().enumerate() {
        find::mark(stage, &format!("checkbox:{index}"), *rect);
    }
    shot_of(stage, "vd-mark-row", verdict::grown(row, MARGIN));
}

/// Every near-square laid-out box inside `row` that says nothing, one per place, left to right.
///
/// One per place matters: a checkbox and the mark inside it are both square, both silent and
/// concentric, so a list of matching nodes is twice as long as the list of controls. The outermost
/// at each place is the control, which is the box the ink has to be centred in.
fn squares(census: &Census, row: Rect<DevicePx, Device>) -> Vec<Rect<DevicePx, Device>> {
    let mut found: Vec<Rect<DevicePx, Device>> = Vec::new();
    for node in census.inside(row) {
        if !node.text.is_empty() {
            continue;
        }
        let Some(rect) = node.rect else { continue };
        if rect.size.width.0 < SMALLEST || rect.size.height.0 < SMALLEST {
            continue;
        }
        if (rect.size.width.0 - rect.size.height.0).abs() > rect.size.width.0 * SQUARENESS {
            continue;
        }
        match found
            .iter_mut()
            .find(|other| (other.origin.x.0 - rect.origin.x.0).abs() < rect.size.width.0)
        {
            Some(existing) => {
                if area(&rect) > area(existing) {
                    *existing = rect;
                }
            }
            None => found.push(rect),
        }
    }
    found.sort_by(|left, right| left.origin.x.0.total_cmp(&right.origin.x.0));
    found
}

/// How much of the window `rect` covers.
fn area(rect: &Rect<DevicePx, Device>) -> f32 {
    rect.size.width.0 * rect.size.height.0
}
