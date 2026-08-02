//! The surfaces that float above the page or take it over.
//!
//! Two things are being asked of every one of them, and the second is the one that bites. The
//! first is that it opens. The second is that when it closes it leaves *nothing* behind — because
//! a scrim that stays mounted is invisible, is over the whole window, and turns every later click
//! in the run into a click on nothing. So each dismissal is followed by a click on something
//! ordinary, and the claim is that the ordinary thing still answers.

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// Drives the overlays.
pub(crate) fn run(stage: &mut Stage<'_>) {
    dialog(stage);
    alert_dialog(stage);
    sheet(stage);
    drawer(stage);
    popover(stage);
    tooltip_and_hover_card(stage);
    scroll_lock(stage);
}

/// What a modal surface does to the page under it.
///
/// The claim is that it does nothing at all. Opening one stops the page scrolling, and the obvious
/// way to stop a page scrolling — restyling its root so that it is no longer a scroll container —
/// takes the offset with it: the page snaps to the top under the surface and snaps back when it
/// closes. So the reading taken here is *where the page is*, before, during and after, and the
/// three pictures beside it are of the same page in the same place.
fn scroll_lock(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dialog") else {
        stage
            .report
            .note("ScrollLock", "the Dialog panel is not laid out");
        return;
    };
    // Where the page is, read off something ordinary in it rather than off the scroll offset: the
    // offset is what was never wrong, and the page moved anyway.
    let landmark = |stage: &Stage<'_>| -> Option<f32> {
        stage
            .census()
            .nodes
            .iter()
            .filter(|node| node.text == "Dialog" && node.area() > 0.0)
            .filter_map(|node| node.rect)
            .map(|rect| rect.origin.y.0)
            .next()
    };
    let Some(trigger) = find::at_in(&census, panel, "Rename…") else {
        stage
            .report
            .note("ScrollLock", "no trigger to open the dialog with");
        return;
    };
    let before = landmark(stage);
    stage.shot("overlays-locked-page");

    stage.click(trigger);
    stage.settle(8);
    let opened = landmark(stage);
    stage.report.check(
        "ScrollLock",
        "opening a modal surface leaves the page where it was",
        opened == before,
        &format!("the page's own heading was at {before:?} and is at {opened:?}"),
    );
    stage.shot("overlays-locked-open");

    // The page may not be scrolled behind it, by any of the three ways in.
    stage.wheel((0.0, 6.0));
    stage.settle(8);
    stage.trackpad(zgui::geom::Size::new(
        zgui::geom::CssPx(0.0),
        zgui::geom::CssPx(-240.0),
    ));
    stage.settle(8);
    stage.key(NamedKey::PageDown);
    stage.settle(8);
    let held = landmark(stage);
    stage.report.check(
        "ScrollLock",
        "the page cannot be scrolled behind it",
        held == before,
        &format!("a wheel, a trackpad and a key left the heading at {held:?}, from {before:?}"),
    );

    stage.key(NamedKey::Escape);
    stage.settle(8);
    let after = landmark(stage);
    stage.report.check(
        "ScrollLock",
        "closing it leaves the page where it was too",
        after == before,
        &format!("the page's own heading was at {before:?} and is at {after:?}"),
    );
    stage.shot("overlays-locked-closed");
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}

/// Whether the page still answers a pointer, which is what a leaked scrim would stop.
///
/// The Collapsible is used because it is far from every overlay in the document, folds and unfolds
/// with one click, and says so in the layout — so this is a real interaction with a visible
/// answer, not a click into space that nothing could have contradicted.
fn page_still_answers(stage: &mut Stage<'_>, after: &str) {
    let Some((census, panel)) = find::open_panel(stage, "Collapsible") else {
        stage
            .report
            .note("Overlay", "no Collapsible panel to test the page with");
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Delivery details") else {
        stage.report.note("Overlay", "no Collapsible trigger");
        return;
    };
    let before = drawn(stage, "Arrives Thursday, signed for.");
    stage.click(trigger);
    stage.settle(8);
    let toggled = drawn(stage, "Arrives Thursday, signed for.") != before;
    stage.report.check(
        "Overlay",
        &format!("the page still takes a pointer after {after}"),
        toggled,
        if toggled {
            "a click on the page's own collapsible still folds it"
        } else {
            "a click on the page's collapsible did nothing, so something is over it"
        },
    );
    // Put it back the way it was.
    stage.click(trigger);
    stage.settle(8);
}

