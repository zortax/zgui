//! The vector scenarios: what drawings cost on each route a shape can take.
//!
//! ```text
//! cargo run -p zgui-bench --release -- vector <scenario|all> [variant]
//! ZGUI_BENCH_GPU=1 cargo run -p zgui-bench --release -- vector all
//! ```
//!
//! A report and not a gate: nothing here has a band, and nothing is written to `docs/`. Each
//! (scenario, variant) pair runs in a process of its own, because the general rasteriser is built
//! at most once per renderer and a document is mounted through thread-local state.
//!
//! The renderer-specific counters (`vector_backend_built`, `vello_renders` and the encode counts)
//! move only under `ZGUI_BENCH_GPU=1`. Without it the renderer draws nothing, reports its vector
//! rasteriser as never built, and paint sees the general route as cold.
//!
//! One block per variant:
//!
//! ```text
//! VECTOR <scenario> <variant> <stretch> frames=N paint_p50=.. paint_p95=.. render_p50=.. render_p95=..
//! VCOUNT <scenario> <variant> <stretch> <field>=<total> ...
//! VLIVE  <scenario> <variant> <stretch> atlas_entries_live=<start>-><end> vector_mask_tiles_live=<start>-><end>
//! ```

mod icons;
mod map;
mod scatter;
mod svg;
mod timing;

use std::time::Duration;

use zgui::geom::{Css, CssPx, Point};
use zgui::runtime::Runtime;
use zgui::vocab::{PointerAction, PointerButton};
use zgui_platform_headless::Harness;
use zgui_profile::{Counter, Counters};

use crate::scenario::vector::timing::Frames;

/// Every (scenario, variant) pair, in the order `all` runs them.
pub(crate) const ALL: [(&str, &str); 13] = [
    ("scatter-pan", "mask-512"),
    ("scatter-pan", "mask-2k"),
    ("scatter-pan", "markers-200"),
    ("scatter-pan", "10k"),
    ("scatter-pan", "100k"),
    ("scatter-pan", "waves"),
    ("scatter-pan", "series-100k"),
    ("scatter-pan", "series-1m"),
    ("scatter-pan", "waves-view"),
    ("icons-scroll", "icons"),
    ("icons-scroll", "gallery"),
    ("svg-static", "grid"),
    ("map-pan", "map"),
];

/// One refresh at 120 Hz.
pub(super) const TICK: Duration = Duration::from_micros(8_333);

/// The counters a `VCOUNT` line prints, in order.
const PRINTED: [Counter; 20] = [
    Counter::VectorBackendBuilt,
    Counter::VelloRenders,
    Counter::VectorEncodeHits,
    Counter::VectorEncodeMisses,
    Counter::VectorRouteMask,
    Counter::VectorRouteGeneral,
    Counter::VectorRouteAnalytic,
    Counter::VectorRouteMarks,
    Counter::MarksUnionBins,
    Counter::MarksPayloadBytes,
    Counter::SeriesPayloadsBuilt,
    Counter::VectorMaskMisses,
    Counter::VectorMaskBudgetOverflow,
    Counter::VectorReplaysMoved,
    Counter::VectorTierChanges,
    Counter::VelloPasses,
    Counter::AtlasTilesEvicted,
    Counter::RebuiltAfterEviction,
    Counter::ChunksReencoded,
    Counter::ChunksTranslated,
];

/// Runs `scenario` at `variant`, or every pair when `scenario` is `all`.
///
/// # Panics
///
/// Panics on an unknown pair, and when a spawned pair does not finish.
pub(crate) fn main(scenario: &str, variant: Option<&str>) {
    if scenario == "all" {
        let binary = std::env::current_exe().expect("the running binary can be named");
        for (scenario, variant) in ALL {
            println!("== vector {scenario} {variant}");
            let status = std::process::Command::new(&binary)
                .args(["vector", scenario, variant])
                .status()
                .expect("the sweep can run this binary again");
            assert!(
                status.success(),
                "vector {scenario} {variant} failed: {status}"
            );
        }
        return;
    }
    let variants: Vec<&str> = match variant {
        Some(variant) => vec![variant],
        None => ALL
            .iter()
            .filter(|(name, _)| *name == scenario)
            .map(|(_, variant)| *variant)
            .collect(),
    };
    assert!(
        !variants.is_empty(),
        "unknown vector scenario `{scenario}`; one of {ALL:?}"
    );
    timing::start();
    for variant in variants {
        match scenario {
            "scatter-pan" => scatter::run(variant),
            "icons-scroll" => icons::run(variant),
            "svg-static" => svg::run(variant),
            "map-pan" => map::run(variant),
            other => panic!("unknown vector scenario `{other}`; one of {ALL:?}"),
        }
    }
}

