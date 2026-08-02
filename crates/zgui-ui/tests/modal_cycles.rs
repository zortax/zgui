//! Whether a window is still a window after a modal surface has come and gone many times.
//!
//! # Why a control is pressed after every cycle
//!
//! Everything a modal surface installs is invisible. A focus trap, an entry on the dismissable
//! stack, a hold on the window's scroll lock and a set of listeners are all things a photograph of
//! a closed dialog cannot disagree with: the surface is gone from the screen whether or not any of
//! them came down with it. A window carrying a trap it never released still hovers, still scrolls
//! and still paints — and answers no press and no key ever again.
//!
//! So the assertion is not that the surface closed. It is that an ordinary control on the page
//! behind it still works afterwards: its value goes up, and the window draws a different picture
//! because of it. That is a claim only a window with nothing left over can satisfy, and it is made
//! after **every** cycle, because the defect these fixtures were written for was invisible on the
//! first one and total on the second.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::reactive::RwSignal;
use zgui::reactive::prelude::{Get, GetUntracked, Set};
use zgui::view;
use zgui::view::AnyView;
use zgui::vocab::NamedKey;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{HEIGHT, SETTLED, Stage, WIDTH};
use crate::painted::words::{aim, assert_absent, assert_painted};

/// The page every fixture is laid out on.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// How many times each fixture opens and closes its surface.
///
/// Ten rather than two. Two is the smallest number that catches the defect these were written for,
/// and a fixture that stopped there would pass for anything that accumulates more slowly.
const CYCLES: u32 = 10;

/// The label of the control every fixture presses between cycles.
const TALLY: &str = "Tally";

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// The whole surface, which is what a picture of the window is taken over.
fn surface() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
    )
}

/// Where a fixture leaves the count its view built, so it can read what a press did.
#[derive(Clone, Default)]
struct Counter(Rc<RefCell<Option<RwSignal<u32, zgui::reactive::LocalStorage>>>>);

impl Counter {
    /// Records the signal the view built.
    fn keep(&self, signal: RwSignal<u32, zgui::reactive::LocalStorage>) {
        *self.0.borrow_mut() = Some(signal);
    }

    /// How many presses have landed.
    ///
    /// # Panics
    ///
    /// Panics before the view has been built, because a count read from nowhere agrees with
    /// everything.
    fn get(&self) -> u32 {
        self.0
            .borrow()
            .expect("the view built its count")
            .get_untracked()
    }
}

/// The control the page answers with, and the line that says what it has counted.
///
/// Written as a fragment each fixture pastes in front of its own surface, because the assertion is
/// about the page *behind* a modal rather than about anything on it.
macro_rules! page {
    ($counter:expr, $($surface:tt)*) => {{
        let clicks = RwSignal::new_local(0u32);
        $counter.keep(clicks);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Button(on:click = move |_| clicks.set(clicks.get_untracked() + 1)) {
                        {TALLY}
                    }
                    text {{move || format!("Clicks {}", clicks.get())}}
                    $($surface)*
                }
            }
        })
    }};
}

/// Opens the surface `trigger` opens, closes it with `close`, and does it [`CYCLES`] times.
///
/// After each cycle the page's own control is pressed, and both halves of what a working control
/// does are asserted: the value it holds goes up, and the window's pixels change because of it.
/// Either on its own would pass for a window that had stopped listening — a value can be written
/// by something other than the press, and pixels change for a transition nobody asked for.
fn cycle(stage: &mut Stage, counter: &Counter, trigger: &str, title: &str, close: fn(&mut Stage)) {
    for round in 1..=CYCLES {
        let at = aim(stage, trigger);
        stage.click(at);
        stage.wait(SETTLED);
        assert_painted(stage, title);

        close(stage);
        stage.wait(SETTLED);
        assert_absent(stage, title);

        let value = counter.get();
        let before = stage.colours_in(surface());
        let at = aim(stage, TALLY);
        stage.click(at);
        stage.wait(SETTLED);

        assert_eq!(
            counter.get(),
            value + 1,
            "the page stopped answering a press after {round} of {CYCLES} cycles"
        );
        assert_painted(stage, &format!("Clicks {}", value + 1));
        let after = stage.colours_in(surface());
        assert_ne!(
            before, after,
            "the window drew the same picture after the press as before it, on cycle {round}"
        );
    }
}

/// Presses Escape.
fn by_escape(stage: &mut Stage) {
    stage.press_named(NamedKey::Escape);
}

/// Presses the dimmed window behind the surface, well clear of anything on it.
fn by_scrim(stage: &mut Stage) {
    stage.click(Point::new(DevicePx(8.0), DevicePx(8.0)));
}

/// Presses the surface's own close control.
fn by_close_button(stage: &mut Stage) {
    let at = aim(stage, "Done");
    stage.click(at);
}

/// Takes the answer the surface is asking for.
fn by_answering(stage: &mut Stage) {
    let at = aim(stage, "Delete it");
    stage.click(at);
}

#[test]
fn a_dialog_closed_by_escape_leaves_the_page_working() {
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        Dialog {
            DialogTrigger {"Open dialog"}
            DialogContent {
                DialogTitle {"Rename project"}
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open dialog",
        "Rename project",
        by_escape,
    );
}

#[test]
fn a_dialog_closed_by_its_own_button_leaves_the_page_working() {
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        Dialog {
            DialogTrigger {"Open dialog"}
            DialogContent {
                DialogTitle {"Rename project"}
                DialogFooter {DialogClose {"Done"}}
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open dialog",
        "Rename project",
        by_close_button,
    );
}

#[test]
fn a_dialog_closed_by_the_scrim_leaves_the_page_working() {
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        Dialog {
            DialogTrigger {"Open dialog"}
            DialogContent {
                DialogTitle {"Rename project"}
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open dialog",
        "Rename project",
        by_scrim,
    );
}

#[test]
fn an_alert_dialog_answered_from_inside_leaves_the_page_working() {
    // The one surface a press on the scrim does not close, so the way out is the answer itself.
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        AlertDialog {
            AlertDialogTrigger {"Open alert"}
            AlertDialogContent {
                AlertDialogTitle {"Delete this invoice"}
                AlertDialogFooter {
                    AlertDialogAction {"Delete it"}
                }
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open alert",
        "Delete this invoice",
        by_answering,
    );
}

#[test]
fn a_sheet_closed_by_escape_leaves_the_page_working() {
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        Sheet {
            SheetTrigger {"Open sheet"}
            SheetContent {
                SheetTitle {"Invoice details"}
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open sheet",
        "Invoice details",
        by_escape,
    );
}

#[test]
fn a_drawer_closed_by_its_own_button_leaves_the_page_working() {
    let counter = Counter::default();
    let built = counter.clone();
    let mut stage = staged!(move || page!(
        built,
        Drawer {
            DrawerTrigger {"Open drawer"}
            DrawerContent {
                DrawerTitle {"Share this invoice"}
                DrawerFooter {DrawerClose {"Done"}}
            }
        }
    ));
    stage.wait(SETTLED);
    cycle(
        &mut stage,
        &counter,
        "Open drawer",
        "Share this invoice",
        by_close_button,
    );
}
