//! Wall-clock budgets for the layout engine.
//!
//! Every case here asserts a time, which makes this target unlike every other test in the crate: it
//! is meaningful only in an optimised build, and it is meaningful only if what it times is work the
//! engine does rather than work the harness does around it. Both are structural rather than
//! remembered. The target is behind the `wall-clock` feature, so the ordinary debug run cannot
//! execute it and mistake an unoptimised number for a regression, and the gate turns that feature
//! on and runs this target in release; a wall-clock assertion in any other file under a crate's
//! `tests/` is a ledger violation.
//!
//! # How a number here is arrived at
//!
//! A budget is measured, not chosen. Each case runs its loop many times and keeps the *fastest*
//! round: on a machine with anything else running, the slow rounds measure the scheduler and the
//! clock, and the fastest round is the closest thing to the cost of the code. The budget is then
//! set far enough above that figure that the spread between runs cannot reach it, because a budget
//! that is red one afternoon in three is one nobody reads. Each case states what it measured, what
//! it was set to, and what the difference between the two is there to absorb.
//!
//! That headroom is affordable only because a case does not rely on its clock alone. A budget that
//! is about a specific piece of work says so in a counter as well — how many times the work was
//! done, which is the same number on a fast machine and a slow one — so the regression the case was
//! written for fails it exactly, and the time is left to catch the slowdowns nothing counts.

mod support;

use std::time::{Duration, Instant};

use support::text::{first_inline_root, lines, paragraph};
use support::{Content, Element, Fixture, lay_out, lay_out_only, measurer};
use zgui_layout::BoxKey;
use zgui_layout::tree::store::LayoutStore;

/// How many times a case repeats its loop before believing the fastest round.
const ROUNDS: usize = 15;

/// How many widths one round asks the paragraph about.
const WIDTHS: usize = 20;

/// What the fastest round of [`twenty_widths`] is allowed to cost.
///
/// Stated once, so the number in the message a failure prints is the number that was asserted.
const BUDGET: Duration = Duration::from_micros(1_800);

/// Runs `round` [`ROUNDS`] times and returns how long the fastest of them took.
fn fastest(mut round: impl FnMut()) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        round();
        best = best.min(start.elapsed());
    }
    best
}

/// Asks one context about [`WIDTHS`] widths in a row, invalidating it before each one.
///
/// The context is named by the caller rather than looked up here: finding it means walking the box
/// tree, which is the harness and not the engine, and nothing inside the loop can move it.
fn twenty_widths(store: &mut LayoutStore, content: &mut Content, root: BoxKey) {
    for step in 0..WIDTHS {
        let width = 300.0 + step as f32 * 3.0;
        zgui_layout::tree::dirty::mark_dirty(store, root);
        lay_out_only(store, content, width, 40000.0);
    }
}

#[test]
fn resizing_a_two_hundred_line_paragraph_costs_under_one_and_eight_tenths_milliseconds() {
    // A resize drags a paragraph through a run of widths, and every one of them is a question the
    // layout algorithm asks while the user is still holding the mouse. Twenty of them over a
    // two-hundred-line paragraph is the case that has to stay imperceptible.
    //
    // Timed: the layout pass alone, which is what this crate can make faster. Composing fragments,
    // maintaining the hit index and checking the three levels agree cost several times as much on
    // this fixture, and none of the three is what the budget is about — a loop that timed them
    // would be measuring the test harness and would report the same number however fast layout
    // became.
    //
    // The budget: 1.8 ms for twenty widths, 90 us each. The method is the one this target's own
    // documentation describes — fifteen rounds, keep the fastest. By it the loop measures 0.79 ms
    // to 0.85 ms across repeated runs on the development machine, and the figure is that stable
    // because taking the fastest of fifteen discards the rounds that measured the scheduler: the
    // individual rounds inside one run reach 1.7 ms while nothing about the work changes.
    //
    // The headroom over that is a factor of about two, which is what an unremarkably slower machine
    // costs, and it stops there deliberately. Before a paragraph's flattened form was kept beside
    // its box it was rebuilt character by character for every width, and the same loop measured
    // 2.07 ms by the same method — so a budget set any higher than this could not fail on the one
    // regression it was written for, and would have been a number that could only ever go red on a
    // catastrophe. The flattening count below catches that regression exactly, on any machine at
    // any speed; this catches it as well, and catches the slowdowns no counter is watching.
    let text: &'static str = Box::leak(paragraph(2400).into_boxed_str());
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        "root { display: block }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 300.0, 40000.0);
    assert!(
        lines(&store).len() >= 200,
        "the fixture broke into {} lines, so the budget is measuring nothing",
        lines(&store).len()
    );

    let before = content.shaper().breaks;
    let root = first_inline_root(&store);
    let elapsed = fastest(|| twenty_widths(&mut store, &mut content, root));

    // The guards that stop the budget passing on a loop that did no work: every width has to have
    // reached the breaker, none of them may have re-shaped, the paragraph has to have gone on
    // wrapping to the end, and the width-independent half has to have been done once for all of
    // them rather than once each.
    let passes = content.shaper().breaks - before;
    assert!(
        passes >= (ROUNDS * WIDTHS - 1) as u32,
        "{} widths cost only {passes} breaking passes",
        ROUNDS * WIDTHS
    );
    assert_eq!(content.shaper().shapes, 1, "and not one re-shape");
    assert!(
        lines(&store).len() >= 200,
        "the last width has to wrap as much as the first"
    );
    assert_eq!(
        store.flattenings(),
        1,
        "the paragraph was flattened into the shaper's string {} times for {} widths",
        store.flattenings(),
        ROUNDS * WIDTHS + 1
    );

    assert!(
        elapsed < BUDGET,
        "the fastest of {ROUNDS} rounds of {WIDTHS} widths took {elapsed:?}, over a budget of \
         {BUDGET:?}"
    );
}
