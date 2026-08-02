//! Whether the icons stay on the screen while the page is used.
//!
//! Every other section asks whether a control answered. This one asks whether the window still
//! *looks* the way it looked, over a part of the page nothing was supposed to change — and it asks
//! it of the compositor, by comparing two captures of the same rectangle taken with the pointer
//! parked in the same place both times.
//!
//! The rectangle is the select's trigger, because the icon in it is the one that went out: an
//! element repaints, its icon is not drawn into the pixels the repaint cleared, and nothing damages
//! that rectangle again until a surface opens over the whole window. So the gesture here is the one
//! that produces it, and the claim is that the trigger afterwards is the trigger before, to the
//! pixel.
//!
//! That claim is made from the captures rather than inside the run, the same way the checkboxes'
//! is: the rectangle is marked here and the pictures are compared afterwards. What this checks for
//! itself is the half a picture cannot be trusted for — that the trigger is still a laid-out box of
//! the size it was, so a comparison of the same rectangle in two captures is a comparison of the
//! same control.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// How many times the list is opened and closed again.
///
/// More than two. Two is the smallest number that reaches the second repaint of an unchanged icon,
/// and a run that stopped there would say nothing about whether the icon comes back.
const CYCLES: usize = 6;

/// The names the trigger can be showing by the time this runs.
///
/// The section is placed after the ones that use the select, so which currency it holds depends on
/// what they left it at — and a finder that named one of them would report the trigger as missing
/// on every run that had changed it.
const CURRENCIES: [&str; 4] = ["Pound sterling", "Euro", "US dollar", "Choose one"];

/// Opens and closes the select's list, and captures the trigger between every cycle.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Select and combobox") else {
        stage
            .report
            .note("Icon", "the select panel is not laid out");
        return;
    };
    let Some(seen) = census
        .inside(panel)
        .into_iter()
        .filter(|node| CURRENCIES.contains(&node.text.as_str()) && node.area() > 0.0)
        .max_by(|left, right| left.area().total_cmp(&right.area()))
    else {
        stage
            .report
            .note("Icon", "the select's trigger is not laid out");
        return;
    };
    let trigger_id = seen.id;
    let Some(trigger) = seen.rect else {
        stage.report.note("Icon", "the select's trigger has no box");
        return;
    };
    find::mark(stage, "select-trigger", trigger);

    // The pointer is parked here for both captures, far enough from the trigger that no hover
    // style is in force in either of them: a picture taken with the trigger lit and one taken with
    // it unlit differ for a reason that is not the one being looked for.
    let parked = Point::new(
        DevicePx(panel.origin.x.0 + panel.size.width.0 - 8.0),
        DevicePx(panel.origin.y.0 + 8.0),
    );
    stage.move_to(parked);
    stage.shot("drawings-00-parked");

    let centre = Point::new(
        DevicePx(trigger.origin.x.0 + trigger.size.width.0 / 2.0),
        DevicePx(trigger.origin.y.0 + trigger.size.height.0 / 2.0),
    );
    for cycle in 1..=CYCLES {
        stage.click(centre);
        stage.settle(8);
        stage.key(NamedKey::Escape);
        stage.settle(8);
        stage.move_to(parked);
        stage.settle(4);
        stage.shot(&format!("drawings-{cycle:02}-cycled"));
    }

    let after = stage
        .census()
        .node(trigger_id)
        .and_then(|seen| seen.rect)
        .map(rect_of);
    stage.report.check(
        "Icon",
        "the select's trigger is where it was after its list has opened and closed",
        after == Some(rect_of(trigger)),
        &format!(
            "{CYCLES} cycles: {:?} became {after:?}; the captures either side are compared \
             afterwards",
            rect_of(trigger)
        ),
    );
}

/// A rectangle as four numbers, for the detail line.
fn rect_of(rect: Rect<DevicePx, Device>) -> (f32, f32, f32, f32) {
    (
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    )
}