/// The modal dialog, the select inside it, and the order Escape closes them in.
fn dialog(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dialog") else {
        stage.report.note("Dialog", "the panel is not laid out");
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Rename…") else {
        stage.report.note("Dialog", "no trigger");
        return;
    };

    stage.click(trigger);
    stage.settle(8);
    stage.report.check(
        "Dialog",
        "the trigger opens it",
        drawn(stage, "Rename project"),
        "the dialog's title is laid out",
    );
    stage.shot("overlays-dialog-open");

    // Tab has to stay inside a modal. Walking further than there are controls in the dialog and
    // still being inside it is the claim; walking once and finding *a* control is not.
    let inside = stage.census().control("Rename project").map(|node| node.id);
    let dialog_root = stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text.contains("Rename project") && node.text.contains("Cancel"))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .map(|node| node.id);
    let mut escaped = 0;
    let mut visited = Vec::new();
    for _ in 0..12 {
        stage.key(NamedKey::Tab);
        let now = stage.focused();
        visited.push(stage.focused_text());
        let contained = match (dialog_root, now) {
            (Some(root), Some(node)) => stage.handles().host.contains(root, node),
            _ => false,
        };
        if !contained {
            escaped += 1;
        }
    }
    stage.report.check(
        "Dialog",
        "Tab is trapped inside the modal",
        escaped == 0,
        &format!("12 tabs left the dialog {escaped} times; they landed on {visited:?}"),
    );
    let _ = inside;
    stage.shot("overlays-dialog-tabbed");

    // The layering case: a select open inside a dialog. Escape must take the list first.
    //
    // Inside the dialog, and that qualification is the whole of whether this step lands on
    // anything: the page has a second select with the same currencies in it, its trigger says the
    // same words, and its label is laid out to within a pixel of the same size. Asked for the
    // smallest thing on the page saying "Pound sterling", the answer is a coin toss decided by
    // text metrics — and half the time it is a control scrolled far off the side of the window,
    // where a click reaches nothing and the dialog's own select is reported as not opening.
    let census = stage.census();
    let select = census
        .nodes
        .iter()
        .filter(|node| node.text == "Pound sterling" && node.area() > 0.0)
        .filter(|node| dialog_root.is_some_and(|root| stage.handles().host.contains(root, node.id)))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.centre());
    if let Some(select) = select {
        stage.click(select);
        stage.settle(8);
        let list_open = drawn(stage, "Euro");
        stage.report.check(
            "Select",
            "it opens inside a dialog",
            list_open,
            "the list's other items are laid out",
        );
        stage.shot("overlays-dialog-select-open");

        stage.key(NamedKey::Escape);
        stage.settle(8);
        let list_gone = !drawn(stage, "Euro");
        let dialog_still = drawn(stage, "Rename project");
        stage.report.check(
            "Select",
            "Escape closes the list and not the dialog",
            list_gone && dialog_still,
            &format!(
                "after one Escape the list is {} and the dialog is {}",
                if list_gone { "closed" } else { "still open" },
                if dialog_still {
                    "still open"
                } else {
                    "closed too"
                }
            ),
        );
        stage.shot("overlays-dialog-escape-once");
    }

    stage.key(NamedKey::Escape);
    stage.settle(8);
    stage.report.check(
        "Dialog",
        "the second Escape closes the dialog",
        !drawn(stage, "Rename project"),
        "the dialog's title produces no box",
    );
    stage.shot("overlays-dialog-closed");
    page_still_answers(stage, "a dialog closed with Escape");

    // And again, closed by its own button rather than by a key, because the two paths are
    // different code and only one of them was ever suspected.
    let census = stage.census();
    if let Some(trigger) = find::at_in(&census, panel, "Rename…") {
        stage.click(trigger);
        stage.settle(8);
        let census = stage.census();
        if let Some(cancel) = census.innermost("Cancel").and_then(|node| node.centre()) {
            stage.click(cancel);
            stage.settle(8);
            stage.report.check(
                "Dialog",
                "its own Cancel closes it",
                !drawn(stage, "Rename project"),
                "the dialog's title produces no box",
            );
            page_still_answers(stage, "a dialog closed with its own button");
        }
    }
}

