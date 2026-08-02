//! The assertion that catches a replayed range drawing a raster that was freed underneath it.
//!
//! A fragment that has not changed is drawn by copying last frame's operations forward. Nothing on
//! that path looks a glyph up — the instances already carry the rectangle of the texture the pixels
//! are in — so to the atlas holding those pixels, a glyph drawn by a replay is a glyph this frame
//! did not draw. A budget that frees the coldest content therefore frees exactly the tiles a
//! static label is replaying, hands their rectangles to whatever is rasterised next, and the label
//! goes on drawing the rectangles: not a blank glyph, but whichever glyph took the space.
//!
//! Nothing else this project checks can see it. The display list says what it said last frame, the
//! geometry never moved, the damage is right, and a picture comparison notices only in the run
//! where the freed rectangle happens to be reallocated and refilled while the label is still on
//! the screen. So the assertion is over the *dependency*.
//!
//! The counters are a process-wide block, so this is one test in a target of its own — two of them
//! running beside each other would each be reading the other's frames.

mod support;

use support::churn::{Churn, moved};

/// A replayed range never names a raster the atlas has freed.
#[test]
fn no_replayed_range_names_an_evicted_tile() {
    // Two mebibytes is two of the atlas's monochrome textures, which a few thousand glyphs fill,
    // so the budget fires within the first handful of turns and keeps firing.
    let mut churn = Churn::open(Some(2 * 1024 * 1024));
    let before = zgui_profile::counter::snapshot();
    for turn in 0..80 {
        churn.turn();
        churn.assert_nothing_stale(turn);
    }
    let under_budget = moved(&before);

    // The control, and it has to be a second run rather than an earlier part of this one: what it
    // establishes is that the assertion above was made about a window where something *was* at
    // risk. A window that evicted nothing satisfies it trivially, and so does one that replayed
    // nothing.
    let mut control = Churn::open(None);
    let before = zgui_profile::counter::snapshot();
    for _ in 0..20 {
        control.turn();
    }
    let unbudgeted = moved(&before);

    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    assert!(
        under_budget.atlas_tiles_evicted > 0,
        "the budget never freed anything, so nothing was ever at risk"
    );
    assert!(
        under_budget.chunks_translated > 0,
        "nothing was replayed, so the assertion never had a range to check"
    );
    assert!(
        under_budget.record_tiles_retained > 0,
        "no record took ownership of a raster, so nothing was protecting anything"
    );
    assert!(
        unbudgeted.glyphs_placed > 0,
        "the control window drew no text, so it is a control for nothing"
    );
    assert_eq!(
        unbudgeted.atlas_tiles_evicted, 0,
        "a window that stated no budget freed content anyway, so eviction is not driven by the \
         limit at all"
    );
}
