//! A dropdown opened inside a modal, and the two of them taken away in the right order.
//!
//! The layered case, ten times over. Two surfaces are on the dismissable stack at once, and what
//! has to survive is not only that Escape takes the list before the dialog but that both of them
//! give back everything they took — the trap, the stack entry, the hold on the page's scrolling.
//! Half the cycles leave by choosing an item and pressing the dialog's own button instead, which
//! is the other path through the same teardown.

use core::cell::{Cell, RefCell};
use core::time::Duration;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::answer;
use crate::stage::Stage;

/// How long a surface is given to arrive or to leave.
const MOVED: Duration = Duration::from_millis(420);

/// How many times the pair is opened and closed.
const CYCLES: usize = 10;

/// How many turns of the loop this part needs.
pub(crate) const STEPS: usize = CYCLES;

/// The panel the dialog's trigger is in.
const PANEL: &str = "Dialog";

/// What that trigger says.
const TRIGGER: &str = "Rename…";

/// What the dialog says once it is up.
const TITLE: &str = "Rename project";

/// The dialog's own way out.
const CANCEL: &str = "Cancel";

/// What the select's trigger can be showing, since choosing from it changes what it says.
const CURRENCIES: [&str; 4] = ["Pound sterling", "Euro", "US dollar", "Choose one"];

/// The item that is chosen on the cycles that answer the list rather than dismissing it.
const ITEM: &str = "Euro";

/// An item that is never chosen here, so that the list coming up is the only thing that can put
/// another box saying it on the page.
///
/// How many boxes say it, rather than whether any does: the page has a select of its own with the
/// same list in it, and an earlier part of the run may well have left that one showing this very
/// word. Asked whether anything says it, such a page answers yes with no list open anywhere.
const OTHER: &str = "US dollar";

/// What the pair has managed so far.
#[derive(Clone, Copy, Default)]
struct Tally {
    /// How many times the dialog came up.
    opened: usize,
    /// How many times the list came up inside it.
    listed: usize,
    /// How many times the first Escape took the list and left the dialog.
    ordered: usize,
    /// How many times both were gone at the end of the cycle.
    cleared: usize,
    /// How many times the page took a pointer and a key afterwards.
    answered: usize,
    /// The first cycle after which it did not.
    deaf: Option<usize>,
}

thread_local! {
    /// Which cycle runs next.
    static CURSOR: Cell<usize> = const { Cell::new(0) };
    /// What the pair has managed so far.
    static TALLY: RefCell<Tally> = const { RefCell::new(Tally {
        opened: 0,
        listed: 0,
        ordered: 0,
        cleared: 0,
        answered: 0,
        deaf: None,
    }) };
}

/// Runs one cycle of the pair, and reports once the last one is done.
pub(crate) fn chunk(stage: &mut Stage<'_>) {
    let round = CURSOR.with(|cursor| {
        let at = cursor.get();
        cursor.set(at + 1);
        at
    });
    if round >= CYCLES {
        return;
    }
    let tag = format!("nested-{:02}", round + 1);
    let mut tally = TALLY.with(|tally| *tally.borrow());
    // What the page says with nothing open, which every later count is measured against.
    let quiet = saying(stage, OTHER);

    if open_dialog(stage) {
        tally.opened += 1;
        if let Some(trigger) = open_list(stage, quiet) {
            tally.listed += 1;
            // Alternating, because the two ways out of a list are different code: the key that
            // dismisses it, and a choice that closes it by answering it.
            if round.is_multiple_of(2) {
                stage.key(NamedKey::Escape);
                stage.settle(8);
                if saying(stage, OTHER) == quiet && drawn(stage, TITLE) {
                    tally.ordered += 1;
                }
            } else {
                choose(stage, trigger);
            }
        }
        shut_dialog(stage, round);
    }
    stage.wait(MOVED);
    if !drawn(stage, TITLE) && saying(stage, OTHER) == quiet {
        tally.cleared += 1;
    }
    let reply = answer::press_and_type(stage, &tag);
    if reply.pointer && reply.key {
        tally.answered += 1;
    } else if tally.deaf.is_none() {
        tally.deaf = Some(round + 1);
        stage.shot(&format!("gx-{tag}-deaf"));
    }
    TALLY.with(|held| *held.borrow_mut() = tally);
    if round + 1 == CYCLES {
        report(stage, tally);
    }
}

