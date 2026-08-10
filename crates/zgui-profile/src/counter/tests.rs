//! Tests for the counter set and the block behind it.

use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use super::{COUNTERS_ENABLED, Counter, Counters, Group, add, bump, get, reset, set, snapshot};

/// The counter block is process-wide, so tests that write to it take turns.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn the_set_is_complete_and_has_no_duplicates() {
    assert_eq!(Counter::COUNT, 78);
    assert_eq!(Counter::ALL.len(), Counter::COUNT);

    let names: BTreeSet<&str> = Counter::ALL.iter().map(|counter| counter.name()).collect();
    assert_eq!(names.len(), Counter::COUNT, "two counters share a name");

    let indices: BTreeSet<usize> = Counter::ALL.iter().map(|counter| counter.index()).collect();
    assert_eq!(indices, (0..Counter::COUNT).collect());
}

#[test]
fn only_the_counters_a_capture_renderer_cannot_produce_are_renderer_specific() {
    let renderer_specific: BTreeSet<&str> = Counter::ALL
        .iter()
        .filter(|counter| counter.group() == Group::RendererSpecific)
        .map(|counter| counter.name())
        .collect();
    assert_eq!(
        renderer_specific,
        BTreeSet::from([
            "atlas_texture_writes",
            "atlas_upload_batches",
            "bytes_uploaded",
            "damage_px",
            "draw_calls",
            "side_table_slots_prepared",
            "staging_one_shot_bytes",
            "staging_warm_bytes",
            "upload_chunks_allocated",
        ])
    );
}

#[test]
fn a_snapshot_field_answers_the_same_as_its_counter() {
    let counters = Counters {
        nodes_visited: 7,
        vello_passes: 2,
        ..Counters::ZERO
    };
    assert_eq!(counters.get(Counter::NodesVisited), 7);
    assert_eq!(counters.get(Counter::VelloPasses), 2);
    assert_eq!(counters.get(Counter::DrawCalls), 0);
    assert_eq!(counters.iter().map(|(_, value)| value).sum::<u64>(), 7 + 2);
}

#[test]
fn a_delta_reports_what_moved_between_two_snapshots() {
    let before = Counters {
        repaints: 4,
        ..Counters::ZERO
    };
    let after = Counters {
        repaints: 10,
        wakes: 1,
        ..Counters::ZERO
    };
    let delta = before.delta(&after);
    assert_eq!(delta.repaints, 6);
    assert_eq!(delta.wakes, 1);
    // A counter that went backwards — because it was reset between the two reads — saturates
    // rather than wrapping to something enormous.
    assert_eq!(after.delta(&before).repaints, 0);
}

#[test]
fn debug_output_names_only_the_counters_that_moved() {
    let counters = Counters {
        timers_fired: 3,
        ..Counters::ZERO
    };
    let text = format!("{counters:?}");
    assert!(text.contains("timers_fired: 3"), "{text}");
    assert!(!text.contains("wakes"), "{text}");
}

#[test]
fn counting_accumulates_until_it_is_reset() {
    let _guard = exclusive();
    reset();
    bump(Counter::PrimitivesEmitted);
    bump(Counter::PrimitivesEmitted);
    add(Counter::PrimitivesCulled, 40);

    let expected_emitted = if COUNTERS_ENABLED { 2 } else { 0 };
    let expected_culled = if COUNTERS_ENABLED { 40 } else { 0 };
    assert_eq!(get(Counter::PrimitivesEmitted), expected_emitted);
    assert_eq!(snapshot().primitives_emitted, expected_emitted);
    assert_eq!(snapshot().primitives_culled, expected_culled);

    reset();
    assert_eq!(get(Counter::PrimitivesEmitted), 0);
    assert_eq!(snapshot(), Counters::ZERO);
}

#[test]
fn counting_from_several_threads_loses_nothing() {
    let _guard = exclusive();
    reset();

    const THREADS: u64 = 8;
    const ROUNDS: u64 = 1_000;
    thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..ROUNDS {
                    bump(Counter::NodesVisited);
                }
            });
        }
    });

    let expected = if COUNTERS_ENABLED {
        THREADS * ROUNDS
    } else {
        0
    };
    assert_eq!(get(Counter::NodesVisited), expected);
    reset();
}

#[test]
fn the_block_is_live_exactly_when_it_says_it_is() {
    assert_eq!(
        COUNTERS_ENABLED,
        cfg!(any(feature = "counters", debug_assertions))
    );
}

#[test]
fn a_counter_name_states_its_polarity() {
    // A field called `idle_frames` that counts frames *drawn* cost one acceptance criterion its
    // meaning: read literally, meeting it required every panel to redraw on every refresh. A
    // counter is named for what it counts, and this is the mechanical form of that rule.
    let names: Vec<&str> = Counter::ALL.iter().map(|counter| counter.name()).collect();
    assert!(
        names.contains(&"frames_drawn"),
        "the counter of frames drawn is named for what it counts: {names:?}"
    );
    assert!(
        !names.contains(&"idle_frames"),
        "no counter is named for the opposite of what it holds"
    );
}

#[test]
fn a_live_count_is_assigned_and_a_total_is_accumulated() {
    let _guard = exclusive();
    reset();
    // The two writes are different operations and the group is what says which a counter takes.
    // A gauge that was accumulated would report the sum of every length it has ever had.
    set(Counter::ClipEntriesLive, 2_002);
    set(Counter::ClipEntriesLive, 167_488);
    add(Counter::PrimitivesEmitted, 40);
    add(Counter::PrimitivesEmitted, 2);

    let expected = if COUNTERS_ENABLED {
        (167_488, 42)
    } else {
        (0, 0)
    };
    assert_eq!(
        (
            get(Counter::ClipEntriesLive),
            get(Counter::PrimitivesEmitted)
        ),
        expected
    );
    reset();
}

#[test]
fn every_live_count_says_so_and_nothing_else_does() {
    for counter in Counter::live() {
        assert!(counter.group().is_live(), "{}", counter.name());
        assert!(
            counter.name().ends_with("_live")
                || counter.name().starts_with("scratch_")
                || counter.name().ends_with("_nodes")
        );
    }
    assert!(
        !Counter::PrimitivesEmitted.group().is_live(),
        "a running total is not a gauge"
    );
}
