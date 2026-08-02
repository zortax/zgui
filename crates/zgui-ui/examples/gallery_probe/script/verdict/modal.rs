//! A dialog inside the gallery's theme provider: where its panel sits, what the scrim covers, and
//! where a list opened from inside it goes.
//!
//! The panel is centred by pulling itself back half its own width and half its own height, which is a
//! transform and therefore paint-time: the box the layout produced is at the middle of the window and
//! the panel a person sees is half its size up and to the left of that. So the box asked for here is
//! the one mapped through the transforms above it, which is the space a pointer is resolved in, and
//! the capture beside it is what settles the question either way.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;
use crate::script::verdict;
use crate::stage::Stage;

/// What the trigger says.
const TRIGGER: &str = "Rename…";

/// What the panel begins with once it is up.
const TITLE: &str = "Rename project";

/// What the control in the footer says, which the panel also has to hold.
const FOOTER: &str = "Cancel";

/// What the select's trigger inside the dialog says.
const CHOSEN: &str = "Pound sterling";

/// Everything the list says once it is open, which is how the list itself is told from its trigger.
const LIST: &str = "Pound sterlingEuroUS dollar";

/// How far off the middle of the window the panel's own middle may be, in device pixels.
///
/// One pixel each way for a panel whose width is odd, and nothing more: half of a box is not a whole
/// number and the middle of the window need not be either.
const OFF_CENTRE: f32 = 2.0;

/// Opens the dialog and measures it.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dialog") else {
        stage
            .report
            .note("Dialog", "the Dialog panel is not laid out");
        return;
    };
    let Some(at) = find::at_in(&census, panel, TRIGGER) else {
        stage.report.note("Dialog", "no trigger to open it with");
        return;
    };
    let window = stage.window();
    shot_of(stage, "vd-modal-before", window);
    stage.click(at);
    stage.leave();

    let census = stage.census();
    let surface = census
        .nodes
        .iter()
        .filter(|node| {
            node.area() > 0.0
                && !census.on_the_page(node)
                && node.text.starts_with(TITLE)
                && node.text.contains(FOOTER)
        })
        // The smallest of them, because that is the panel itself: the boxes above it on the band hold
        // the panel and the scrim together and are therefore the size of the window, and one of those
        // measured instead would report a dialog centred whatever the panel did.
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect);
    let Some(surface) = surface else {
        stage
            .report
            .note("Dialog", &stage.presence(TITLE).to_string());
        return;
    };
    find::mark(stage, "dialog:panel", surface);
    let middle = (
        surface.origin.x.0 + surface.size.width.0 / 2.0,
        surface.origin.y.0 + surface.size.height.0 / 2.0,
    );
    // The viewport rather than the surface, because that is what a fixed box's percentages are of: a
    // page keeping a gutter for its scrollbar has a viewport narrower than its window, and half of
    // each is a different pixel.
    let root = stage.handles().root();
    let viewport = stage.handles().host.scroll_position(root).scrollport;
    let want = (viewport.width.0 / 2.0, viewport.height.0 / 2.0);
    stage.report.check(
        "Dialog",
        "the panel's middle is the middle of the viewport",
        (middle.0 - want.0).abs() <= OFF_CENTRE && (middle.1 - want.1).abs() <= OFF_CENTRE,
        &format!(
            "the panel is at {} so its middle is {:.1},{:.1}; the viewport is {:.0}x{:.0} and its \
             middle {:.1},{:.1}; the window is {:.0}x{:.0}",
            wrote(&surface),
            middle.0,
            middle.1,
            viewport.width.0,
            viewport.height.0,
            want.0,
            want.1,
            window.size.width.0,
            window.size.height.0
        ),
    );

    // The scrim: a fixed box the size of the window, behind the panel. What "a gap at the edge" would
    // be is this box falling short of one of them.
    let scrim = census
        .nodes
        .iter()
        .filter(|node| node.text.is_empty() && !census.on_the_page(node))
        .filter_map(|node| node.rect)
        .filter(|rect| rect.size.width.0 > window.size.width.0 * 0.8)
        .max_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        });
    match scrim {
        Some(scrim) => {
            find::mark(stage, "dialog:scrim", scrim);
            // The window and not the viewport, which is the whole of the reported gap: a page keeping
            // a gutter for its scrollbar has a viewport fifteen pixels short of its window on each
            // scrolling axis, and a scrim that stopped at the viewport would leave that strip lit.
            stage.report.check(
                "Dialog",
                "the scrim covers the whole window",
                scrim.origin.x.0 <= 0.5
                    && scrim.origin.y.0 <= 0.5
                    && scrim.origin.x.0 + scrim.size.width.0 >= window.size.width.0 - 0.5
                    && scrim.origin.y.0 + scrim.size.height.0 >= window.size.height.0 - 0.5,
                &format!(
                    "the scrim is {}, the viewport {:.0}x{:.0} and the window {}",
                    wrote(&scrim),
                    viewport.width.0,
                    viewport.height.0,
                    wrote(&window)
                ),
            );
        }
        None => stage.report.note(
            "Dialog",
            "no box behind the panel is the size of the window",
        ),
    }
    shot_of(stage, "vd-modal-open", window);

    list_inside(stage, surface);

    // Out again the way a person would: the first Escape closes the list, the second the dialog.
    stage.key(NamedKey::Escape);
    stage.key(NamedKey::Escape);
    stage.settle(20);
    stage.report.check(
        "Dialog",
        "it closes again",
        !stage.floating(TITLE),
        &stage.presence(TITLE),
    );
}

