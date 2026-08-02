//! Every modal surface in the gallery, opened and shut ten times, by all the ways out it has.
//!
//! One cycle per turn of the loop, so that the run through all of them is a run the compositor
//! stays in touch with. What is counted is four things per cycle: that the surface came up, that
//! it went away, that the page under it still takes a pointer afterwards, and that it still takes
//! a key. The cycle each of those first stops being true at is the finding.

use core::cell::{Cell, RefCell};
use core::time::Duration;

use zgui::geom::{DevicePx, Point};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::{answer, ink};
use crate::stage::Stage;

/// How long a surface is given to arrive or to leave.
///
/// Real time, because what is being waited for is an animation: a sheet sliding in from an edge is
/// paced by a clock rather than by how fast this loop can ask for frames.
const MOVED: Duration = Duration::from_millis(420);

/// How many times each surface is opened and closed.
const CYCLES: usize = 10;

/// Where a press lands that is on the dimmed window rather than on the surface.
const CORNER: f32 = 8.0;

/// A way out of a surface, because each is a different piece of code.
#[derive(Copy, Clone)]
enum Exit {
    /// The Escape key.
    Escape,
    /// A control on the surface itself, named by what it says.
    Control(&'static str),
    /// The dimmed window behind it.
    Scrim,
}

impl Exit {
    /// How this is written down.
    const fn name(self) -> &'static str {
        match self {
            Self::Escape => "escape",
            Self::Control(_) => "button",
            Self::Scrim => "scrim",
        }
    }
}

/// A modal surface, and everything needed to work it.
#[derive(Copy, Clone)]
struct Subject {
    /// What it is called in the report and in the file names.
    key: &'static str,
    /// The panel its trigger is in.
    panel: &'static str,
    /// What the trigger says.
    trigger: &'static str,
    /// What the surface itself says once it is up.
    title: &'static str,
    /// The ways out, taken in turn, cycle by cycle.
    exits: &'static [Exit],
}

/// The four surfaces that take the whole window over.
///
/// The alert dialog is the one with no scrim in its list, and that is the component's own
/// decision rather than an omission here: a press past it deliberately does not answer the
/// question it is asking.
const SUBJECTS: [Subject; 4] = [
    Subject {
        key: "dialog",
        panel: "Dialog",
        trigger: "Rename…",
        title: "Rename project",
        exits: &[Exit::Escape, Exit::Control("Cancel"), Exit::Scrim],
    },
    Subject {
        key: "alert",
        panel: "Dialog",
        trigger: "Delete",
        title: "Delete this project?",
        exits: &[Exit::Control("Keep it"), Exit::Escape],
    },
    Subject {
        key: "sheet",
        panel: "Sheet and drawer",
        trigger: "Details",
        title: "Invoice 4471",
        exits: &[Exit::Escape, Exit::Control("Close"), Exit::Scrim],
    },
    Subject {
        key: "drawer",
        panel: "Sheet and drawer",
        trigger: "Share",
        title: "Share this invoice",
        exits: &[Exit::Escape, Exit::Control("Done"), Exit::Scrim],
    },
];

/// How many turns of the loop this part needs, one cycle each.
pub(crate) const STEPS: usize = SUBJECTS.len() * CYCLES;

/// What one surface has managed so far.
#[derive(Clone, Copy, Default)]
struct Tally {
    /// How many times it came up.
    opened: usize,
    /// How many times it went away again.
    closed: usize,
    /// How many times the page took a pointer afterwards.
    pointer: usize,
    /// How many times the page took a key afterwards.
    key: usize,
    /// The first cycle after which the page answered neither.
    deaf: Option<usize>,
}

thread_local! {
    /// Which cycle runs next.
    static CURSOR: Cell<usize> = const { Cell::new(0) };
    /// What each surface has managed so far.
    static TALLIES: RefCell<[Tally; SUBJECTS.len()]> = const { RefCell::new([Tally {
        opened: 0,
        closed: 0,
        pointer: 0,
        key: 0,
        deaf: None,
    }; SUBJECTS.len()]) };
}

