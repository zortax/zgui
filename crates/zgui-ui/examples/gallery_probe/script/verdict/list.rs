//! A select on the page: where its list goes when it opens.
//!
//! The same question the select inside a dialog is asked, put to one that is laid out by the page
//! itself. A list is placed against its trigger by a positioner that works in the window's own
//! coordinates, and a trigger inside a surface that has moved itself is the case that can come apart
//! — so both have to be measured, and a difference between the two is the finding.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;
use crate::script::verdict;
use crate::stage::Stage;

/// What the trigger says, which is whichever currency the gallery starts on.
const CHOSEN: &str = "Pound sterling";

/// A word that only the open list says.
const ITEM: &str = "US dollar";

/// Opens the select and measures where its list landed.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Select and combobox") else {
        stage
            .report
            .note("Select", "the select panel is not laid out");
        return;
    };
    let trigger = verdict::one_per_place(&census, CHOSEN)
        .into_iter()
        .filter(|node| census.on_the_page(node))
        .filter_map(|node| node.rect)
        .filter(|rect| {
            rect.origin.y.0 >= panel.origin.y.0 - 0.5
                && rect.origin.y.0 <= panel.origin.y.0 + panel.size.height.0
        })
        .max_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        });
    let Some(trigger) = trigger else {
        stage.report.note("Select", &stage.presence(CHOSEN));
        return;
    };
    find::mark(stage, "page-select:trigger", trigger);
    stage.click(Point::new(
        DevicePx(trigger.origin.x.0 + trigger.size.width.0 / 2.0),
        DevicePx(trigger.origin.y.0 + trigger.size.height.0 / 2.0),
    ));

    let census = stage.census();
    let list = census
        .nodes
        .iter()
        .filter(|node| node.area() > 0.0 && !census.on_the_page(node) && node.text.contains(ITEM))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect);
    let Some(list) = list else {
        stage.report.note("Select", &stage.presence(ITEM));
        return;
    };
    // The item's own box is what was found, so the list is the surface around it: the smallest
    // floating box that holds it and is wider than it.
    let surface = census
        .nodes
        .iter()
        .filter(|node| node.area() > 0.0 && !census.on_the_page(node))
        .filter_map(|node| node.rect)
        .filter(|rect| holds(*rect, list) && rect.size.width.0 > list.size.width.0)
        .min_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        })
        .unwrap_or(list);
    find::mark(stage, "page-select:list", surface);
    stage.report.check(
        "Select",
        "a select on the page opens its list under its own trigger",
        (surface.origin.x.0 - trigger.origin.x.0).abs() <= 8.0
            && surface.origin.y.0 >= trigger.origin.y.0 - trigger.size.height.0
            && surface.origin.y.0 <= trigger.origin.y.0 + trigger.size.height.0 * 3.0,
        &format!(
            "the trigger is at {} and the list at {}",
            wrote(&trigger),
            wrote(&surface)
        ),
    );
    let window = stage.window();
    shot_of(stage, "vd-list-open", window);
    stage.key(NamedKey::Escape);
    stage.settle(20);
}

/// Whether `outer` contains `inner`.
fn holds(outer: Rect<DevicePx, Device>, inner: Rect<DevicePx, Device>) -> bool {
    outer.origin.x.0 <= inner.origin.x.0 + 0.5
        && outer.origin.y.0 <= inner.origin.y.0 + 0.5
        && outer.origin.x.0 + outer.size.width.0 >= inner.origin.x.0 + inner.size.width.0 - 0.5
        && outer.origin.y.0 + outer.size.height.0 >= inner.origin.y.0 + inner.size.height.0 - 0.5
}

/// How `rect` reads in a report.
fn wrote(rect: &Rect<DevicePx, Device>) -> String {
    format!(
        "{:.0},{:.0} {:.0}x{:.0}",
        rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
    )
}
