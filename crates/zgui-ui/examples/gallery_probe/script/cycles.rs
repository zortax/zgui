//! Opening and closing a modal surface again and again, pressing the page after every one.
//!
//! The overlays section opens each surface once. Once is not enough, and the reason is specific:
//! what a modal installs — a focus trap, an entry on the dismissable stack, a hold on the scroll
//! lock, a set of listeners — is put up on the way in and taken down on the way out, and a
//! teardown that misses one of them leaves a window that looks completely normal. It hovers, it
//! scrolls, it paints. It answers no press and no key.
//!
//! The first cycle is also not where such a fault shows. The first pass is the one that installs
//! whatever is left behind; the second is the one that meets it. So each surface is driven ten
//! times, and after every single cycle something ordinary on the page is pressed and required to
//! answer — which is the one claim a window with a leftover trap cannot satisfy.

use core::time::Duration;

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// How long a surface is given to arrive or to leave.
///
/// Real time rather than a count of frames, because what is being waited for is an animation, and
/// a sheet sliding in from an edge is paced by a clock rather than by how fast this loop can ask
/// for frames. A step that counted frames instead would read the document halfway through a slide
/// and call a surface that is on its way out one that never went.
const MOVED: Duration = Duration::from_millis(400);

/// How many times each surface is opened and closed.
///
/// Ten rather than two. Two is the fewest that catches a hold taken twice and given back once, and
/// a run that stopped there would pass for anything that accumulates more slowly.
const CYCLES: u32 = 10;

/// The panel holding the control that proves the page still takes a pointer.
const PROBE_PANEL: &str = "Tabs";

/// The tab pressed to move the page off what it is showing.
const PROBE_AWAY: &str = "Billing";

/// What that tab shows.
const PROBE_AWAY_ANSWER: &str = "Cards, invoices and the plan you are on.";

/// The tab pressed to put it back.
const PROBE_BACK: &str = "Profile";

/// What that one shows.
const PROBE_BACK_ANSWER: &str = "Your name, your picture and how to reach you.";

/// How a surface is closed, because the four paths out of one are four different pieces of code.
#[derive(Copy, Clone)]
enum Close {
    /// The Escape key.
    Escape,
    /// A control on the surface itself, named by what it says.
    Control(&'static str),
    /// The dimmed window behind it.
    Scrim,
}

impl Close {
    /// How this is written in the report.
    const fn name(self) -> &'static str {
        match self {
            Self::Escape => "escape",
            Self::Control(_) => "its own button",
            Self::Scrim => "the scrim",
        }
    }
}

/// Drives every modal surface through [`CYCLES`] open/close cycles.
pub(crate) fn run(stage: &mut Stage<'_>) {
    // The dialog, out by each of its three exits.
    cycles(stage, "Dialog", "Rename…", "Rename project", Close::Escape);
    cycles(
        stage,
        "Dialog",
        "Rename…",
        "Rename project",
        Close::Control("Cancel"),
    );
    cycles(stage, "Dialog", "Rename…", "Rename project", Close::Scrim);
    // The alert dialog, which a press on the scrim deliberately does not close, so the way out is
    // the answer it is asking for.
    cycles(
        stage,
        "Dialog",
        "Delete",
        "Delete this project?",
        Close::Control("Keep it"),
    );
    cycles(
        stage,
        "Sheet and drawer",
        "Details",
        "Invoice 4471",
        Close::Escape,
    );
    cycles(
        stage,
        "Sheet and drawer",
        "Share",
        "Share this invoice",
        Close::Control("Done"),
    );
}

/// Opens the surface `trigger` opens and closes it again, [`CYCLES`] times, checking the page
/// after each.
fn cycles(stage: &mut Stage<'_>, panel: &str, trigger: &str, title: &str, close: Close) {
    let subject = format!("{title} by {}", close.name());
    let mut opened = 0;
    let mut closed = 0;
    let mut answered = 0;
    for round in 1..=CYCLES {
        if !open(stage, panel, trigger, title) {
            break;
        }
        opened += 1;
        if !dismiss(stage, title, close) {
            break;
        }
        closed += 1;
        if !page_answers(stage) {
            stage.shot(&format!("cycles-{trigger}-{round}-deaf"));
            break;
        }
        answered += 1;
        // Every cycle, not only the last: the picture that matters is the one showing the page
        // still working after the *n*th, and which n goes wrong is the finding.
        if matches!(close, Close::Escape) && panel == "Dialog" {
            stage.shot(&format!("cycles-dialog-escape-{round:02}-answered"));
        }
    }
    stage.report.check(
        "ModalCycles",
        &format!("{subject}: the page answers after every one of {CYCLES} cycles"),
        answered == CYCLES,
        &format!(
            "opened {opened}, closed {closed}, and the page answered {answered} times out of \
             {CYCLES}"
        ),
    );
}

/// Opens the surface, and answers whether it came up.
fn open(stage: &mut Stage<'_>, panel: &str, trigger: &str, title: &str) -> bool {
    let Some((census, rect)) = find::open_panel(stage, panel) else {
        stage
            .report
            .note("ModalCycles", "the panel is not laid out");
        return false;
    };
    let Some(at) = find::at_in(&census, rect, trigger) else {
        stage
            .report
            .note("ModalCycles", &format!("nothing says {trigger}"));
        return false;
    };
    stage.click(at);
    stage.settle(10);
    stage.wait(MOVED);
    drawn(stage, title)
}

/// Closes it the way this run was asked to, and answers whether it went.
fn dismiss(stage: &mut Stage<'_>, title: &str, close: Close) -> bool {
    match close {
        Close::Escape => stage.key(NamedKey::Escape),
        Close::Scrim => {
            // A corner of the window. Nothing of the page is reachable there while a modal is
            // open, because the scrim is over all of it — which is the whole of what makes this a
            // press *outside* the surface.
            stage.click(zgui::geom::Point::new(
                zgui::geom::DevicePx(8.0),
                zgui::geom::DevicePx(8.0),
            ));
        }
        Close::Control(label) => {
            let census = stage.census();
            let Some(at) = census.innermost(label).and_then(|node| node.centre()) else {
                stage
                    .report
                    .note("ModalCycles", &format!("the surface has no {label}"));
                return false;
            };
            stage.click(at);
        }
    }
    stage.settle(10);
    stage.wait(MOVED);
    !drawn(stage, title)
}

/// Moves the page's own tab strip to another tab and back, and answers whether it followed.
///
/// A control far from every overlay whose answer is a change in the layout — one panel goes and
/// another arrives — rather than a click into space, which nothing could have contradicted. It is
/// pressed twice so the page it leaves behind is the page the next cycle starts from.
fn page_answers(stage: &mut Stage<'_>) -> bool {
    let Some((census, rect)) = find::open_panel(stage, PROBE_PANEL) else {
        return false;
    };
    let (Some(away), Some(back)) = (
        find::at_in(&census, rect, PROBE_AWAY),
        find::at_in(&census, rect, PROBE_BACK),
    ) else {
        return false;
    };
    stage.click(away);
    stage.settle(8);
    let moved = drawn(stage, PROBE_AWAY_ANSWER) && !drawn(stage, PROBE_BACK_ANSWER);
    stage.click(back);
    stage.settle(8);
    let returned = drawn(stage, PROBE_BACK_ANSWER) && !drawn(stage, PROBE_AWAY_ANSWER);
    moved && returned
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}
