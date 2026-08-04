//! Invalidating a box's layout, and stopping as soon as it is redundant.

use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};

use crate::tree::store::LayoutStore;
use crate::tree::store::state::BoxLayout;

/// Invalidates a box's layout and every ancestor's, stopping at the first already-invalid ancestor.
///
/// The early stop is what makes marking `n` boxes cost `O(n + depth)` rather than `O(n × depth)`:
/// an ancestor that is already invalid has already invalidated everything above it, so walking past
/// it can only repeat work that has been done.
///
/// A box that has never been laid out at all is not an already-invalid one, and the walk does not
/// stop at it: its ancestors were computed against content it contributed to and are still holding
/// results. Whole classes of box are never laid out on their own — a run of text is sized by the
/// line box above it and never asked for a size of its own — and a change to one of those has to
/// reach the box that *was* asked.
///
/// Returns how many boxes this call invalidated, which is one plus the number of ancestors it
/// reached before stopping.
pub fn mark_dirty(store: &mut LayoutStore, box_: BoxKey) -> u32 {
    let mut marked = 0;
    let mut next = Some(box_);
    while let Some(key) = next {
        if !store.contains(key) {
            break;
        }
        if store
            .state(key)
            .is_some_and(|state| state.holds_no_layout() && !never_laid_out(state))
        {
            break;
        }
        store.state_mut(key).forget_layout();
        marked += 1;
        next = store.node(key).parent;
    }
    marked
}

/// Whether a box has no result of its own, as opposed to a result that was thrown away.
///
/// Throwing a result away empties the cache and leaves the geometry behind, so geometry at its
/// default with an empty cache is a box no algorithm ever produced anything for.
fn never_laid_out(state: &BoxLayout) -> bool {
    state.unrounded == taffy::Layout::default()
}

/// Whether a box's layout is invalid, and would therefore be recomputed.
pub fn is_dirty(store: &LayoutStore, box_: BoxKey) -> bool {
    store.state(box_).is_none_or(BoxLayout::holds_no_layout)
}

/// Invalidates every box in the tree, which is what a change of scale factor forces.
///
/// Every length handed to the layout algorithms is in device pixels, so a scale change makes every
/// one of them wrong at once and there is no subtree that escapes. Returns how many boxes were
/// holding something that had to go.
///
/// Three kinds of held answer are dropped, and all three are measured in device pixels:
///
/// * the **per-box cache**, whose slots are keyed by the question asked — a run mode, an available
///   space, a known size — so two questions that differ only in the ratio they were asked at can
///   still be the same key: a min-content probe carries no size at all;
/// * the **baselines**, which a size-only answer served from the cache carries forward on the box's
///   behalf, and which are a distance down from its top edge;
/// * the **resolved lines** of an inline formatting context, which an intrinsic probe answers a
///   height from without breaking the paragraph again.
///
/// Leaving any of them behind gives a document that half rescales: boxes with an explicit size
/// move and grow correctly while boxes sized by their own content keep an extent from the previous
/// ratio, and the two disagree by a fraction of a pixel that snapping then turns into a visible
/// edge in the wrong place.
pub fn mark_all_dirty(store: &mut LayoutStore) -> u32 {
    // The viewport the held results belong to goes with them. A scale change leaves the surface the
    // same number of device pixels across while making every length inside it mean something else,
    // so a pass that compared viewports alone would find them equal and hold a document laid out at
    // the previous ratio.
    store.forget_root_layout();
    let keys = store.keys();
    let mut marked = 0;
    for key in keys {
        let held = store.state(key).is_none_or(|state| {
            !state.holds_no_layout()
                || state.first_baseline.is_some()
                || state.last_baseline.is_some()
                || state.inline.is_some()
        });
        if !held {
            continue;
        }
        store.take_inline_resolution(key);
        let state = store.state_mut(key);
        state.forget_layout();
        state.first_baseline = None;
        state.last_baseline = None;
        marked += 1;
    }
    counter::add(Counter::BoxesMarkedAllDirty, u64::from(marked));
    marked
}
