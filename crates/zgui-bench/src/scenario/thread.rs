//! Thread resize: a chat-shaped document dragged wider, one width at a time.
//!
//! The other resize number, `kitchen.resize`, is taken over the gallery — thirteen sections of
//! controls, little of it prose. What a person actually resizes is a window with a conversation
//! in it: hundreds of wrapped paragraphs in a flex column inside a scroll container, most of them
//! off screen. That shape misses every cache a resize can miss — every box re-asks its question,
//! every paragraph re-breaks its lines — so its cost scales with the thread rather than with the
//! window, and none of the other scenarios can see it.
//!
//! Two phases over one document. The **step** phase is `kitchen.resize`'s loop over this document:
//! each step is a new width, delivered and settled, and is the millisecond figure the resize work
//! is judged by. The **drag** phase replays the pacing question from `resize_cost.rs`: a configure
//! per millisecond against a 75 Hz output, where the bound is elapsed time over the refresh
//! interval and never the number of configures that arrived.
//!
//! Beside the times, every step records what the frame *did* — nodes relaid out, paragraphs
//! rebroken, chunks re-encoded against replayed — because those counts are machine-independent
//! and each names the stage a regression lives in. `ZGUI_BENCH_FRAMES=1` prints them per step.

use std::time::Duration;

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::view::{Anchor, IntoView, View};
use zgui::{component, view};
use zgui_profile::{Counter, counter};

use crate::scenario::band::{Band, Measurement, Pace, STARTUP_TOLERANCE, Spread};
use crate::scenario::{Outcome, counters, fixture, quiet};

/// How many messages the thread holds.
///
/// Chosen so one width step costs whole milliseconds on an ordinary machine — a resize whose cost
/// is dominated by the fixed frame overhead would band the overhead — while staying near what a
/// long conversation actually holds.
const MESSAGES: usize = 250;

/// How many width steps the step phase takes.
const REPEATS: usize = 32;

/// How many distinct widths the steps cycle through, matching `kitchen.resize`'s cycle.
const CYCLE: usize = 24;

/// How long the paced drag lasts: one configure per millisecond, one millisecond per turn.
const DRAG_MILLIS: u64 = 120;

/// The output the drag is paced against, in millihertz.
const DRAG_OUTPUT: u32 = 75_000;

/// The words the thread's prose is assembled from.
///
/// A fixed vocabulary rather than lorem ipsum, so the text is deterministic across runs and the
/// shaping cache sees the same runs every time.
const WORDS: &[&str] = &[
    "the",
    "resize",
    "asks",
    "every",
    "paragraph",
    "its",
    "question",
    "again",
    "and",
    "what",
    "one",
    "step",
    "costs",
    "is",
    "decided",
    "by",
    "how",
    "much",
    "of",
    "last",
    "width's",
    "answer",
    "was",
    "kept",
    "a",
    "line",
    "that",
    "wraps",
    "carries",
    "words",
    "over",
    "to",
    "where",
    "they",
    "fit",
    "now",
    "while",
    "fixed",
    "header",
    "stays",
];

/// `count` words of deterministic prose, drawn from [`WORDS`] by `seed`.
fn prose(seed: usize, count: usize) -> String {
    let mut text = String::new();
    for step in 0..count {
        if step > 0 {
            text.push(' ');
        }
        text.push_str(
            WORDS[seed.wrapping_mul(31).wrapping_add(step.wrapping_mul(7)) % WORDS.len()],
        );
    }
    text
}

/// How many wrapped paragraphs the whole thread holds.
fn paragraphs() -> usize {
    (0..MESSAGES).map(|index| 1 + index % 3).sum()
}

