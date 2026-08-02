//! That the bits stay distinct from the engine's own, and from each other.

use style::selector_parser::RestyleDamage;

use super::{
    ALL, CONSTRUCT_BOX, CONSTRUCT_DESCENDANTS, CONSTRUCT_FC, REBREAK_TEXT, RECALCULATE_INK,
    RELAYOUT_BOX, RESHAPE_TEXT,
};

#[test]
fn our_bits_never_collide_with_the_engines_own_four() {
    let engine = RestyleDamage::all();
    for bit in [
        CONSTRUCT_BOX,
        CONSTRUCT_FC,
        CONSTRUCT_DESCENDANTS,
        RELAYOUT_BOX,
        RESHAPE_TEXT,
        REBREAK_TEXT,
        RECALCULATE_INK,
    ] {
        assert_eq!(bit.bits().count_ones(), 1, "each of ours is a single bit");
        assert!(
            !engine.contains(bit),
            "the engine's own bits are the low four and ours start above them"
        );
    }
    assert_eq!(ALL.bits().count_ones(), 6);
}

#[test]
fn relaying_a_box_out_is_not_rebuilding_it() {
    // The whole point of the narrow answer: a width change carries the bit that says "measure this
    // again" and none of the three that say "make these boxes again". A build that let them drift
    // together would put every inset animation back on the whole-document path.
    assert!(ALL.contains(RELAYOUT_BOX));
    assert!(!RELAYOUT_BOX.intersects(CONSTRUCT_BOX | CONSTRUCT_FC | CONSTRUCT_DESCENDANTS));
}

#[test]
fn shaping_and_breaking_are_distinct_because_one_is_far_more_expensive() {
    assert_ne!(RESHAPE_TEXT, REBREAK_TEXT);
    assert!(!ALL.difference(RESHAPE_TEXT).contains(RESHAPE_TEXT));
}

#[test]
fn recalculating_ink_is_not_one_of_the_bits_that_mean_a_layout() {
    // The stage that reads these tests for a layout obligation before it tests for an ink one, and
    // it does so by asking whether any bit of `ALL` is present. An ink bit inside that set would
    // make every corner radius rebuild a box, which is the defect this bit was added to end.
    assert!(!ALL.contains(RECALCULATE_INK));
    assert!(!RECALCULATE_INK.intersects(ALL));
}
