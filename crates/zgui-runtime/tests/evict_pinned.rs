//! What a budget may never take, however hard it is driven.
//!
//! Eviction is forced on every frame here by a limit no working set can fit under, so every frame
//! the atlas frees everything it is permitted to free. What is left is exactly what this frame
//! looked up and what the live records hold — and the second of those is the half that did not
//! exist before, because a replayed range looks nothing up.
//!
//! The holds are checked in both directions. A record that never takes one is the defect; a record
//! that never gives one back is its mirror, an atlas in which nothing may ever be freed again, and
//! no pixel is ever wrong in either case.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use support::churn::{Churn, PER_TURN, moved};

/// Nothing a live record holds is ever freed, however aggressively the budget evicts.
#[test]
fn pinned_resources_survive_aggressive_eviction() {
    // Zero is unmeetable by construction, so the loop frees everything it may and then stops at
    // the frame's own working set. A window that actually reached this limit would have evicted
    // what it was drawing.
    let mut churn = Churn::open(Some(0));
    let before = zgui_profile::counter::snapshot();

    for turn in 0..60 {
        churn.turn();
        churn.assert_nothing_stale(turn);
        assert!(
            churn.window().content().resident_bytes() > 0,
            "turn {turn}: the atlas came back to zero bytes, which means it freed what it was \
             drawing"
        );
    }
    let forced = moved(&before);

    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    assert!(
        forced.atlas_tiles_evicted > 0,
        "eviction never ran, so nothing survived anything"
    );
    assert!(
        forced.record_tiles_retained > 0,
        "no record ever took ownership of a raster"
    );
    assert!(
        forced.record_tiles_released > 0,
        "no record ever gave one back, so the holds are one-way"
    );
    // Every turn replaces the paragraph completely, so all but the standing records' worth of
    // holds have been given back by now. The two totals cannot be equal — the records that still
    // stand are still holding what they draw — but the gap is bounded by what is on the page.
    let outstanding = forced
        .record_tiles_retained
        .saturating_sub(forced.record_tiles_released);
    assert!(
        outstanding <= u64::from(4 * PER_TURN),
        "{outstanding} holds outstanding after sixty turns, which is more than the records still \
         standing can account for: holds are being taken and not given back"
    );
}
