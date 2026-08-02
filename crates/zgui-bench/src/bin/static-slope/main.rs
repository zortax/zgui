//! A large static document under single-property updates: what one control's colour costs when
//! there are ten thousand of them.
//!
//! The first of the reference workloads. It exists because the claim every later compositor phase
//! makes is some form of "the cost of a change is the size of the change, not the size of the
//! document", and that claim needs a document big enough for the difference to be visible and a
//! change small enough to have an obvious right answer. Ten thousand controls, and one of them
//! changes one property.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin static-slope
//! ```
//!
//! # What it holds fixed and what it varies
//!
//! Fixed: the interaction. One class, on one control, changing one declaration — the same class on
//! the same control at every size, so nothing about the *change* differs between the four
//! documents.
//!
//! Varied: how many controls are in the document — 1 250, 2 500, 5 000, 10 000. Four sizes rather
//! than two because a slope taken from two points is a line through two points, and it cannot tell
//! a cost proportional to the document from one proportional to its square.
//!
//! # What it publishes
//!
//! - **Click nanoseconds per control**: the least-squares slope of the single-property update
//!   against control count. Printed, keyed to the machine that took it, and **gating nothing**.
//! - **The same-run ratio** of that slope to the slope of a *whole-document* single-property
//!   update — one class on the root that every control's colour depends on, which reaches every
//!   element in the document by a route that has nothing to do with how many of them a change
//!   ought to touch. That ratio is what gates. A machine twice as fast halves both slopes and
//!   leaves it exactly where it was.
//! - **Elements restyled per control added**, from the counters. A count is a property of the
//!   design rather than of the machine, so this one needs no baseline at all: a local update that
//!   restyles one more element for every hundred controls added is an invalidation that has stopped
//!   being local, whatever the clock says about it.
//!
//! # Why the controls are `<control>` and not a shipped button
//!
//! A shipped [`Button`](zgui_ui::button::Button) is several elements deep, so "ten thousand
//! buttons" is a forty-thousand-element document and the axis the slope is taken against would no
//! longer be the number of controls. The element here is the framework's own `<control>`, styled
//! with the ordinary run of rules a control carries — a border, a background, a hover rule, a focus
//! ring — so the cascade has a real control's work to do and the count on the x-axis is the count
//! in the name.

#![forbid(unsafe_code)]

mod criteria;
mod document;
mod measure;

use zgui_bench::reference::{fit, verdict};

/// The four document sizes, in controls.
///
/// They double rather than quadruple: ten thousand is the size the deliverable names, and four
/// doubling steps down from it reaches 1 250, which is still a document large enough that a cost
/// proportional to it is unmistakable against the noise on one interaction.
const CONTROLS: [usize; 4] = [1_250, 2_500, 5_000, 10_000];

fn main() {
    let mut local = Vec::new();
    let mut global = Vec::new();
    let mut restyles = Vec::new();
    let mut visits = Vec::new();

    for controls in CONTROLS {
        let signals = document::Signals::new();
        let mut harness = document::opened(controls, signals);
        let built = document::controls_built(&harness);
        assert_eq!(
            built, controls,
            "the document was asked for {controls} controls and laid out {built}, so the axis the \
             slope is taken against is not the axis it is named for",
        );

        let one = measure::local(&mut harness, signals);
        let whole = measure::global(&mut harness, signals);
        let work = measure::work_per_local_update(&mut harness, signals);
        println!(
            "SIZE controls={controls} local_ns={one:.1} global_ns={whole:.1} \
             restyled={} visited={} relaid_out={} diffed={} emitted={} tree_inserts={}",
            work.restyled, work.visited, work.relaid_out, work.diffed, work.emitted, work.inserts,
        );

        #[expect(
            clippy::cast_precision_loss,
            reason = "the control count is ten thousand at the largest, and the counts beside it \
                      are bounded by the number of elements, so both are exactly representable"
        )]
        let (axis, restyled, visited) =
            (controls as f64, work.restyled as f64, work.visited as f64);
        local.push((axis, one));
        global.push((axis, whole));
        restyles.push((axis, restyled));
        visits.push((axis, visited));
    }

    let local_slope = fit::slope(&local);
    let global_slope = fit::slope(&global);
    let restyle_slope = fit::slope(&restyles);

    let mut report = verdict::Report::new();
    if let Some(slope) = local_slope {
        report.advisory(format!(
            "click slope {slope:.4} ns per control, over {} controls",
            CONTROLS[CONTROLS.len() - 1]
        ));
    }
    if let Some(slope) = global_slope {
        report.advisory(format!(
            "whole-document update slope {slope:.4} ns per control"
        ));
    }
    report.judged(&criteria::LOCALITY.judge(local_slope, global_slope));
    report.judged(&criteria::RESTYLE_LOCALITY.judge_directly(restyle_slope));
    report.judged(&criteria::VISIT_LOCALITY.judge_directly(fit::slope(&visits)));
    println!("{report}");
    if !report.passed() {
        std::process::exit(1);
    }
}
