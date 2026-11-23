//! Scroll deadlines: the compositor's glide over a virtualised list, in real time, while the
//! window recycles rows.
//!
//! The seven scenarios run on a virtual clock and time the window's thread. This one runs on the
//! wall clock, so a driver on a thread of its own builds its frames at the display's rate while
//! the window builds the rows the glide reaches. Every frame the renderer draws is stamped, and a
//! gap between two drawn frames longer than one and a half refresh intervals is a missed deadline.
//! A miss that spans a frame in which the window rebuilt rows is one the recycle cost.

use std::time::{Duration, Instant};

use zgui::geom::{CssPx, Point};
use zgui::vocab::PointerAction;

use crate::scenario::band::Spread;
use crate::scenario::fixture::{self, Fixture};
use crate::scenario::quiet;

/// How long the glide is carried for.
const RUN: Duration = Duration::from_secs(10);

/// The refresh interval the driver paces at when no presenter reports one.
const INTERVAL: Duration = Duration::from_nanos(16_666_667);

/// How often a wheel notch is delivered, so the glide never runs out.
const NOTCH_EVERY: Duration = Duration::from_millis(667);

/// Runs the scenario and prints its `DEADLINE` line.
pub(crate) fn run() {
    let mut harness = crate::drive::harness(fixture::runtime(Fixture::LongList));
    quiet(&mut harness);
    let middle = Point::new(
        CssPx(crate::gallery::WIDTH / 2.0),
        CssPx(crate::gallery::HEIGHT / 2.0),
    );
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, middle));
    harness.settle(64);

    crate::draw::stamp_frames();
    let start = Instant::now();
    let mut last = start;
    let mut notch = start;
    let mut recycles = Vec::new();
    while start.elapsed() < RUN {
        let now = Instant::now();
        if now >= notch {
            harness.deliver_to_first(crate::input::wheel(middle, 6.0));
            notch += NOTCH_EVERY;
        }
        // The window's clock follows the wall clock, so its deadlines fall when they would.
        harness.advance(now - last);
        last = now;
        let mark = zgui_profile::counter::snapshot();
        let began = Instant::now();
        let ran = harness.pump();
        let ended = Instant::now();
        if ran > 0 && mark.delta(&zgui_profile::counter::snapshot()).boxes_rebuilt > 0 {
            recycles.push((began, ended));
        }
        std::thread::sleep(Duration::from_micros(500));
    }
    let stamps = crate::draw::take_stamps();

    let late = INTERVAL.mul_f64(1.5);
    let mut gaps = Vec::new();
    let mut misses = 0;
    let mut recycle_misses = 0;
    for pair in stamps.windows(2) {
        let gap = pair[1] - pair[0];
        gaps.push(gap.as_secs_f64() * 1e3);
        if gap > late {
            misses += 1;
            let spans = recycles
                .iter()
                .any(|&(began, ended)| began < pair[1] && ended > pair[0]);
            recycle_misses += usize::from(spans);
        }
    }
    if gaps.is_empty() {
        println!("DEADLINE\tscroll\tno frames were drawn");
        return;
    }
    let gap = Spread::of(&mut gaps);
    println!(
        "DEADLINE\tscroll\tframes={}\trecycles={}\tmisses={misses}\trecycle_misses={recycle_misses}\t\
         gap_p50_ms={:.2}\tgap_p99_ms={:.2}\tgap_max_ms={:.2}",
        stamps.len(),
        recycles.len(),
        gap.p50,
        gap.p99,
        gap.max,
    );
}
