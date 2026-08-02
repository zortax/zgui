//! Whether a stated budget is actually enforced, and enforced without thrashing.
//!
//! The two halves are one claim. Resident bytes coming back under the limit says the budget is
//! enforced at all; the rebuild fraction says it is enforced usefully — a policy that freed the
//! working set on every frame would hold any limit there is and would rasterise the whole page
//! again on every one of them.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use support::churn::{Churn, PER_TURN, moved};

/// The budget the window is held to: two of the atlas's monochrome textures.
const SOFT: u64 = 2 * 1024 * 1024;

/// How many turns of two hundred and fifty fresh characters the window is driven through.
const TURNS: u32 = 200;

/// How many turns in a row the atlas may sit over its limit before that is a budget not enforced.
///
/// Not zero, and it cannot be. Texture memory comes back only when a whole texture empties, and
/// the tiles inside one belong to whichever generations happened to allocate there — so a step
/// down one generation can free a great many tiles and no bytes at all, and the next step is the
/// next frame. What must not happen is the level being exceeded and *staying* exceeded, which is a
/// budget that is stated and not kept.
const GRACE: u32 = 4;

/// Fifty thousand distinct glyphs pass through and resident bytes stay under the stated limit.
#[test]
fn atlas_evicts_when_over_soft_limit() {
    let mut churn = Churn::open(Some(SOFT));

    let mut run = 0;
    let mut longest = 0;
    for _ in 0..TURNS {
        churn.turn();
        run = if churn.window().content().resident_bytes() > SOFT {
            run + 1
        } else {
            0
        };
        longest = longest.max(run);
    }
    assert_eq!(
        churn.shown(),
        TURNS * PER_TURN,
        "the run is specified at fifty thousand distinct rasterisations"
    );
    assert!(
        longest <= GRACE,
        "resident bytes stayed over the soft limit for {longest} turns in a row, which is a budget          stated and not kept rather than a texture waiting to empty"
    );

    // Over a window rather than over the run. A total only ever grows, so a session that thrashed
    // for its first second and behaved for the next hour is indistinguishable from one that
    // thrashed throughout.
    let before = zgui_profile::counter::snapshot();
    for _ in 0..60 {
        churn.turn();
    }
    let window = moved(&before);

    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    assert!(window.glyphs_placed > 0, "the window drew no text at all");
    assert!(
        window.atlas_tiles_evicted > 0,
        "nothing was freed over the whole window, so the limit was never enforced and the bound \
         above says only that the fixture is small"
    );
    assert!(
        window.rebuilt_after_eviction * 20 < window.glyphs_placed,
        "over a sixty-frame window {} of {} placements were rasters made again after being freed, \
         which is past the twentieth part that separates a cache filling up from one thrashing",
        window.rebuilt_after_eviction,
        window.glyphs_placed,
    );
}
