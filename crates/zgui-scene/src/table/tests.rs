//! What the side tables promise, including the promise across frames.

use zgui_geom::Matrix4;

use crate::id::ClipId;
use crate::table::{ChangeCoverage, Table};

/// A table of transforms, which is the smallest content there is to intern.
fn table() -> Table<ClipId, Matrix4> {
    Table::new()
}

/// A transform that is distinguishable by its content.
fn shifted(by: f32) -> Matrix4 {
    Matrix4::translation(by, 0.0, 0.0)
}

#[test]
fn the_same_content_interns_to_the_same_id() {
    let mut table = table();
    assert_eq!(table.intern(shifted(1.0)), table.intern(shifted(1.0)));
    assert_ne!(table.intern(shifted(1.0)), table.intern(shifted(2.0)));
    assert_eq!(table.len(), 2);
}

/// The acceptance property: an id handed out in one frame keeps resolving to the same content five
/// frames later, while a recorded range of paint operations still refers to it.
#[test]
fn an_id_survives_five_frames_while_a_recorded_range_refers_to_it() {
    let mut table = table();

    table.begin_frame();
    let id = table.intern(shifted(3.0));
    let hash = table.content_hash(id).expect("just interned");
    // A recorded range of a previous frame's operations is what holds it: those operations carry
    // this id, and replaying them resolves it.
    table.retain(id);

    for frame in 1..=5 {
        table.begin_frame();
        // Churn: every intervening frame interns unrelated content and evicts what it can.
        table.intern(shifted(100.0 + frame as f32));
        table.evict_least_recently_used();
    }

    assert!(table.contains(id), "a referenced id must not be reused");
    assert_eq!(table.content_hash(id), Some(hash));
    assert_eq!(table.get(id), Some(&shifted(3.0)));

    // Once the range is discarded, the entry becomes ordinary and is eventually reclaimed.
    table.release(id);
    table.begin_frame();
    assert_eq!(table.evict_least_recently_used(), 1);
    assert!(!table.contains(id));
}

#[test]
fn eviction_takes_the_coldest_generation_and_no_other() {
    let mut table = table();

    table.begin_frame();
    let old_a = table.intern(shifted(1.0));
    let old_b = table.intern(shifted(2.0));

    table.begin_frame();
    let newer = table.intern(shifted(3.0));

    table.begin_frame();
    assert_eq!(table.evict_least_recently_used(), 2);
    assert!(!table.contains(old_a));
    assert!(!table.contains(old_b));
    assert!(table.contains(newer));

    assert_eq!(table.evict_least_recently_used(), 1);
    assert!(table.is_empty());
    assert_eq!(table.evict_least_recently_used(), 0);
}

#[test]
fn using_an_entry_this_frame_saves_it() {
    let mut table = table();
    table.begin_frame();
    let kept = table.intern(shifted(1.0));
    let dropped = table.intern(shifted(2.0));

    table.begin_frame();
    assert!(table.use_of(kept).is_some());

    assert_eq!(table.evict_least_recently_used(), 1);
    assert!(table.contains(kept));
    assert!(!table.contains(dropped));
}

#[test]
fn a_pinned_entry_is_never_evicted() {
    let mut table = table();
    table.begin_frame();
    let permanent = table.intern(Matrix4::IDENTITY);
    table.pin(permanent);

    table.begin_frame();
    assert_eq!(table.evict_least_recently_used(), 0);
    assert!(table.contains(permanent));
}

#[test]
fn a_refcount_saturates_rather_than_wrapping_in_either_direction() {
    let mut table = table();
    let id = table.intern(shifted(1.0));

    for _ in 0..5 {
        table.release(id);
    }
    assert_eq!(table.refs(id), Some(0));

    table.retain(id);
    table.retain(id);
    assert_eq!(table.refs(id), Some(2));
}

#[test]
fn a_freed_slot_is_reused_and_the_hash_index_does_not_keep_a_ghost() {
    let mut table = table();
    table.begin_frame();
    let first = table.intern(shifted(1.0));

    table.begin_frame();
    assert_eq!(table.evict_least_recently_used(), 1);

    let second = table.intern(shifted(2.0));
    assert_eq!(second.0, first.0, "the slot is reused");
    assert_eq!(table.get(second), Some(&shifted(2.0)));
    assert_eq!(table.len(), 1);

    // The evicted content is genuinely gone rather than still reachable through its old hash.
    let reinterned = table.intern(shifted(1.0));
    assert_ne!(reinterned, second);
}

#[test]
fn a_reader_sees_only_changes_after_its_version() {
    let mut table = table();
    let before = table.version();
    let first = table.intern(shifted(1.0));
    let after_first = table.version();
    let second = table.intern(shifted(2.0));

    let mut changes = Vec::new();
    assert_eq!(
        table.changes_since(after_first, &mut changes),
        ChangeCoverage::Delta
    );
    assert_eq!(changes, vec![second]);

    changes.clear();
    assert_eq!(
        table.changes_since(before, &mut changes),
        ChangeCoverage::Delta
    );
    assert_eq!(changes, vec![first, second]);
}

#[test]
fn reinterning_unchanged_content_does_not_publish_a_change() {
    let mut table = table();
    table.intern(shifted(1.0));
    let settled = table.version();
    table.intern(shifted(1.0));

    let mut changes = Vec::new();
    assert_eq!(
        table.changes_since(settled, &mut changes),
        ChangeCoverage::Delta
    );
    assert!(changes.is_empty());
}

#[test]
fn a_clone_rejects_the_sources_version() {
    let mut table = table();
    table.intern(shifted(1.0));
    let source = table.version();
    let cloned = table.clone();

    assert_eq!(
        cloned.changes_since(source, &mut Vec::new()),
        ChangeCoverage::All
    );
}