/// The destructive dialog, which is the same shape with a different job.
fn alert_dialog(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dialog") else {
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Delete") else {
        stage.report.note("AlertDialog", "no Delete trigger");
        return;
    };
    stage.click(trigger);
    stage.settle(8);
    stage.report.check(
        "AlertDialog",
        "it opens",
        drawn(stage, "Delete this project?"),
        "the title is laid out",
    );
    stage.shot("overlays-alert-dialog");
    let census = stage.census();
    if let Some(keep) = census.innermost("Keep it").and_then(|node| node.centre()) {
        stage.click(keep);
        stage.settle(8);
        stage.report.check(
            "AlertDialog",
            "cancelling closes it",
            !drawn(stage, "Delete this project?"),
            "the title produces no box",
        );
    }
    page_still_answers(stage, "an alert dialog");
}

/// The sheet, which comes in from an edge.
fn sheet(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Sheet and drawer") else {
        stage.report.note("Sheet", "the panel is not laid out");
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Details") else {
        stage.report.note("Sheet", "no trigger");
        return;
    };
    stage.click(trigger);
    stage.settle(10);
    stage.report.check(
        "Sheet",
        "it opens",
        drawn(stage, "Invoice 4471"),
        "the sheet's title is laid out",
    );
    // It comes in from the right, so it has to be at the right — of *this* window, whatever size
    // the desktop gave it. A number written down here instead is a number that is right on the
    // window it was measured on and wrong on every other, and a sheet correctly against the right
    // edge of a wide window is then reported as having arrived on the wrong side.
    let census = stage.census();
    let window = stage.window();
    let title = census.control("Invoice 4471").and_then(|node| node.rect);
    let at_right =
        title.is_some_and(|rect| rect.origin.x.0 > window.origin.x.0 + window.size.width.0 / 2.0);
    stage.report.check(
        "Sheet",
        "it arrives on the side it was asked for",
        at_right,
        &format!(
            "its title is at x {:?}, in a window {} wide",
            title.map(|rect| rect.origin.x.0),
            window.size.width.0
        ),
    );
    stage.shot("overlays-sheet-open");
    stage.key(NamedKey::Escape);
    stage.settle(10);
    stage.report.check(
        "Sheet",
        "Escape closes it",
        !drawn(stage, "Invoice 4471"),
        "the title produces no box",
    );
    page_still_answers(stage, "a sheet");
}

/// The drawer, which comes up from the bottom.
fn drawer(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Sheet and drawer") else {
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Share") else {
        stage.report.note("Drawer", "no trigger");
        return;
    };
    stage.click(trigger);
    stage.settle(10);
    stage.report.check(
        "Drawer",
        "it opens",
        drawn(stage, "Share this invoice"),
        "the drawer's title is laid out",
    );
    stage.shot("overlays-drawer-open");
    let census = stage.census();
    if let Some(done) = census.innermost("Done").and_then(|node| node.centre()) {
        stage.click(done);
        stage.settle(10);
        stage.report.check(
            "Drawer",
            "its own button closes it",
            !drawn(stage, "Share this invoice"),
            "the title produces no box",
        );
    }
    page_still_answers(stage, "a drawer");
}

