//! Wall-clock budgets for the path from an input event to a finished frame.
//!
//! Every case here drives a real application — a real cascade, a real box tree, real layout, the
//! real system font engine and the real glyph rasteriser — against the headless platform, and times
//! what a person waits for: the whole frame, from the event being delivered to the display list
//! being handed to a renderer. Only the renderer is a stub, and only so that the number is this
//! framework's own rather than a graphics driver's.
//!
//! # Why a time *and* a count
//!
//! A time alone measures the machine. Every case here therefore also asserts a count that is the
//! same number on a fast machine and a slow one — how many glyphs were rasterised, how many frames
//! ran, how large the damaged region was — so that the regression the case was written for fails it
//! exactly, and the budget is left to catch the slowdowns nothing counts.
//!
//! The counts are the sharper half. The defects these cases exist for were all of the same kind: a
//! stage doing the whole document's work for one element's change. That shows up as a count long
//! before it shows up as a time, and it shows up identically wherever the suite runs.
//!
//! # How a budget is arrived at
//!
//! Measured, never chosen. Each case runs its interaction many times and divides, which is the
//! number a person's finger is waiting on. The budget is then set several times above that so the
//! spread between runs on a loaded machine cannot reach it — a budget that is red one afternoon in
//! three is one nobody reads — and each case states what it measured and what it was set to.

mod support;

use std::time::{Duration, Instant};

use support::Gallery;
use zgui_profile::{Counter, counter};

/// How many times each interaction is repeated before the average is believed.
const REPEATS: usize = 32;

/// What one click is allowed to cost, from the button going down to the frame being finished.
///
/// Measured: 0.058 ms per click on the development machine, over a 136-box document with 28 lines
/// of text in it. Set to 3 ms, which is fifty times that — the budget is not trying to detect a
/// machine two times slower, it is trying to detect the frame going back to rebuilding the whole
/// box tree, which was 36 ms.
const CLICK: Duration = Duration::from_micros(3_000);

/// What one hover on and off is allowed to cost.
///
/// Measured: 0.066 ms per hover pair. Set to 3 ms for the same reason.
const HOVER: Duration = Duration::from_micros(3_000);

/// What one resize step is allowed to cost, event to finished frame.
///
/// Measured: 0.78 ms per step, which is a full relayout of every box in the document — a resize is
/// the one interaction that genuinely invalidates everything. Set to 8 ms: a step that misses 60 Hz
/// is a step the drag visibly lags behind, and 36 ms is what it cost when every glyph in the
/// document was rasterised again on every frame.
const RESIZE: Duration = Duration::from_millis(8);

/// What the counters read for one interaction, and how long it took.
struct Measured {
    /// How many glyphs were turned into pixels.
    rasterised: u64,
    /// How many glyphs one full repaint of the document places.
    ///
    /// The scale every other number here is read against, and the anti-vacuity check: a machine
    /// with no fonts installed lays out no text, rasterises nothing, and would satisfy every
    /// assertion about rasterisation without the path those assertions are about having run.
    document: u64,
    /// How many glyphs the interaction placed.
    placed: u64,
    /// How many frames ran.
    frames: u64,
    /// How many primitives reached the display list.
    ///
    /// The proof that the interaction did something. Every count here is a count of work *not*
    /// done, and a script that clicked nothing would satisfy all of them.
    primitives: u64,
    /// How long the whole interaction took.
    elapsed: Duration,
}

impl Measured {
    /// The time one repeat of the interaction took.
    fn each(&self) -> Duration {
        self.elapsed / u32::try_from(REPEATS).expect("the repeat count fits")
    }

    /// Fails unless the document this was measured against has text in it.
    fn drew_text(&self) {
        assert!(
            self.document > 100,
            "the document's first paint placed {} glyphs, so nothing here measured the glyph path",
            self.document
        );
    }
}

/// Runs `interaction` against a settled gallery and reports what it cost.
fn measure(interaction: impl FnOnce(&mut Gallery)) -> Measured {
    let mut gallery = Gallery::open();
    gallery.settle();
    // One full repaint, measured before the interaction is: every count below is meaningful only
    // beside it, since "no glyph was drawn" is also what a document with no text in it reports.
    // A resize step is how a full repaint is asked for, and the step back restores the extent the
    // interaction is then measured at.
    counter::reset();
    gallery.resize_step(1);
    let document = counter::get(Counter::GlyphsPlaced);
    gallery.resize_step(0);
    gallery.settle();
    counter::reset();
    let started = Instant::now();
    interaction(&mut gallery);
    let elapsed = started.elapsed();
    let measured = Measured {
        rasterised: counter::get(Counter::GlyphsRasterised),
        document,
        placed: counter::get(Counter::GlyphsPlaced),
        frames: gallery.frames(),
        primitives: counter::get(Counter::PrimitivesEmitted),
        elapsed,
    };
    gallery.shut_down();
    measured
}

