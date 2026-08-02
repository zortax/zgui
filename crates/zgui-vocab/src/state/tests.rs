//! Properties of the bit set that a per-bit constant cannot state on its own.

use crate::state::UiState;
use crate::state::bits::NAMED;
use crate::state::pairs::COMPLEMENTS;

#[test]
fn every_named_state_is_a_single_bit() {
    for (state, name) in NAMED {
        assert_eq!(
            state.bits().count_ones(),
            1,
            "{name} is not a single bit: {:#x}",
            state.bits()
        );
    }
}

#[test]
fn no_two_named_states_share_a_bit() {
    for (index, (state, name)) in NAMED.iter().enumerate() {
        for (other, other_name) in &NAMED[index + 1..] {
            assert!(
                !state.intersects(*other),
                "{name} and {other_name} occupy the same bit"
            );
        }
    }
}

#[test]
fn the_author_settable_mask_is_exactly_the_eight_states_a_view_may_assert() {
    let expected = UiState::CHECKED
        | UiState::DISABLED
        | UiState::OPEN
        | UiState::INDETERMINATE
        | UiState::PLACEHOLDER_SHOWN
        | UiState::READ_ONLY
        | UiState::REQUIRED
        | UiState::INVALID;
    assert_eq!(UiState::AUTHOR_SETTABLE, expected);
    assert_eq!(UiState::AUTHOR_SETTABLE.bits().count_ones(), 8);
    // The framework-computed states are not in it, which is the half that matters.
    for computed in [
        UiState::HOVER,
        UiState::ACTIVE,
        UiState::FOCUS,
        UiState::FOCUS_RING,
        UiState::FOCUS_WITHIN,
    ] {
        assert!(!UiState::AUTHOR_SETTABLE.intersects(computed));
    }
}

#[test]
fn complementary_halves_are_distinct_and_named() {
    for (positive, negative) in COMPLEMENTS {
        assert!(!positive.intersects(*negative));
        assert_eq!(positive.bits().count_ones(), 1);
        assert_eq!(negative.bits().count_ones(), 1);
    }
}

#[test]
fn applying_either_half_of_a_pair_clears_the_other() {
    for (positive, negative) in COMPLEMENTS {
        let with_positive = UiState::EMPTY.apply(*positive, true);
        assert!(with_positive.contains(*positive));
        assert!(!with_positive.contains(*negative));
        assert!(with_positive.pairs_are_consistent());

        let with_negative = with_positive.apply(*negative, true);
        assert!(with_negative.contains(*negative));
        assert!(!with_negative.contains(*positive));
        assert!(with_negative.pairs_are_consistent());

        // Clearing one half asserts the other rather than leaving the element in neither state.
        let cleared = with_negative.apply(*negative, false);
        assert!(cleared.contains(*positive));
    }
}

#[test]
fn applying_a_state_with_no_complement_touches_only_that_bit() {
    let before = UiState::ENABLED | UiState::VALID;
    let after = before.apply(UiState::HOVER, true);
    assert_eq!(after.symmetric_difference(before), UiState::HOVER);
}

#[test]
fn the_symmetric_difference_is_what_changed() {
    let before = UiState::ENABLED | UiState::HOVER;
    let after = before.apply(UiState::DISABLED, true);
    assert_eq!(
        after.symmetric_difference(before),
        UiState::ENABLED | UiState::DISABLED
    );
}

#[test]
fn heading_levels_round_trip_and_clamp() {
    for level in 1..=9u8 {
        assert_eq!(
            UiState::with_heading_level(level).heading_level(),
            Some(level)
        );
    }
    assert_eq!(UiState::with_heading_level(0).heading_level(), None);
    assert_eq!(UiState::with_heading_level(15).heading_level(), Some(9));
}

#[test]
fn debug_names_the_states_that_are_set() {
    assert_eq!(format!("{:?}", UiState::EMPTY), "UiState(empty)");
    assert_eq!(
        format!("{:?}", UiState::HOVER | UiState::CHECKED),
        "UiState(HOVER|CHECKED)"
    );
}