/// Opens a window over `runtime` and drives it to rest.
///
/// The counters are not reset: a live count keeps the value the last frame published, and the
/// stretch reads its start from there.
pub(super) fn opened(runtime: Runtime) -> Harness<Runtime> {
    let mut harness = crate::drive::harness(runtime);
    settle(&mut harness);
    harness
}

/// Drives `harness` until it stops asking for frames, with a few refreshes in between.
pub(super) fn settle(harness: &mut Harness<Runtime>) {
    harness.settle(256);
    for _ in 0..8 {
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(256);
}

/// One measured stretch of one variant.
pub(super) struct Stretch {
    /// Which scenario.
    scenario: &'static str,
    /// Which variant.
    variant: String,
    /// Which stretch.
    name: &'static str,
    /// The counters when the stretch began.
    before: Counters,
    /// The frames it drew.
    frames: Frames,
}

impl Stretch {
    /// Starts a stretch now.
    pub(super) fn begin(scenario: &'static str, variant: &str, name: &'static str) -> Self {
        Self {
            scenario,
            variant: variant.to_owned(),
            name,
            before: zgui_profile::counter::snapshot(),
            frames: Frames::default(),
        }
    }

    /// One refresh: advances the clock, runs whatever frames are due and samples them.
    pub(super) fn tick(&mut self, harness: &mut Harness<Runtime>) {
        zgui_profile::latency::clear();
        harness.advance(TICK);
        harness.pump();
        self.frames.read(&zgui_profile::latency::recent());
    }

    /// Ends the stretch and prints its block.
    pub(super) fn end(mut self) {
        let after = zgui_profile::counter::snapshot();
        let moved = self.before.delta(&after);
        let (scenario, variant, name) = (self.scenario, &self.variant, self.name);
        let (paint, render) = self.frames.spreads();
        println!(
            "VECTOR {scenario} {variant} {name} frames={} paint_p50={:.3} paint_p95={:.3} \
             render_p50={:.3} render_p95={:.3}",
            self.frames.len(),
            paint.0,
            paint.1,
            render.0,
            render.1,
        );
        let fields: Vec<String> = PRINTED
            .iter()
            .map(|counter| format!("{}={}", counter.name(), moved.get(*counter)))
            .collect();
        println!("VCOUNT {scenario} {variant} {name} {}", fields.join(" "));
        let live = |counter: Counter| {
            format!(
                "{}={}->{}",
                counter.name(),
                self.before.get(counter),
                after.get(counter)
            )
        };
        println!(
            "VLIVE  {scenario} {variant} {name} {} {}",
            live(Counter::AtlasEntriesLive),
            live(Counter::VectorMaskTilesLive),
        );
    }
}

/// A mouse button event at `at`.
pub(super) fn button(action: PointerAction, at: Point<CssPx, Css>) -> zgui::platform::SurfaceEvent {
    zgui::platform::SurfaceEvent::Pointer {
        action,
        event: zgui::vocab::PointerEvent::mouse(at).with_button(PointerButton::Primary),
        modifiers: zgui::vocab::Modifiers::NONE,
        timestamp: zgui::vocab::Timestamp::ORIGIN,
    }
}

/// Drags from `from` to the right by `step` CSS pixels a tick for `ticks` ticks, measured as one
/// stretch. The press and the release are outside it.
pub(super) fn drag(
    harness: &mut Harness<Runtime>,
    stretch: (&'static str, &str, &'static str),
    from: Point<CssPx, Css>,
    step: f32,
    ticks: usize,
) {
    harness.deliver_to_first(button(PointerAction::Pressed, from));
    harness.settle(64);
    let (scenario, variant, name) = stretch;
    let mut measured = Stretch::begin(scenario, variant, name);
    let mut at = from;
    for _ in 0..ticks {
        at.x.0 += step;
        harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, at));
        measured.tick(harness);
    }
    measured.end();
    harness.deliver_to_first(button(PointerAction::Released, at));
    settle(harness);
}

/// Scrolls the port under `at` for `ticks` ticks, a six-line notch every fortieth, as one stretch.
pub(super) fn scroll(
    harness: &mut Harness<Runtime>,
    stretch: (&'static str, &str, &'static str),
    at: Point<CssPx, Css>,
    ticks: usize,
) {
    harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, at));
    settle(harness);
    let (scenario, variant, name) = stretch;
    let mut measured = Stretch::begin(scenario, variant, name);
    for tick in 0..ticks {
        if tick % 40 == 0 {
            harness.deliver_to_first(crate::input::wheel(at, 6.0));
        }
        measured.tick(harness);
    }
    measured.end();
    settle(harness);
}

/// A seeded linear congruential generator: the same document every run.
pub(super) struct Lcg(u64);

impl Lcg {
    /// A generator starting from `seed`.
    pub(super) const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next value in `0.0..1.0`.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the top 53 bits are exactly representable"
    )]
    pub(super) fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
}