#[test]
fn a_click_rasterises_no_glyph_and_stays_far_inside_one_frame() {
    // Clicking a swatch toggles one class, which changes one border colour and one background
    // colour. The engine's own predicate calls that a relayout, because the layout it was written
    // for keeps painting fragments inside its boxes; taking its word for it rebuilt all 136 boxes,
    // lost every layout cache, renamed every fragment, widened damage to the whole surface and so
    // re-placed all 483 glyphs on the page — 36 ms, for a colour.
    //
    // Timed: the whole interaction, pointer move to press to release, each settled to a finished
    // frame. That is the thing a person is waiting for and it is the only honest boundary; timing
    // the paint stage alone is what let this defect through in the first place.
    let measured = measure(|gallery| {
        for index in 0..REPEATS {
            gallery.click_swatch(index % Gallery::SWATCHES);
        }
    });
    measured.drew_text();
    assert!(
        measured.primitives >= u64::try_from(REPEATS).expect("it fits"),
        "{REPEATS} clicks emitted {} primitives; a click that changed nothing is not the \
         interaction this is a budget for",
        measured.primitives
    );
    // The sharp assertion, and the one that is the same number on every machine: a click that
    // damages the swatch it landed on reaches no text at all. Rebuilding the box tree renames every
    // fragment, which widens damage to the whole surface, which places all of the document's
    // glyphs again — so this reads zero exactly when the classification is right and the whole
    // document when it is not.
    assert_eq!(
        measured.placed, 0,
        "a click placed {} glyphs; the document has {} and none of them is inside a swatch",
        measured.placed, measured.document
    );
    assert_eq!(
        measured.rasterised, 0,
        "a click rasterised {} glyphs; the page's text did not change, so every one of them was \
         already in the atlas",
        measured.rasterised
    );
    assert!(
        measured.each() < CLICK,
        "a click cost {:?}, and the budget is {CLICK:?}",
        measured.each()
    );
}

#[test]
fn a_click_runs_one_frame_for_each_event_and_not_two() {
    // A frame drains its events, flushes its reactive graph and then styles, lays out and paints —
    // so the mutation a handler makes during a frame is shown by that same frame. The flag it
    // raised used to still be standing at the end of it, and every interaction bought a second
    // frame that damaged nothing and presented a surface identical to the one just presented.
    let measured = measure(|gallery| {
        for index in 0..REPEATS {
            gallery.click_swatch(index % Gallery::SWATCHES);
        }
    });
    let per_click = measured.frames / u64::try_from(REPEATS).expect("the repeat count fits");
    assert_eq!(
        per_click, 3,
        "a click delivers three events — move, press, release — and each is shown by one frame; \
         this ran {} frames for {REPEATS} clicks",
        measured.frames
    );
}

#[test]
fn a_hover_rasterises_no_glyph_and_stays_far_inside_one_frame() {
    // The control the click case is measured against: `:hover` changes a background colour through
    // exactly the same cascade, and it was always fast — because a hover does not toggle a class
    // that changes a border colour, and border colours were what the engine called a relayout.
    let measured = measure(|gallery| {
        for index in 0..REPEATS {
            gallery.hover_swatch(index % Gallery::SWATCHES);
            gallery.hover_away();
        }
    });
    measured.drew_text();
    assert!(
        measured.primitives >= u64::try_from(REPEATS).expect("it fits"),
        "{REPEATS} hovers emitted {} primitives; a pointer that reached nothing is not the \
         interaction this is a budget for",
        measured.primitives
    );
    assert_eq!(
        measured.placed, 0,
        "a hover placed {} glyphs; it damages the swatch and no text is inside one",
        measured.placed
    );
    assert_eq!(measured.rasterised, 0, "the page's text did not change");
    assert!(
        measured.each() < HOVER,
        "a hover cost {:?}, and the budget is {HOVER:?}",
        measured.each()
    );
}

#[test]
fn a_resize_repaints_the_whole_document_without_rasterising_it_again() {
    // The defect this is the regression test for is not that the glyph cache missed. It is that
    // there was no cache to miss: the tile was in the atlas and *where its pixels go* was not, so
    // every full repaint rasterised every glyph to learn a number it had already computed — and the
    // glyphs that rasterise to nothing at all, the spaces, were rasterised again for ever because
    // nothing was ever inserted for them to be found.
    //
    // A resize damages the whole surface by construction, so this is the case where a repaint of
    // unchanged text is unavoidable and its cost has to be zero.
    let measured = measure(|gallery| {
        for step in 0..REPEATS {
            gallery.resize_step(step);
        }
    });
    measured.drew_text();
    assert!(
        measured.placed >= measured.document * u64::try_from(REPEATS / 2).expect("it fits"),
        "a resize damages the whole surface, so every step places the whole document's glyphs          again; {} placed over {REPEATS} steps of a {} glyph document is not that",
        measured.placed,
        measured.document
    );
    // Not zero: a step changes the width, so a line breaks in a new place and a few glyphs land at
    // a subpixel phase nothing has drawn them at yet. What must never come back is the whole
    // document being rasterised again, which is what a tenth of one document over thirty-two full
    // repaints is far below.
    assert!(
        measured.rasterised * 10 < measured.document,
        "{} of the document's {} glyphs were rasterised again over {REPEATS} resize steps",
        measured.rasterised,
        measured.document
    );
    assert!(
        measured.each() < RESIZE,
        "a resize step cost {:?}, and the budget is {RESIZE:?}",
        measured.each()
    );
}

#[test]
fn a_resize_that_repeats_the_extent_runs_no_frame_at_all() {
    // A drag delivers the same extent more than once, and every repeat used to rebuild the
    // swapchain — which waits for the device to go idle — and then repaint the whole surface.
    let measured = measure(|gallery| {
        gallery.resize_step(1);
        for _ in 0..REPEATS {
            gallery.resize_step(1);
        }
    });
    assert_eq!(
        measured.frames, 1,
        "the extent moved once and {REPEATS} events repeated it; only the move is a frame"
    );
}

#[test]
fn a_settled_application_runs_no_frames_at_all() {
    // The park policy, which has been broken four times: nothing is animating, nothing is pending,
    // and the loop must sit still. A frame here is the whole pipeline running for ever behind a
    // window nobody is touching.
    let measured = measure(|gallery| {
        for _ in 0..REPEATS {
            gallery.idle_tick();
        }
    });
    assert_eq!(measured.frames, 0, "an idle application drew something");
}
