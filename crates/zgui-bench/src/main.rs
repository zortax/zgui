//! The measurement harness: the shipped component gallery, driven in process, with a band around
//! every number it reports.
//!
//! The real gallery — the very source files `crates/zgui-ui/examples/gallery` ships, included
//! through `#[path]` so the two cannot drift — driven against the headless platform with the real
//! cascade, the real layout, the real font engine and the real glyph rasteriser. Nothing here is a
//! fixture written to be measured, with the two exceptions the scenarios need and name.
//!
//! ```text
//! cargo run -p zgui-bench --release -- scenarios
//! ```
//!
//! runs the five end-to-end scenarios, stores the run under `docs/perf/runs/`, regenerates
//! `docs/performance.md`, and exits non-zero naming every number that left its band. That is what
//! `cargo xtask perf` runs and what the definition of done runs in turn.
//!
//! ```text
//! ZGUI_LATENCY=/tmp/t.jsonl cargo run -p zgui-bench --release -- <phase> <size> <repeats>
//! ```
//!
//! runs one *phase* instead: the exploratory measurements and differentials somebody reaches for
//! once a band has gone red, at one of seven document sizes — one section, a few, all thirteen, and
//! the same thirteen repeated two and three times over for a document past the shipped one.
//!
//! One probe row of four 34x34 swatches sits at the top of every size. Clicking a swatch is a
//! purely local change — one class on one element — and it is byte-for-byte the interaction
//! `crates/zgui/examples/pipeline_cpu.rs` measures on the 136-box `styled` gallery, so the cost of
//! *the same local change* can be read off against document size directly.
//!
//! `all` in place of a size runs the phase at every document size, smallest first, and stops at the
//! first that fails. The comparison phases are meant to be run that way: the smallest sizes are
//! where a differential is likeliest to be comparing nothing — a page with a handful of primitives
//! on it produces steps that draw no frame at all — and a sweep that starts at the shipped document
//! never visits them.
//!
//! # Where everything lives
//!
//! | Module | Contents |
//! |---|---|
//! | [`gallery`] | the document: the shipped gallery at seven sizes, the probe row, the sheet |
//! | [`growth`] | the live counts after ten ticks and after a thousand, and the band of zero |
//! | [`pace`] | frame intervals as a distribution, against the refresh they were driven at |
//! | [`draw`] | what a window draws through, and what records the display list it drew |
//! | [`inspect`] | what a run reads back off a window — its size, its scroll, where a swatch is |
//! | [`input`] | one event, as the platform would have delivered it |
//! | [`drive`] | opening a window, and running one phase at every document size |
//! | [`phase`] | the phases themselves, in three groups |
//! | [`scenario`] | the five end-to-end scenarios and the ratchet around them |
//! | [`script`] | the 42-step gallery script the differentials are driven by |
//! | [`stats`] | what a ledger of interactions reports |
//! | [`verify`] | the transcript and picture differentials |
//!
//! The three reference workloads are not here at all. Each is its own binary under `src/bin`,
//! because each mounts a document of its own and a document mounted on a thread that has already
//! mounted another is a document whose caches are somebody else's.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    reason = "the gallery's own source is included whole, and this harness drives the parts of it \
              a document size is made of rather than the shell's own chrome"
)]
#![allow(
    clippy::too_many_lines,
    reason = "the sizes are literal view bodies on purpose: a document size that is generated is a \
              document size nobody can read off the page"
)]

#[path = "../../zgui-ui/examples/gallery/section/mod.rs"]
#[allow(
    unused_imports,
    reason = "the gallery's sections are one module; the ladder below mounts the ones it is sized by"
)]
mod section;
#[path = "../../zgui-ui/examples/gallery/shell.rs"]
mod shell;

mod draw;
mod drive;
mod gallery;
mod growth;
mod input;
mod inspect;
mod pace;
mod phase;
mod resize;
mod scenario;
mod script;
mod stats;
mod verify;

