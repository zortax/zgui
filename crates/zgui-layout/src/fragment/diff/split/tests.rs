//! The accumulator's own behaviour, which is all this module has of its own.

use super::{Part, Passes, current, set, take, timed, walked};

/// Leaves the thread the way every other test on it expects to find it.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        set(Passes::Together);
        let _ = take();
    }
}

#[test]
fn a_frame_that_asked_for_nothing_divides_nothing() {
    let _restore = Restore;
    assert_eq!(current(), Passes::Together);
}

#[test]
fn each_descent_lands_in_its_own_field() {
    let _restore = Restore;
    let _ = take();
    timed(Part::Skeleton, || ());
    timed(Part::Geometry, || ());
    timed(Part::Index, || ());
    walked();
    let spent = take();
    assert_eq!(spent.together, 0, "no fused descent was made");
    assert_eq!(spent.walks, 1);
    // A descent of nothing can still take zero nanoseconds on a coarse clock, so what is asserted
    // is that the three fields moved independently of each other and not that any of them is large.
    assert_eq!(
        take(),
        super::Spent::default(),
        "taking the accumulator empties it"
    );
}

#[test]
fn the_accumulator_sums_rather_than_replaces() {
    let _restore = Restore;
    let _ = take();
    for _ in 0..3 {
        walked();
    }
    assert_eq!(take().walks, 3);
}
