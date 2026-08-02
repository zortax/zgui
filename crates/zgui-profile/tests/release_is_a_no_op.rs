//! Proves the counters really are compiled out when nothing asked for them.
//!
//! Run as `cargo test -p zgui-profile --release`. In that build debug assertions are off and the
//! `counters` feature is not enabled, so the storage behind the counters does not exist and every
//! entry point is an empty inlined body. What is observable from outside is that writing to a
//! counter changes nothing at all, and that is what is asserted here — including that a snapshot
//! taken after a million increments is byte-for-byte the zero snapshot.
//!
//! The complementary assertion, that the counters *do* record when they are compiled in, is in
//! the crate's own test module and runs in the ordinary debug build.

use zgui_profile::{COUNTERS_ENABLED, Counter, Counters, counter};

#[test]
fn the_flag_matches_how_this_build_was_compiled() {
    assert_eq!(
        COUNTERS_ENABLED,
        cfg!(any(feature = "counters", debug_assertions))
    );
}

#[cfg(not(any(feature = "counters", debug_assertions)))]
#[test]
fn a_counter_written_a_million_times_still_reads_zero() {
    for counter in Counter::ALL {
        counter::add(counter, 1);
    }
    for _ in 0..1_000_000u64 {
        counter::bump(Counter::NodesVisited);
    }

    const { assert!(!COUNTERS_ENABLED) };
    assert_eq!(counter::snapshot(), Counters::ZERO);
    for name in Counter::ALL {
        assert_eq!(counter::get(name), 0, "{} recorded something", name.name());
    }
}

#[cfg(any(feature = "counters", debug_assertions))]
#[test]
fn a_counter_records_when_it_is_compiled_in() {
    counter::reset();
    counter::bump(Counter::NodesVisited);
    const { assert!(COUNTERS_ENABLED) };
    assert_eq!(counter::get(Counter::NodesVisited), 1);
    assert_ne!(counter::snapshot(), Counters::ZERO);
    counter::reset();
}
