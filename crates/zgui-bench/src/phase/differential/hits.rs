//! `hits`: what is under a point, live against a window that holds nothing.
//!
//! The question a pointer asks, asked of both windows at every point of a grid after every step of
//! the script. It is the one thing about a frame no other differential can see: a hit answer is not
//! drawn, so a display list that agrees to the byte and a set of rectangles that agree to the bit
//! both stay silent while an element is drawn in one place and hit in another.
//!
//! # What a green run means, and what it does not
//!
//! **It means the index a running window has been updating a piece at a time still answers what a
//! freshly built one answers.** That is drift, and nothing else in the project reaches it: the live
//! window's index is carried, settled, translated and rebuilt across ninety-five steps, and each of
//! those is a chance to leave an entry behind. Dropping the deferred hierarchy update in
//! `HitIndex::carry` faults 49 of 85 steps here.
//!
//! **It does not mean a hit answer is correct.** Both windows call the same `HitIndex::hit` through
//! the same spatial chain, so an error there is made twice, identically, and cancels. Testing the
//! clip chain in the fragment's own space instead of the space it was measured in — a box translated
//! clean out of its scrollport going on answering clicks over empty screen — leaves this gate green
//! at every size. What rejects that build is
//! `a_transformed_box_answers_only_where_its_ancestors_clip_shows_it` in
//! `zgui-layout/tests/fragments/hits.rs`, which is one of the tests `xtask/src/oracle/subject.rs`
//! names beside this gate and checks for before running it.
//!
//! # Why the answer is compared as positions and not as names
//!
//! The two windows are two documents. A node's name carries which document minted it, so the same
//! element in each of them has two different names and comparing those would report a difference at
//! every point. What is the same in both is where the element sits in its own document — and the
//! path from the root down to what was hit, written as those positions, is also the path an event
//! would travel, so a difference three steps above the target reads as the difference it is.

use zgui::geom::{Device, DevicePx, Point};
use zgui::runtime::Window;

use crate::phase::Driver;
use crate::phase::differential::twin::Twin;
use crate::script::script;

/// How many sample points across the surface.
///
/// A grid rather than the centres of known controls: what this looks for is an element answering
/// somewhere other than where it is drawn, and a sample taken at the middle of where it is
/// *supposed* to be is the one sample a displaced element still gets right. Twenty-four by sixteen
/// puts a point every sixty device pixels or so, which is finer than most of what the gallery
/// draws.
const COLUMNS: u32 = 24;

/// How many down.
const ROWS: u32 = 16;

/// The centres of every cell of the grid over one window's surface.
fn grid(window: &Window) -> Vec<Point<DevicePx, Device>> {
    let size = window.surface().size();
    let (width, height) = (size.width.0, size.height.0);
    let mut points = Vec::with_capacity((COLUMNS * ROWS) as usize);
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            points.push(Point::new(
                DevicePx(width * (column as f32 + 0.5) / COLUMNS as f32),
                DevicePx(height * (row as f32 + 0.5) / ROWS as f32),
            ));
        }
    }
    points
}

/// The path from the root down to whatever is under `point`, as positions in the document.
fn chain(window: &Window, point: Point<DevicePx, Device>) -> Vec<u32> {
    let path = window.chain_at(point);
    let document = window.document().borrow();
    path.iter()
        .filter_map(|node| document.store().index_of(*node))
        .map(|index| index.get())
        .collect()
}

/// What one step's grid found in one window.
fn sample(window: &Window) -> Vec<Vec<u32>> {
    grid(window)
        .into_iter()
        .map(|point| chain(window, point))
        .collect()
}

/// How many different paths the grid found.
fn distinct(sampled: &[Vec<u32>]) -> usize {
    let mut paths: Vec<&[u32]> = sampled
        .iter()
        .filter(|chain| !chain.is_empty())
        .map(Vec::as_slice)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths.len()
}

