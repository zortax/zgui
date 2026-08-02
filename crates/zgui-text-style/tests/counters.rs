//! What lowering costs, in the frame counters a budget is written against.
//!
//! The cache's own `lowerings()` and `hits()` answer for one cache. The frame counters answer for
//! the frame, which is what a budget needs: nothing holding a budget has a handle on the cache that
//! produced the work.
//!
//! # Why this is a target of its own
//!
//! The counter block is process-wide and accumulates until it is reset, so a case that reads one
//! has to be the only thing moving it. Every counter assertion in this crate lives here, behind one
//! lock, and no other target reads them at all.

use std::sync::{Mutex, MutexGuard};

use zgui_css::StyleDraft;
use zgui_css::values::font::FontSize;
use zgui_css::values::font::FontSizeExt;
use zgui_geom::CssPx;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_text_style::TextStyleCache;

/// Held for the whole of any case that reads a counter.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// A style whose font size is `px`, which is enough to give it property groups of its own.
fn sized(px: f32) -> zgui_css::ComputedStyle {
    let mut draft = StyleDraft::initial();
    draft.font().font_size = FontSize::for_px(CssPx(px));
    draft.build()
}

#[test]
fn each_distinct_style_is_lowered_once_and_counted_once() {
    let _guard = measuring();
    let mut cache = TextStyleCache::default();

    counter::reset();
    for step in 0..8 {
        cache.get(&sized(10.0 + step as f32));
    }

    assert_eq!(cache.lowerings(), 8);
    if COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::StylesLowered),
            8,
            "eight distinct styles, eight lowerings, and the counter has to have seen all of them"
        );
        assert_eq!(
            counter::get(Counter::StylesLoweredFromCache),
            0,
            "no two of the eight share their property groups, so nothing could have been a hit"
        );
    }
}

#[test]
fn a_thousand_elements_sharing_one_style_lower_once_and_hit_the_rest_of_the_time() {
    let _guard = measuring();
    let mut cache = TextStyleCache::default();
    let style = sized(17.0);

    counter::reset();
    for _ in 0..1000 {
        cache.get(&style);
    }

    if !COUNTERS_ENABLED {
        return;
    }
    let lowered = counter::get(Counter::StylesLowered);
    let from_cache = counter::get(Counter::StylesLoweredFromCache);
    assert_eq!(
        lowered, 1,
        "the ratio is the whole point of the pair: a thousand elements sharing a style lower once"
    );
    assert_eq!(from_cache, 999);
    assert_eq!(
        lowered + from_cache,
        1000,
        "every call is counted exactly once, on one side or the other, or the ratio the two are \
         read as is not a ratio of anything"
    );
}

#[test]
fn a_second_cache_starting_empty_lowers_again_and_says_so() {
    let _guard = measuring();
    let style = sized(23.0);

    // The control: the first cache moves both counters, so the zero asserted below is a zero the
    // counters demonstrably could have left.
    let mut first = TextStyleCache::default();
    counter::reset();
    first.get(&style);
    first.get(&style);
    let control_lowered = counter::get(Counter::StylesLowered);
    let control_hits = counter::get(Counter::StylesLoweredFromCache);

    // A cleared cache holds nothing, so the same style is lowered again rather than answered.
    let mut second = TextStyleCache::default();
    counter::reset();
    second.get(&style);
    let lowered = counter::get(Counter::StylesLowered);
    let hits = counter::get(Counter::StylesLoweredFromCache);

    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(control_lowered, 1);
    assert_eq!(control_hits, 1, "the control moved the hit counter");
    assert_eq!(lowered, 1);
    assert_eq!(
        hits, 0,
        "an empty cache cannot answer anything, and a hit recorded here would mean the counter is \
         being moved by something other than a cache hit"
    );
}
