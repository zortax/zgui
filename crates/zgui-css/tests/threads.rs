//! Where the six-worker ceiling comes from.
//!
//! A constant asserted against itself proves nothing, so this target asks the engine instead: it
//! requests a pool far wider than the ceiling, lets the engine size its own, and reads back what it
//! built. The pool is process-global and built once, which is why this is a target of its own with
//! a single test in it.

use style::global_style_data::STYLE_THREAD_POOL;
use zgui_css::engine::threads::MAX_STYLE_THREADS;

#[test]
fn the_engine_will_not_build_a_pool_wider_than_the_ceiling() {
    // Written past this crate's own clamp deliberately: the question is what the *engine* does with
    // an unreasonable request, not what this crate does before making one.
    stylo_static_prefs::set_pref!("layout.threads", 64i32);

    assert_eq!(
        STYLE_THREAD_POOL.num_threads,
        Some(MAX_STYLE_THREADS),
        "the ceiling this crate publishes has to be the one the engine enforces"
    );
}