/// The select inside the dialog, opened, and where its list went.
fn list_inside(stage: &mut Stage<'_>, surface: Rect<DevicePx, Device>) {
    let census = stage.census();
    let trigger = verdict::one_per_place(&census, CHOSEN)
        .into_iter()
        .filter_map(|node| node.rect)
        .filter(|rect| {
            rect.origin.y.0 >= surface.origin.y.0 - 0.5
                && rect.origin.y.0 + rect.size.height.0
                    <= surface.origin.y.0 + surface.size.height.0 + 0.5
        })
        .max_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        });
    let Some(trigger) = trigger else {
        stage
            .report
            .note("Select", "the select inside the dialog is not laid out");
        return;
    };
    find::mark(stage, "select:trigger", trigger);
    stage.click(Point::new(
        DevicePx(trigger.origin.x.0 + trigger.size.width.0 / 2.0),
        DevicePx(trigger.origin.y.0 + trigger.size.height.0 / 2.0),
    ));

    let census = stage.census();
    let list = census
        .nodes
        .iter()
        .filter(|node| node.text == LIST && node.area() > 0.0 && !census.on_the_page(node))
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect);
    let Some(list) = list else {
        stage.report.note("Select", &stage.presence(LIST));
        return;
    };
    find::mark(stage, "select:list", list);
    // Under it, and lined up with it: a list placed in the window's own coordinates rather than in the
    // dialog's would be off by half the panel, which is the fault this is looking for.
    stage.report.check(
        "Select",
        "the list opens lined up with its trigger",
        (list.origin.x.0 - trigger.origin.x.0).abs() <= 8.0,
        &format!(
            "the trigger is at {} and the list at {}",
            wrote(&trigger),
            wrote(&list)
        ),
    );
    stage.report.check(
        "Select",
        "the list opens beside its trigger rather than across the window",
        list.origin.y.0 >= trigger.origin.y.0 - trigger.size.height.0
            && list.origin.y.0 <= trigger.origin.y.0 + trigger.size.height.0 * 3.0,
        &format!(
            "the trigger's top edge is {:.0} and the list's is {:.0}",
            trigger.origin.y.0, list.origin.y.0
        ),
    );
    let window = stage.window();
    shot_of(stage, "vd-modal-list", window);
}

/// How `rect` reads in a report.
fn wrote(rect: &Rect<DevicePx, Device>) -> String {
    format!(
        "{:.0},{:.0} {:.0}x{:.0}",
        rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
    )
}