/// The popover, which is beside its trigger rather than over the page.
fn popover(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Popover, tooltip, hover card") else {
        stage.report.note("Popover", "the panel is not laid out");
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Size") else {
        stage.report.note("Popover", "no trigger");
        return;
    };
    let trigger_rect = census.control("Size").and_then(|node| node.rect);
    stage.click(trigger);
    stage.settle(8);
    stage.report.check(
        "Popover",
        "it opens",
        drawn(stage, "Width"),
        "the popover's label is laid out",
    );
    // Beside, not over: the surface has to be below the trigger and aligned to its start.
    let census = stage.census();
    let placed = census.control("Width").and_then(|node| node.rect);
    let below = placed
        .zip(trigger_rect)
        .is_some_and(|(surface, trigger)| surface.origin.y.0 >= trigger.origin.y.0);
    stage.report.check(
        "Popover",
        "it is placed where it was asked to be",
        below,
        &format!("the trigger is at {trigger_rect:?} and the surface at {placed:?}"),
    );
    stage.shot("overlays-popover-open");

    // A click outside has to dismiss it, and has to be the only thing that click does.
    let elsewhere = find::left_edge(panel, 8.0, 0.02);
    stage.click(elsewhere);
    stage.settle(8);
    stage.report.check(
        "Popover",
        "a click outside dismisses it",
        !drawn(stage, "Width"),
        "the label produces no box",
    );
    page_still_answers(stage, "a popover");
}

/// The tooltip and the hover card, which open on a pointer resting rather than on a click.
fn tooltip_and_hover_card(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Popover, tooltip, hover card") else {
        return;
    };
    if let Some(bold) = find::at_in(&census, panel, "B") {
        stage.move_to(bold);
        // The tooltip was given a delay of three hundred milliseconds, so it must not be there at
        // once and must be there after.
        let at_once = drawn(stage, "Bold");
        stage.wait(core::time::Duration::from_millis(700));
        let after = drawn(stage, "Bold");
        stage.report.check(
            "Tooltip",
            "it waits for its delay and then opens",
            !at_once && after,
            &format!("after the move it was {at_once}, and after 700ms {after}"),
        );
        stage.shot("overlays-tooltip");
        stage.leave();
        // Real time, not frames. A tooltip closes on a timer of its own, and frames run inside one
        // turn of the loop take no time off a timer — so a tooltip asked whether it has closed
        // immediately after the pointer left is being asked before it was ever going to, and the
        // answer is the same whether it would have closed a moment later or never at all.
        stage.wait(core::time::Duration::from_millis(600));
        // Of the *surface*: the page has a panel of styled text with a sample that says Bold too,
        // so a tooltip that closed perfectly well reads as one that stayed up for as long as
        // anything else in the window happens to use its word.
        stage.report.check(
            "Tooltip",
            "it closes when the pointer goes",
            !stage.floating("Bold"),
            &format!(
                "once the pointer has left, a surface saying Bold is {}",
                if stage.floating("Bold") {
                    "still up"
                } else {
                    "gone"
                }
            ),
        );
    }

    let census = stage.census();
    if let Some(handle) = find::at_in(&census, panel, "@ada") {
        stage.move_to(handle);
        stage.wait(core::time::Duration::from_millis(900));
        stage.report.check(
            "HoverCard",
            "resting on the trigger opens it",
            drawn(stage, "Joined December 1842"),
            "the card's text is laid out",
        );
        stage.shot("overlays-hover-card");
        stage.leave();
        stage.wait(core::time::Duration::from_millis(600));
        stage.report.check(
            "HoverCard",
            "it closes when the pointer goes",
            !drawn(stage, "Joined December 1842"),
            "the card produces no box once the pointer has left",
        );
    }
    page_still_answers(stage, "a tooltip and a hover card");
}
