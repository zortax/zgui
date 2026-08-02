//! The window used hard, with the same drawings looked for after every step.
//!
//! The fault this is aimed at does not stop anything working. An element repaints, whatever was
//! drawn into it as an outline is not drawn again into the pixels the repaint cleared, and the
//! window goes on being perfectly usable with a hole in it — until something covers the whole
//! surface and brings it back. Nothing inside the process can see that: the box is laid out, the
//! style resolves, the display list has the element in it.
//!
//! So this drives the page the way somebody using it would — hovering, scrolling, opening and
//! shutting surfaces, typing, stepping a carousel, walking the focus, flipping the scheme, and
//! being resized under all of it — and after every one of those it goes and looks at a fixed set of
//! drawings through the compositor. One step per turn of the loop, so the window stays answerable
//! and the pictures stay trustworthy.

use core::cell::Cell;

use zgui::geom::{CssPx, DevicePx, Point, Size};
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::ink;
use crate::stage::Stage;

/// How many times the whole plan is run.
const ROUNDS: usize = 3;

/// What is done to the window between two sweeps.
#[derive(Copy, Clone)]
enum Step {
    /// The pointer dragged across a band of controls.
    Hover,
    /// The page wheeled and dragged up and down.
    Scroll,
    /// Every kind of floating surface opened and shut.
    Overlays,
    /// Text typed into a field and taken out again.
    Typing,
    /// The carousel stepped along.
    Carousel,
    /// The focus walked with the keyboard.
    Tabbing,
    /// The colour scheme flipped, which restyles every box in the window.
    Theme,
}

impl Step {
    /// How this is written into the file names.
    const fn name(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Scroll => "scroll",
            Self::Overlays => "overlays",
            Self::Typing => "typing",
            Self::Carousel => "carousel",
            Self::Tabbing => "tabbing",
            Self::Theme => "theme",
        }
    }
}

/// One round of use. The scheme is flipped twice, so a round ends in the scheme it started in.
const PLAN: [Step; 10] = [
    Step::Hover,
    Step::Scroll,
    Step::Overlays,
    Step::Typing,
    Step::Carousel,
    Step::Tabbing,
    Step::Theme,
    Step::Scroll,
    Step::Overlays,
    Step::Theme,
];

/// How many turns of the loop this part needs: one for the first sweep, then one per step.
pub(crate) const STEPS: usize = 1 + ROUNDS * PLAN.len();

thread_local! {
    /// Which step runs next.
    static CURSOR: Cell<usize> = const { Cell::new(0) };
}

/// Runs one step and looks at every drawing afterwards.
pub(crate) fn chunk(stage: &mut Stage<'_>) {
    let at = CURSOR.with(|cursor| {
        let step = cursor.get();
        cursor.set(step + 1);
        step
    });
    if at >= STEPS {
        return;
    }
    if at == 0 {
        note_size(stage, "base");
        ink::sweep(stage, "00-base");
        return;
    }
    let step = PLAN[(at - 1) % PLAN.len()];
    match step {
        Step::Hover => hover(stage),
        Step::Scroll => scroll(stage),
        Step::Overlays => overlays(stage),
        Step::Typing => typing(stage),
        Step::Carousel => carousel(stage),
        Step::Tabbing => tabbing(stage),
        Step::Theme => theme(stage),
    }
    let when = format!("{at:02}-{}", step.name());
    note_size(stage, &when);
    ink::sweep(stage, &when);
}

/// Writes down how big the window is now, since it is resized under this from outside.
fn note_size(stage: &mut Stage<'_>, when: &str) {
    let root = stage.handles().root();
    if let Some(rect) = stage.handles().host.window_box(root) {
        stage.report.rect(
            &format!("window:{when}"),
            rect.origin.x.0,
            rect.origin.y.0,
            rect.size.width.0,
            rect.size.height.0,
        );
    }
}

/// Drags the pointer across a band of controls, so that every one of them lights and unlights.
fn hover(stage: &mut Stage<'_>) {
    let Some((_census, panel)) = find::open_panel(stage, "Button") else {
        return;
    };
    for step in 0..12 {
        let across = panel.origin.x.0 + panel.size.width.0 * (step as f32 / 12.0);
        stage.move_to(Point::new(
            DevicePx(across),
            DevicePx(panel.origin.y.0 + panel.size.height.0 * 0.5),
        ));
    }
    stage.settle(6);
}

/// Wheels and drags the page about, and puts it back near where it was.
fn scroll(stage: &mut Stage<'_>) {
    let Some((_census, panel)) = find::open_panel(stage, "Card") else {
        return;
    };
    stage.move_to(Point::new(
        DevicePx(panel.origin.x.0 + panel.size.width.0 * 0.5),
        DevicePx(panel.origin.y.0 + panel.size.height.0 * 0.5),
    ));
    stage.wheel((0.0, 9.0));
    stage.wheel((0.0, -4.0));
    stage.trackpad(Size::new(CssPx(0.0), CssPx(-320.0)));
    stage.trackpad(Size::new(CssPx(0.0), CssPx(180.0)));
    stage.settle(6);
}

/// Opens and shuts a modal, a sheet and a popover.
fn overlays(stage: &mut Stage<'_>) {
    for (panel, trigger) in [
        ("Dialog", "Rename…"),
        ("Sheet and drawer", "Details"),
        ("Popover, tooltip, hover card", "Size"),
    ] {
        let Some((census, rect)) = find::open_panel(stage, panel) else {
            continue;
        };
        let Some(at) = find::at_in(&census, rect, trigger) else {
            continue;
        };
        stage.click(at);
        stage.settle(10);
        stage.key(NamedKey::Escape);
        stage.settle(10);
    }
}

/// Types into a field and takes it back out.
fn typing(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Input and textarea") else {
        return;
    };
    let Some(label) = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text == "Display name" && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
    else {
        return;
    };
    stage.click(Point::new(
        DevicePx(label.origin.x.0 + label.size.width.0 / 2.0),
        DevicePx(label.origin.y.0 + label.size.height.0 + 20.0),
    ));
    stage.type_text("wear");
    for _ in 0..4 {
        stage.key(NamedKey::Backspace);
    }
    stage.settle(6);
}

/// Steps the carousel along, twice.
fn carousel(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Carousel") else {
        return;
    };
    // Its two controls carry no text at all; the later of them along the row is the one that goes
    // forward.
    let Some(next) = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0 && node.area() < 4000.0)
        .filter_map(|node| node.rect)
        .max_by(|left, right| left.origin.x.0.total_cmp(&right.origin.x.0))
    else {
        return;
    };
    for _ in 0..2 {
        stage.click(Point::new(
            DevicePx(next.origin.x.0 + next.size.width.0 / 2.0),
            DevicePx(next.origin.y.0 + next.size.height.0 / 2.0),
        ));
        stage.settle(8);
    }
}

/// Walks the focus through the document with the keyboard.
fn tabbing(stage: &mut Stage<'_>) {
    for _ in 0..14 {
        stage.key(NamedKey::Tab);
    }
    stage.settle(6);
}

/// Flips the colour scheme, which restyles every box in the window.
fn theme(stage: &mut Stage<'_>) {
    let Some(label) = stage.census().control("Dark").map(|node| node.id) else {
        return;
    };
    stage.reveal(label);
    let Some(rect) = stage.census().node(label).and_then(|node| node.rect) else {
        return;
    };
    // The switch itself says nothing, and sits at the end of the words that name it.
    stage.click(Point::new(
        DevicePx(rect.origin.x.0 + rect.size.width.0 + 24.0),
        DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
    ));
    stage.settle(12);
}