/// What the run has found so far.
#[derive(Default)]
struct Tally {
    /// Steps where the two windows disagreed anywhere.
    faults: usize,
    /// Steps compared.
    steps: usize,
    /// Steps whose windows had already laid the document out differently.
    apart: usize,
    /// Points sampled, in one window.
    points: usize,
    /// Points that landed on an element.
    landed: usize,
    /// The longest path any point produced.
    deepest: usize,
}

/// Compares one step, reporting what it found.
///
/// # Panics
///
/// Panics when the two windows are not the same size, and when the grid found so little that
/// agreeing about it would mean nothing.
fn compare(step: usize, what: &str, driver: &mut Driver, twin: &mut Twin, tally: &mut Tally) {
    twin.settle(&mut driver.harness);
    if !twin.laid_out_alike(&driver.harness) {
        tally.apart += 1;
        println!("  HIT skip step {step} ({what}): the two windows are not laid out alike");
        return;
    }
    let (live_window, cold_window) = twin.windows(&driver.harness);
    assert_eq!(
        live_window.surface().size(),
        cold_window.surface().size(),
        "step {step} ({what}): the two windows are different sizes, so the grid is not one grid",
    );
    let (live, cold) = (sample(live_window), sample(cold_window));
    let here = distinct(&live);
    // The control. A grid over a document that answers nothing agrees with itself perfectly, and so
    // does one whose every point lands on the same page background: the comparison is worth
    // something only where the answers differ from one another.
    assert!(
        here >= 2,
        "step {step} ({what}): the grid found {here} distinct paths, so nothing is being compared",
    );
    tally.steps += 1;
    tally.points += live.len();
    tally.landed += live.iter().filter(|chain| !chain.is_empty()).count();
    tally.deepest = tally
        .deepest
        .max(live.iter().map(Vec::len).max().unwrap_or(0));

    let points = grid(live_window);
    let mut differing = 0;
    let mut first = None;
    for (index, (one, two)) in live.iter().zip(cold.iter()).enumerate() {
        if one != two {
            differing += 1;
            if first.is_none() {
                let at = points[index];
                first = Some(format!(
                    "({}, {}) live {one:?} cold {two:?}",
                    at.x.0, at.y.0
                ));
            }
        }
    }
    if differing == 0 {
        println!("  HIT ok   step {step} ({what}), {here} distinct paths");
    } else {
        tally.faults += 1;
        println!(
            "  HIT FAULT step {step} ({what}): {differing} of {} points answer differently [{}]",
            live.len(),
            first.as_deref().unwrap_or("-"),
        );
    }
}

/// Runs the phase, or answers `None` when the name is not this one.
///
/// # Panics
///
/// Panics when the two windows answered differently anywhere.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    if phase != "hits" {
        return None;
    }
    let size = driver.size.clone();
    let centres = driver.centres.clone();
    let scheme = driver.scheme;
    let mut twin = Twin::open(&size, &mut driver.harness, scheme, &centres);

    let mut tally = Tally::default();
    compare(0, "settled", driver, &mut twin, &mut tally);
    for (index, step) in script().iter().enumerate() {
        twin.step(&mut driver.harness, step);
        compare(
            index + 1,
            &format!("{step:?}"),
            driver,
            &mut twin,
            &mut tally,
        );
    }

    let verdict = if tally.faults == 0 {
        "ok "
    } else {
        "REGRESSION"
    };
    println!(
        "hit_results_agree_with_a_cold_window {verdict} size={size} compared={} apart={} \
         points={} landed={} deepest={} faults={}",
        tally.steps, tally.apart, tally.points, tally.landed, tally.deepest, tally.faults,
    );
    assert!(
        tally.steps > tally.apart,
        "more steps were skipped than compared, so this says more about the layout the two \
         windows arrived at than about what either of them answers",
    );
    assert_eq!(
        tally.faults, 0,
        "the two windows answered differently at {} steps",
        tally.faults
    );
    Some(0)
}
