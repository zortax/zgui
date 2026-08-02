//! The order caches are asked to give memory back in.

use crate::budget::eviction_order;
use crate::budget::report::{CacheId, rebuild};
use crate::budget::tests::{at, line, uniform};

/// The rule the whole ordering exists to get right.
///
/// Ordering by rebuild cost alone would take the cheapest thing to produce again — and the cheapest
/// thing in a window is whatever is being produced constantly, which is to say whatever is on the
/// screen. Here the cheap cache was read this very frame and the expensive one has not been read
/// for a hundred, and the expensive one has to go first.
#[test]
fn the_coldest_cache_goes_before_the_cheapest_one_to_rebuild() {
    let hot_and_cheap = CacheId::GlyphAtlas;
    let cold_and_dear = CacheId::ParagraphShaping;

    let lines = CacheId::ALL.map(|id| match id {
        id if id == hot_and_cheap => line(id, 100, rebuild::ARITHMETIC),
        id if id == cold_and_dear => line(id, 1, rebuild::RESHAPED),
        // Every other cache is left between the two, so that the assertion is about these.
        id => line(id, 50, rebuild::RECOMPUTED),
    });

    let order = eviction_order(&crate::budget::BudgetReport::new(lines));
    let cold = order.iter().position(|id| *id == cold_and_dear);
    let hot = order.iter().position(|id| *id == hot_and_cheap);
    assert!(
        cold < hot,
        "the cache untouched for a hundred frames must be asked before the one read this frame, \
         however much cheaper the second is to produce again — order was {order:?}"
    );
    assert_eq!(
        order.first(),
        Some(&cold_and_dear),
        "and it is asked first of all"
    );
    assert_eq!(
        order.last(),
        Some(&hot_and_cheap),
        "and the hottest is asked last"
    );
}

/// Cost decides only between caches that are equally cold.
#[test]
fn among_equally_cold_caches_the_cheapest_to_rebuild_goes_first() {
    let lines = CacheId::ALL.map(|id| match id {
        CacheId::RenderTargets => line(id, 10, rebuild::ARITHMETIC),
        CacheId::ParagraphShaping => line(id, 10, rebuild::RESHAPED),
        id => line(id, 10, rebuild::RECOMPUTED),
    });
    let order = eviction_order(&crate::budget::BudgetReport::new(lines));

    assert_eq!(
        order.first(),
        Some(&CacheId::RenderTargets),
        "an allocation is the cheapest thing here to get back, and every cache was read in the \
         same frame — order was {order:?}"
    );
    assert_eq!(
        order.last(),
        Some(&CacheId::ParagraphShaping),
        "and reshaping, which drags a relayout behind it, is the dearest"
    );
}

/// A guess nobody has used goes before everything, however recently it was produced.
#[test]
fn the_speculative_class_goes_first_however_warm_it_is() {
    let mut lines = CacheId::ALL.map(|id| line(id, 1, rebuild::ARITHMETIC));
    let speculating = CacheId::VectorResources.index();
    // Produced this very frame, the most expensive thing in the registry to produce again, and
    // never used — so every other rule in the ordering says it should go last.
    lines[speculating].report.last_used = at(999);
    lines[speculating].report.rebuild_cost = rebuild::RESHAPED;
    lines[speculating].report.speculative = 1;

    let order = eviction_order(&crate::budget::BudgetReport::new(lines));

    assert_eq!(
        order.first(),
        Some(&CacheId::VectorResources),
        "a guess that has never paid off is the one thing known not to be needed — order was \
         {order:?}"
    );
}

/// Two runs over the same reports agree, which is what makes the order assertable at all.
#[test]
fn the_order_is_a_function_of_the_reports_alone() {
    let report = uniform();
    assert_eq!(eviction_order(&report), eviction_order(&report));
    assert_eq!(
        eviction_order(&report).len(),
        CacheId::COUNT,
        "every registered cache is in the order exactly once"
    );
}