use zgui::geom::{CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui_platform_headless::Harness;

use crate::draw::mounted_recorder;
use crate::drive::every_size;
use crate::gallery::{HEIGHT, STILL, WIDTH, mounted_scheme, runtime};
use crate::inspect::{document, swatch_centres};

fn main() {
    let phase = std::env::args().nth(1).unwrap_or_else(|| "click".into());
    let size = std::env::args().nth(2).unwrap_or_else(|| "s13".into());
    let repeats: usize = std::env::args()
        .nth(3)
        .and_then(|n| n.parse().ok())
        .unwrap_or(24);

    // The ratchet, and the one scenario it is made of. Both come before the phase dispatch because
    // neither takes a document size: a scenario decides its own document, which is half of what
    // makes one run comparable to the next.
    if phase == "scenarios" {
        match crate::scenario::write() {
            Ok(()) => println!("PERF ok: every measurement inside its band"),
            Err(over) => {
                for line in &over {
                    eprintln!("PERF REGRESSION {line}");
                }
                eprintln!(
                    "PERF failed: {} measurements outside their bands",
                    over.len()
                );
                std::process::exit(1);
            }
        }
        return;
    }
    // The growth check, which takes no document size from the caller because the band it enforces
    // is about the reference workload rather than about whichever size somebody asked for.
    if phase == "growth" {
        let outcome = crate::growth::run(&size);
        if crate::growth::report(&outcome) {
            println!("GROWTH ok: every live count is flat");
            return;
        }
        std::process::exit(1);
    }
    // Pacing, which is a report and not a gate: it exits non-zero only when it may not publish at
    // all, never because a number it took was a bad one.
    if phase == "pacing" {
        let seconds = std::env::args()
            .nth(3)
            .and_then(|n| n.parse().ok())
            .unwrap_or(60.0);
        let script = crate::pace::Script {
            size: size.clone(),
            seconds,
        };
        if !crate::pace::run(&script) {
            std::process::exit(1);
        }
        return;
    }
    if phase == "scenario" {
        zgui_profile::latency::start_epoch();
        let outcome = crate::scenario::run(&size);
        crate::scenario::print(&outcome);
        zgui_profile::latency::flush();
        // Deliberately not a failure here, however far outside its band a number landed. The
        // verdict belongs to the sweep, which has to store the run and regenerate the document
        // *before* it fails — and a regressed run is the one whose numbers somebody most wants.
        return;
    }

    if size == "all" {
        every_size(&phase, repeats);
        return;
    }

    // The picture differential is two photographs of one state taken a little apart in time, so the
    // document it compares is mounted with its endless animations stopped. See [`STILL_SHEET`].
    STILL.with(|still| still.set(phase == "pixels"));

    zgui_profile::latency::start_epoch();

    let mut harness = Harness::new(runtime(&size));
    let live_scheme = mounted_scheme();
    let live_full = mounted_recorder();
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(256);

    let (boxes, fragments) = document(&harness.app().windows()[0]);
    let centres = swatch_centres(&harness.app().windows()[0]);
    assert_eq!(centres.len(), 4, "the four probe swatches were found");
    let middle = Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0));
    let away = Point::new(CssPx(4.0), CssPx(4.0));

    harness.reset_counts();
    zgui_profile::latency::flush();
    let mark = crate::stats::Boundary::here();

    let mut driver = phase::Driver {
        harness,
        scheme: live_scheme,
        full: live_full,
        centres,
        middle,
        away,
        boxes,
        fragments,
        size: size.clone(),
        repeats,
        // What the user feels: one input event and every frame the loop ran before it went quiet.
        ledger: crate::stats::Ledger::new(&phase),
        // The second half of an interaction that has one: the frames a glide carries after the
        // notch, the release after the press.
        ticks: crate::stats::Ledger::new(&format!("{phase}-tick")),
    };

    let started = std::time::Instant::now();
    let frames = phase::run(&mut driver, &phase);
    let elapsed = started.elapsed();

    zgui_profile::latency::flush();
    println!(
        "size={size} boxes={boxes} fragments={fragments} phase={phase} repeats={repeats} \
         frames={frames} wall_ms={:.4} per_frame_ms={:.4}",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / frames.max(1) as f64
    );
    driver.ledger.report(&size, boxes);
    driver.ticks.report(&size, boxes);
    crate::stats::report(&size, &phase, boxes, fragments, mark);
}