/// The thread's own styles.
///
/// The log is the shape the complaint arrives in: a flex column inside a scroll container, sized
/// by the window's width, so a width step re-asks every row. The header's two cells carry fixed
/// widths on purpose — a label whose container is pinned owes a resize nothing, and the counters
/// this scenario reports are read against that.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .thread-log {
        display: flex;
        flex-direction: column;
        overflow-y: auto;
        height: 1000px;
        padding: 12px;
        gap: 10px;
     }
     .thread-msg { gap: 4px }
     .thread-head { gap: 8px; align-items: center }
     .thread-name { width: 96px }
     .thread-when { width: 48px; color: #8b93a7 }
     .thread-para { line-height: 1.45 }
     .thread-strong { font-weight: 700 }
     .thread-em { font-style: italic }
     .thread-code { font-family: monospace; background-color: #1e2532 }
     .thread-card {
        width: 320px;
        height: 80px;
        background-color: #1b202b;
        border: 1px solid #232833;
     }"
);

/// One paragraph: styled spans in one inline context, the shape markdown prose lowers to.
#[component]
fn Para(
    /// What decides the words.
    seed: usize,
) -> impl IntoView {
    view! {
        box(class = "thread-para") {
            text {{format!("{} ", prose(seed, 9))}}
            text(class = "thread-strong") {{prose(seed + 1, 3)}}
            text {{format!(" {} ", prose(seed + 2, 11))}}
            text(class = "thread-code") {{format!("frame_budget({seed})")}}
            text {{format!(" {} ", prose(seed + 3, 8))}}
            text(class = "thread-em") {
                text(class = "thread-strong") {{prose(seed + 4, 4)}}
            }
        }
    }
}

/// One message: a fixed-width header, some paragraphs, and now and then a fixed-size card.
#[component]
fn Message(
    /// Which message this is.
    index: usize,
) -> impl IntoView {
    let count = 1 + index % 3;
    let carded = index.is_multiple_of(10);
    view! {
        column(class = "thread-msg") {
            row(class = "thread-head") {
                text(class = "thread-name") {{format!("agent-{}", index % 7)}}
                text(class = "thread-when") {{format!("{:02}:{:02}", index / 60 % 24, index % 60)}}
            }
            for para in move || 0..count, key = |para: &usize| *para {
                Para(seed = index * 131 + para * 17)
            }
            if move || carded {
                box(class = "thread-card") {}
            }
        }
    }
}

/// The whole thread.
#[component]
fn Thread() -> impl IntoView {
    view! {
        scroll(class = "thread-log") {
            for index in move || 0..MESSAGES, key = |index: &usize| *index {
                Message(index = index)
            }
        }
    }
}

/// A surface extent of `width` by the gallery's height.
fn wide(width: f32) -> Size<DevicePx, zgui::geom::Device> {
    Size::new(DevicePx(width), DevicePx(crate::gallery::HEIGHT))
}

/// What every step of the step phase recorded, one entry per step.
#[derive(Default)]
struct Steps {
    /// Wall time, in milliseconds.
    cost_ms: Vec<f64>,
    /// Nodes whose size or position was computed again.
    relaid: Vec<f64>,
    /// Paragraphs broken into lines again.
    rebroken: Vec<f64>,
    /// Text runs shaped, which a width step owes none of.
    shaped: Vec<f64>,
    /// Layout passes that started from the document root.
    roots: Vec<f64>,
    /// Batches the layout pool distributed.
    batches: Vec<f64>,
    /// Cached primitive ranges encoded from scratch.
    reencoded: Vec<f64>,
    /// Cached primitive ranges replayed instead.
    translated: Vec<f64>,
    /// Bytes copied into the persistent arenas.
    uploaded: Vec<f64>,
    /// Surface pixels redrawn.
    damage: Vec<f64>,
    /// Times the hit index was rebuilt wholesale.
    hit_rebuilds: Vec<f64>,
}

/// A counter delta as a sample.
#[expect(
    clippy::cast_precision_loss,
    reason = "per-step counter deltas, bounded by the document"
)]
fn sample(value: u64) -> f64 {
    value as f64
}

