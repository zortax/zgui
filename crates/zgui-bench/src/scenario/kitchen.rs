//! Kitchen sink: every component this library ships, in one window, driven four ways.
//!
//! The scenario the schedule names for resize, widened to carry the three other interactions that
//! have a measured number against this exact document. They belong together because they share a
//! denominator: whatever the gallery costs to restyle, lay out and emit is the constant every one
//! of them is measured on top of, and splitting them across four scenarios would mean building the
//! same thirteen sections four times to learn four things about one document.
//!
//! - **Resize.** The window's extent moves and every box in it is asked again. Nothing else here
//!   touches the whole document, which is why this is the millisecond measurement and the rest are
//!   microsecond ones.
//! - **Click.** One class on one element. The cost of a purely local change, and the number the
//!   per-box slope was fitted from.
//! - **Keystroke.** One character into a real text field: an edit, a reshape of one paragraph, a
//!   caret moved and a repaint of one box.
//! - **Glide tick.** One frame of the scroll animation the framework runs after a wheel notch.

use zgui::geom::{CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::vocab::{KeyCode, KeyEvent, NamedKey, PhysicalKey, PointerAction};

use crate::scenario::band::{
    Band, INTERACTION_TOLERANCE, Measurement, Pace, STARTUP_TOLERANCE, Spread,
};
use crate::scenario::{Outcome, counters, quiet};

/// How many times each interaction is repeated.
const REPEATS: usize = 32;

/// How many refresh intervals the untouched window is watched for.
const UNTOUCHED_TURNS: usize = 240;

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::opened("s13");
    quiet(&mut harness);
    let boxes = harness.app().windows()[0].layout().borrow().keys().len();
    let centres = crate::inspect::swatch_centres(&harness.app().windows()[0]);
    assert_eq!(centres.len(), 4, "the four probe swatches were found");
    let middle = Point::new(
        CssPx(crate::gallery::WIDTH / 2.0),
        CssPx(crate::gallery::HEIGHT / 2.0),
    );

    let before = zgui_profile::counter::snapshot();

    // The order below is not free, and it is the order the recorded numbers were taken in.
    //
    // Keystroke goes first because the field it types into is *found* by tabbing from the top of
    // the document, and anything that moves focus first — a click, most obviously — changes which
    // field that is and therefore what a keystroke into it costs. Resize goes last because every
    // step of it leaves the window a different width, and a number taken at a width no other
    // measurement used is a number that cannot be compared with the ones taken before it.

    // Keystroke, into whichever field tabbing reaches first.
    let (tabs, mut traversal) = crate::input::focus_a_field_timed(&mut harness);
    let mut keys = Vec::with_capacity(REPEATS);
    let letters = ["a", "b", "c", "d", "e", "f", "g", "h"];
    for step in 0..REPEATS {
        let letter = letters[step % letters.len()];
        let started = std::time::Instant::now();
        harness.deliver_to_first(crate::input::key(KeyEvent::character(letter)));
        harness.settle(64);
        keys.push(started.elapsed().as_secs_f64() * 1e6);
        harness.deliver_to_first(crate::input::key_up(KeyEvent::character(letter)));
        harness.settle(64);
        if step % 8 == 7 {
            for _ in 0..8 {
                harness.deliver_to_first(crate::input::key(KeyEvent::named(
                    NamedKey::Backspace,
                    PhysicalKey::Code(KeyCode::Backspace),
                )));
                harness.settle(64);
                harness.deliver_to_first(crate::input::key_up(KeyEvent::named(
                    NamedKey::Backspace,
                    PhysicalKey::Code(KeyCode::Backspace),
                )));
                harness.settle(64);
            }
        }
    }

    // Click. The pointer is put on the swatch and settled first, so what is timed is the press and
    // not the crossing that preceded it.
    let mut clicks = Vec::with_capacity(REPEATS);
    for step in 0..REPEATS {
        let at = centres[step % centres.len()];
        harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, at));
        harness.settle(64);
        let started = std::time::Instant::now();
        harness.deliver_to_first(crate::input::pointer(PointerAction::Pressed, at));
        harness.settle(64);
        clicks.push(started.elapsed().as_secs_f64() * 1e6);
        harness.deliver_to_first(crate::input::pointer(PointerAction::Released, at));
        harness.settle(64);
    }

    // Glide. One notch, then every tick the animation carries after it.
    let mut ticks = Vec::new();
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, middle));
    harness.settle(64);
    for step in 0..8 {
        let lines = if step % 2 == 0 { 3.0 } else { -3.0 };
        harness.deliver_to_first(crate::input::wheel(middle, lines));
        harness.settle(64);
        let mut carried = 0;
        while crate::input::gliding(&harness) && carried < 60 {
            let started = std::time::Instant::now();
            harness.advance(std::time::Duration::from_micros(16_667));
            let ran = harness.pump();
            if ran > 0 {
                ticks.push(started.elapsed().as_secs_f64() * 1e6);
            }
            carried += 1;
        }
    }
    // Resize. Each step is a different width, so nothing can be answered from the last one.
    let mut resizes = Vec::with_capacity(REPEATS);
    for step in 0..REPEATS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a step index bounded by the repeat count"
        )]
        let width = crate::gallery::WIDTH + (step % 24) as f32 * 8.0;
        let started = std::time::Instant::now();
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(width),
            DevicePx(crate::gallery::HEIGHT),
        )));
        harness.settle(64);
        resizes.push(started.elapsed().as_secs_f64() * 1e3);
    }

    // Untouched, but not still: the gallery ships an indeterminate progress bar, so this is what a
    // turn costs a window that is drawing an animation and nothing else. It is the other half of
    // the idle question — that scenario asks what a loop with nothing to do costs, and this asks
    // what the cheapest thing a loop can actually have to do costs.
    let mut untouched = Vec::with_capacity(UNTOUCHED_TURNS);
    for _ in 0..UNTOUCHED_TURNS {
        let started = std::time::Instant::now();
        harness.advance(std::time::Duration::from_micros(16_667));
        harness.pump();
        untouched.push(started.elapsed().as_secs_f64() * 1e6);
    }

    let all = before.delta(&zgui_profile::counter::snapshot());
    assert!(
        !ticks.is_empty(),
        "no wheel notch started a glide, so no scroll animation frame was measured"
    );

    // Every frame this scenario drove, in one population, against the interval it drove them at.
    let mut every: Vec<f64> = traversal
        .iter()
        .chain(keys.iter())
        .chain(clicks.iter())
        .chain(ticks.iter())
        .chain(untouched.iter())
        .copied()
        .chain(resizes.iter().map(|ms| ms * 1e3))
        .collect();
    every.sort_by(f64::total_cmp);
    let pace = Pace::of(&every, 16_667.0);
    let resize = Spread::of(&mut resizes);
    let click = Spread::of(&mut clicks);
    let key = Spread::of(&mut keys);
    let quiet_turn = Spread::of(&mut untouched);
    let glide = Spread::of(&mut ticks);
    let tab = Spread::of(&mut traversal);

    Outcome {
        scenario: "kitchen-sink",
        document: format!(
            "gallery s13, {boxes} boxes, a text field {tabs} tab stops in, {REPEATS} of each \
             interaction"
        ),
        measurements: vec![
            Measurement {
                name: "kitchen.resize",
                unit: "ms",
                value: resize.p50,
                band: Band::Time {
                    baseline: 8.2,
                    tolerance: STARTUP_TOLERANCE,
                },
                rationale: "the measured on-screen p50, which every box in the window pays into",
                budget: None,
                spread: Some(resize),
            },
            Measurement {
                name: "kitchen.click",
                unit: "us",
                value: click.p50,
                band: Band::Time {
                    baseline: 10.8,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "the measured p50 at 1 851 boxes: one class on one element",
                budget: None,
                spread: Some(click),
            },
            Measurement {
                name: "kitchen.keystroke",
                unit: "us",
                value: key.p50,
                band: Band::Time {
                    baseline: 291.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "the measured p50: one edit, one paragraph reshaped, one box repainted",
                budget: None,
                spread: Some(key),
            },
            Measurement {
                name: "kitchen.untouched_turn",
                unit: "us",
                value: quiet_turn.p50,
                band: Band::Time {
                    baseline: 22.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "the measured p50 of one turn over an untouched but animating gallery",
                budget: None,
                spread: Some(quiet_turn),
            },
            Measurement {
                name: "kitchen.glide_tick",
                unit: "us",
                value: glide.p50,
                band: Band::Time {
                    baseline: 610.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "the measured p50 of one frame of the framework's own scroll animation",
                budget: None,
                spread: Some(glide),
            },
            Measurement {
                name: "kitchen.tab_traversal",
                unit: "us",
                value: tab.p50,
                band: Band::Time {
                    baseline: 381.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "one Tab press moves focus by one stop, which is one element's style \
                            and one element's shaping",
                budget: None,
                spread: Some(tab),
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&all))
        .collect(),
        counters: counters(&all),
        notes: vec![format!(
            "the tab traversal is inside the measured part now, and it is the widest distribution \
             this scenario takes: p50 {:.1} us, p99 {:.1} us, max {:.1} us over {} presses",
            tab.p50, tab.p99, tab.max, tab.samples
        )],
        pace,
    }
}