/// Writes down how the pair did.
fn report(stage: &mut Stage<'_>, tally: Tally) {
    let detail = format!(
        "of {CYCLES}: the dialog opened {} times, the list opened inside it {} times, Escape took \
         the list and not the dialog {} times, both were gone afterwards {} times; first \
         unanswered cycle {:?}",
        tally.opened, tally.listed, tally.ordered, tally.cleared, tally.deaf
    );
    stage.report.check(
        "Gauntlet",
        "nested: a dropdown inside a dialog opens and both close",
        tally.opened == CYCLES && tally.listed == CYCLES && tally.cleared == CYCLES,
        &detail,
    );
    stage.report.check(
        "Gauntlet",
        "nested: the page answers after every cycle",
        tally.answered == CYCLES,
        &detail,
    );
}

/// Opens the dialog and answers whether it came up.
fn open_dialog(stage: &mut Stage<'_>) -> bool {
    let Some((census, panel)) = find::open_panel(stage, PANEL) else {
        return false;
    };
    let Some(at) = find::at_in(&census, panel, TRIGGER) else {
        return false;
    };
    stage.click(at);
    stage.settle(10);
    stage.wait(MOVED);
    drawn(stage, TITLE)
}

/// Opens the select that is inside the dialog, and answers with its trigger's box when the list
/// came up.
///
/// Inside the dialog, and that qualification is the whole of whether this lands on anything: the
/// page has a second select with the same currencies in it, whose trigger says the same words.
fn open_list(stage: &mut Stage<'_>, quiet: usize) -> Option<Rect<DevicePx, Device>> {
    let census = stage.census();
    let root = census
        .nodes
        .iter()
        .filter(|node| node.text.contains(TITLE) && node.text.contains(CANCEL))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .map(|node| node.id)?;
    let trigger = census
        .nodes
        .iter()
        .filter(|node| CURRENCIES.contains(&node.text.as_str()) && node.area() > 0.0)
        .filter(|node| stage.handles().host.contains(root, node.id))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)?;
    let at = zgui::geom::Point::new(
        DevicePx(trigger.origin.x.0 + trigger.size.width.0 / 2.0),
        DevicePx(trigger.origin.y.0 + trigger.size.height.0 / 2.0),
    );
    stage.click(at);
    stage.settle(10);
    (saying(stage, OTHER) > quiet).then_some(trigger)
}

/// Chooses an item from the open list.
///
/// Anything but the trigger, which says the same word as the item that is already chosen and sits
/// under the list saying it. Aiming at the smallest box with those words in it would be a coin
/// toss between choosing from the list and shutting it again.
fn choose(stage: &mut Stage<'_>, trigger: Rect<DevicePx, Device>) {
    let census = stage.census();
    let item = census
        .nodes
        .iter()
        .filter(|node| node.text == ITEM && node.area() > 0.0)
        .filter(|node| {
            node.rect
                .is_some_and(|rect| (rect.origin.y.0 - trigger.origin.y.0).abs() > 2.0)
        })
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.centre());
    if let Some(at) = item {
        stage.click(at);
        stage.settle(8);
    }
}

/// Closes the dialog, by Escape on even cycles and by its own button on odd ones.
fn shut_dialog(stage: &mut Stage<'_>, round: usize) {
    if round.is_multiple_of(2) {
        stage.key(NamedKey::Escape);
    } else {
        let census = stage.census();
        if let Some(at) = census.innermost(CANCEL).and_then(|node| node.centre()) {
            stage.click(at);
        }
    }
    stage.settle(10);
    stage.wait(MOVED);
}

/// How many boxes say exactly `text`.
fn saying(stage: &Stage<'_>, text: &str) -> usize {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .count()
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}
