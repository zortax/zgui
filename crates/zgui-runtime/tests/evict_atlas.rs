//! Whether the compositor's atlas budget is enforced, and enforced without thrashing.
//!
//! The two halves are one claim. The device's atlas pages staying near the limit says the budget
//! is enforced at all, and the rasterisation count says it is enforced usefully: a policy that
//! freed the working set on every frame would hold any limit there is and would rasterise the
//! whole page again on every one of them.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use support::churn::{Churn, PER_TURN, moved};

/// One of the compositor's 2048 by 2048 single-channel atlas pages.
const PAGE: u64 = 4 * 1024 * 1024;

/// The limit the compositor's atlas is held to: two pages.
const SOFT: u64 = 2 * PAGE;

/// How far over the limit the device may hold atlas memory.
///
/// A frame evicts before it rasterises, so the glyphs it adds can open one more page. The device
/// also counts the mask buffer beside the pages, which is a few bytes here.
const OVER: u64 = PAGE + 4096;

/// How many turns of two hundred and fifty fresh characters the window is driven through.
const TURNS: u32 = 200;

/// Fifty thousand distinct glyphs pass through, and the device's atlas pages stay within a page of
/// the limit, where they grow far past it without one.
#[test]
fn the_compositor_atlas_stays_under_its_limit_without_thrashing() {
    // The control: the same churn with no limit holds more than the limit, so the bound below is
    // about the budget and not about a fixture too small to reach it.
    let Some(mut unbounded) = Churn::open(None) else {
        return;
    };
    for _ in 0..TURNS {
        unbounded.turn();
    }
    let grown = unbounded.atlas_bytes();
    assert!(
        grown > SOFT + OVER,
        "fifty thousand glyphs grew the atlas to only {grown} bytes, so the limit below is never \
         reached"
    );
    unbounded.harness.shut_down();

    let Some(mut churn) = Churn::open(Some(SOFT)) else {
        return;
    };
    let mut highest = 0;
    for _ in 0..TURNS {
        churn.turn();
        highest = highest.max(churn.atlas_bytes());
    }
    assert_eq!(
        churn.shown(),
        TURNS * PER_TURN,
        "the run is specified at fifty thousand distinct rasterisations"
    );
    assert!(
        highest <= SOFT + OVER,
        "the device held {highest} bytes of atlas pages under a limit of {SOFT}"
    );

    // Over a window rather than over the run. A total only ever grows, so a session that thrashed
    // for its first second and behaved for the next hour is indistinguishable from one that
    // thrashed throughout.
    let before = zgui_profile::counter::snapshot();
    let turns = 60;
    for _ in 0..turns {
        churn.turn();
    }
    let window = moved(&before);
    let fresh = u64::from(turns * PER_TURN);
    assert!(
        window.glyphs_rasterised >= fresh,
        "the window rasterised {} glyphs for {fresh} it had never drawn",
        window.glyphs_rasterised
    );
    assert!(
        window.glyphs_rasterised * 20 < fresh * 21,
        "over a sixty-turn window the compositor rasterised {} glyphs for {fresh} fresh ones, which \
         is past the twentieth part that separates a cache filling up from one thrashing",
        window.glyphs_rasterised,
    );
    churn.harness.shut_down();
}