/// Runs one cycle of one surface, and reports on that surface once its last one is done.
pub(crate) fn chunk(stage: &mut Stage<'_>) {
    let step = CURSOR.with(|cursor| {
        let at = cursor.get();
        cursor.set(at + 1);
        at
    });
    if step >= STEPS {
        return;
    }
    let which = step / CYCLES;
    let round = step % CYCLES;
    let subject = SUBJECTS[which];
    let exit = subject.exits[round % subject.exits.len()];
    let tag = format!("{}-{:02}-{}", subject.key, round + 1, exit.name());

    let mut tally = TALLIES.with(|tallies| tallies.borrow()[which]);
    if open(stage, subject, &tag) {
        tally.opened += 1;
        if shut(stage, subject, exit, &tag) {
            tally.closed += 1;
        }
    }
    let reply = answer::press_and_type(stage, &tag);
    tally.pointer += usize::from(reply.pointer);
    tally.key += usize::from(reply.key);
    if !(reply.pointer && reply.key) && tally.deaf.is_none() {
        tally.deaf = Some(round + 1);
        stage.shot(&format!("gx-{tag}-deaf"));
    }
    // Whatever happened, the next cycle starts from a window with nothing over it.
    clear(stage, subject);
    TALLIES.with(|tallies| tallies.borrow_mut()[which] = tally);
    if round + 1 == CYCLES {
        report(stage, subject, tally);
    }
}

/// Writes down how one surface did.
fn report(stage: &mut Stage<'_>, subject: Subject, tally: Tally) {
    let detail = format!(
        "opened {} of {CYCLES}, closed {}, the page then took a pointer {} times and a key {} \
         times; first unanswered cycle {:?}",
        tally.opened, tally.closed, tally.pointer, tally.key, tally.deaf
    );
    stage.report.check(
        "Gauntlet",
        &format!("{}: it opens and shuts {CYCLES} times", subject.key),
        tally.opened == CYCLES && tally.closed == CYCLES,
        &detail,
    );
    stage.report.check(
        "Gauntlet",
        &format!(
            "{}: the page takes a pointer after every cycle",
            subject.key
        ),
        tally.pointer == CYCLES,
        &detail,
    );
    stage.report.check(
        "Gauntlet",
        &format!("{}: the page takes a key after every cycle", subject.key),
        tally.key == CYCLES,
        &detail,
    );
}

/// Opens the surface and answers whether it came up, capturing the window with it on.
fn open(stage: &mut Stage<'_>, subject: Subject, tag: &str) -> bool {
    let Some((census, panel)) = find::open_panel(stage, subject.panel) else {
        stage
            .report
            .note("Gauntlet", &format!("{} is not laid out", subject.panel));
        return false;
    };
    let Some(at) = find::at_in(&census, panel, subject.trigger) else {
        stage
            .report
            .note("Gauntlet", &format!("nothing says {}", subject.trigger));
        return false;
    };
    stage.click(at);
    stage.settle(10);
    stage.wait(MOVED);
    let up = drawn(stage, subject.title);
    let window = stage.window();
    ink::shot_of(stage, &format!("gx-{tag}-m1"), window);
    up
}

/// Closes it the way this cycle was asked to, and answers whether it went.
fn shut(stage: &mut Stage<'_>, subject: Subject, exit: Exit, tag: &str) -> bool {
    match exit {
        Exit::Escape => stage.key(NamedKey::Escape),
        Exit::Scrim => stage.click(Point::new(DevicePx(CORNER), DevicePx(CORNER))),
        Exit::Control(label) => {
            let census = stage.census();
            let Some(at) = census.innermost(label).and_then(|node| node.centre()) else {
                stage
                    .report
                    .note("Gauntlet", &format!("the surface has no {label}"));
                return false;
            };
            stage.click(at);
        }
    }
    stage.settle(10);
    stage.wait(MOVED);
    let gone = !drawn(stage, subject.title);
    let window = stage.window();
    ink::shot_of(stage, &format!("gx-{tag}-m2"), window);
    gone
}

/// Takes whatever is still over the window away, so one bad cycle does not spoil the next.
fn clear(stage: &mut Stage<'_>, subject: Subject) {
    for _ in 0..3 {
        if !drawn(stage, subject.title) {
            return;
        }
        stage.key(NamedKey::Escape);
        stage.settle(8);
        stage.wait(MOVED);
    }
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}