/// Runs the scenario.
#[expect(
    clippy::too_many_lines,
    reason = "two phases over one document, in the order the numbers are reported"
)]
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::harness(fixture::custom(SHEET, |cx| {
        Box::new(view! { Thread() }.into_view().build(cx)) as Box<dyn Anchor>
    }));
    quiet(&mut harness);
    let boxes = harness.app().windows()[0].layout().borrow().keys().len();
    let dump = std::env::var_os("ZGUI_BENCH_FRAMES").is_some();

    // One resize before the measurement, at a width outside the cycle below. The first resize a
    // freshly opened window takes carries one-time work no later step repeats — the measurement
    // is of a drag in progress, and this puts the window into one.
    harness.deliver_to_first(SurfaceEvent::Resized(wide(crate::gallery::WIDTH + 200.0)));
    harness.settle(64);
    let before = counter::snapshot();

    // The step phase: each step is a new width, and every counter delta below belongs to exactly
    // that step because the harness settled before the snapshot around it.
    let mut steps = Steps::default();
    for step in 0..REPEATS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a step index bounded by the repeat count"
        )]
        let width = crate::gallery::WIDTH + (step % CYCLE) as f32 * 8.0;
        let mark = counter::snapshot();
        let started = std::time::Instant::now();
        harness.deliver_to_first(SurfaceEvent::Resized(wide(width)));
        harness.settle(64);
        let cost = started.elapsed().as_secs_f64() * 1e3;
        let moved = mark.delta(&counter::snapshot());
        steps.cost_ms.push(cost);
        steps.relaid.push(sample(moved.nodes_relaid_out));
        steps.rebroken.push(sample(moved.text_rebroken));
        steps.shaped.push(sample(moved.text_shaped));
        steps.roots.push(sample(moved.layout_reached_root));
        steps.batches.push(sample(moved.layout_batches_distributed));
        steps.reencoded.push(sample(moved.chunks_reencoded));
        steps.translated.push(sample(moved.chunks_translated));
        steps.uploaded.push(sample(moved.chunk_bytes_uploaded));
        steps.damage.push(sample(moved.damage_px));
        steps.hit_rebuilds.push(sample(moved.hit_index_rebuilds));
        if dump {
            println!(
                "FRAME\t{step}\t{cost:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                moved.nodes_relaid_out,
                moved.text_rebroken,
                moved.text_shaped,
                moved.layout_reached_root,
                moved.layout_batches_distributed,
                moved.chunks_reencoded,
                moved.chunks_translated,
                moved.chunk_bytes_uploaded,
                moved.damage_px,
                moved.hit_index_rebuilds,
            );
        }
    }
    assert!(
        steps.rebroken.iter().all(|count| *count > 0.0),
        "a width step re-broke no paragraph at all, so the document is not the text-heavy one \
         this scenario exists to measure"
    );

    // The drag phase: the pacing bound, on this document, at one configure per millisecond. The
    // clock is held so the turns belong to this loop, and every configure sets the redraw flag as
    // a windowing backend does on its own account.
    let surface = harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui::platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window");
    harness
        .platform()
        .offscreens()
        .first()
        .expect("the application opened its window")
        .set_refresh_rate_millihertz(Some(DRAG_OUTPUT));
    harness.settle(8);
    harness.hold_clock(true);
    harness.redraw_on_configure(true);
    harness.advance(Duration::from_millis(100));
    let layouts_before = counter::get(Counter::LayoutReachedRoot);
    let declined_before = harness.app().windows()[0].declined_frames();
    let deferred_before = harness.app().windows()[0].deferred_resizes();
    let mut width = crate::gallery::WIDTH;
    for _ in 0..DRAG_MILLIS {
        harness.deliver(surface, SurfaceEvent::Resized(wide(width)));
        width += 1.0;
        harness.pump();
        harness.advance(Duration::from_millis(1));
    }
    // The tail: nothing else arrives, and the deadline the deferrals left behind is what brings
    // the window to the width the drag ended at.
    for _ in 0..32 {
        harness.advance(Duration::from_millis(1));
        harness.pump();
    }
    let drag_layouts = counter::get(Counter::LayoutReachedRoot) - layouts_before;
    let declined = harness.app().windows()[0].declined_frames() - declined_before;
    let deferred = harness.app().windows()[0].deferred_resizes() - deferred_before;

    let all = before.delta(&counter::snapshot());
    harness.assert_park_invariant();
    harness.shut_down();

    // One layout per refresh interval the drag spanned, plus the configure answered where it
    // arrived and the catch-up frame the tail is for.
    let interval = zgui::platform::refresh_interval(Some(DRAG_OUTPUT)).as_secs_f64();
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a ceiling in the tens, from a bounded drag length"
    )]
    let drag_ceiling = (DRAG_MILLIS as f64 / 1_000.0 / interval).ceil() as u64 + 2;

    let mut cost_us: Vec<f64> = steps.cost_ms.iter().map(|ms| ms * 1e3).collect();
    cost_us.sort_by(f64::total_cmp);
    let pace = Pace::of(&cost_us, 16_667.0);
    let cost = Spread::of(&mut steps.cost_ms);
    let relaid = Spread::of(&mut steps.relaid);
    let rebroken = Spread::of(&mut steps.rebroken);
    let reencoded = Spread::of(&mut steps.reencoded);
    let translated = Spread::of(&mut steps.translated);
    let uploaded = Spread::of(&mut steps.uploaded);
    let batches = Spread::of(&mut steps.batches);
    let roots = Spread::of(&mut steps.roots);
    let shaped_most = steps.shaped.iter().copied().fold(0.0, f64::max);
    let hit_most = steps.hit_rebuilds.iter().copied().fold(0.0, f64::max);

    Outcome {
        scenario: "thread-resize",
        document: format!(
            "{MESSAGES} messages, {} wrapped paragraphs, {boxes} boxes; {REPEATS} width steps \
             and a {DRAG_MILLIS}ms drag at 75 Hz",
            paragraphs()
        ),
        measurements: vec![
            Measurement {
                name: "thread.resize",
                unit: "ms",
                value: cost.p50,
                band: Band::Time {
                    baseline: 51.0,
                    tolerance: STARTUP_TOLERANCE,
                },
                rationale: "the measured on-screen p50 of one width step over the thread, which \
                            every paragraph in it currently pays into",
                budget: Some(8.0),
                spread: Some(cost),
            },
            Measurement {
                name: "thread.resize_p95",
                unit: "ms",
                value: cost.p95,
                band: Band::Time {
                    baseline: 53.0,
                    tolerance: STARTUP_TOLERANCE,
                },
                rationale: "the shoulder of the same distribution: a drag is judged by its worst \
                            steps, which is what a person calls stutter",
                budget: None,
                spread: Some(cost),
            },
            Measurement {
                name: "thread.rebreaks_per_step",
                unit: "paragraphs",
                value: rebroken.p50,
                band: Band::Count {
                    ceiling: paragraphs() as u64,
                },
                rationale: "a width step re-breaks the wrapped paragraphs and no more; the \
                            fixed-width header cells owe it nothing",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "thread.reshapes_per_step",
                unit: "runs",
                value: shaped_most,
                band: Band::Count { ceiling: 0 },
                rationale: "shaping is width-independent by design, so a resize that shapes is a \
                            cache key that grew a width in it",
                budget: Some(0.0),
                spread: None,
            },
            Measurement {
                name: "thread.relayouts_per_step",
                unit: "nodes",
                value: relaid.p50,
                band: Band::Count {
                    ceiling: (boxes as u64) * 2,
                },
                rationale: "every box may be re-asked at a new width, at most twice when a \
                            scrollbar decision flips; a third asking is a pass that ran again",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "thread.hit_rebuilds_per_step",
                unit: "rebuilds",
                value: hit_most,
                band: Band::Count { ceiling: 2 },
                rationale: "a full relayout earns one wholesale rebuild, two when the gutter \
                            fixpoint runs the pass again",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "thread.drag.layouts",
                unit: "layouts",
                value: sample(drag_layouts),
                band: Band::Count {
                    ceiling: drag_ceiling,
                },
                rationale: "a drag's layouts are bounded by elapsed time over the output's \
                            refresh interval, never by how many configures arrived",
                budget: None,
                spread: None,
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&all))
        .collect(),
        counters: counters(&all),
        notes: vec![
            format!(
                "one step relays out {:.0} nodes (p50) against {boxes} boxes and re-breaks {:.0} \
                 of {} paragraphs; {:.0} layout passes reached the root and the pool distributed \
                 {:.0} batches",
                relaid.p50,
                rebroken.p50,
                paragraphs(),
                roots.p50,
                batches.p50,
            ),
            format!(
                "paint answered a step with {:.0} re-encodes against {:.0} replays (p50), \
                 uploading {:.0} chunk bytes",
                reencoded.p50, translated.p50, uploaded.p50,
            ),
            format!(
                "the drag laid out {drag_layouts} times against a ceiling of {drag_ceiling}, \
                 deferring {deferred} configures and declining {declined} offered frames",
            ),
        ],
        pace,
    }
}
