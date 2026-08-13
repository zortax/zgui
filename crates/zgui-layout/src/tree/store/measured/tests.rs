//! What the memo promises: the same question is answered once, and a different one is not confused
//! with it.

use taffy::{AvailableSpace, LayoutInput, Line, RequestedAxis, RunMode, Size, SizingMode};

use super::{CAPACITY, Measured};

/// A min-content probe on the inline axis, taken against a containing block `basis` wide.
fn probe(basis: Option<f32>, available: AvailableSpace) -> LayoutInput {
    LayoutInput {
        run_mode: RunMode::ComputeSize,
        sizing_mode: SizingMode::InherentSize,
        axis: RequestedAxis::Horizontal,
        known_dimensions: Size::NONE,
        parent_size: Size {
            width: basis,
            height: None,
        },
        available_space: Size {
            width: available,
            height: AvailableSpace::MaxContent,
        },
        vertical_margins_are_collapsible: Line::FALSE,
    }
}

fn answer(width: f32) -> Size<f32> {
    Size { width, height: 0.0 }
}

#[test]
fn the_same_question_is_answered_from_the_memo() {
    let mut memo = Measured::default();
    let input = probe(Some(100.0), AvailableSpace::MinContent);
    assert_eq!(memo.get(&input), None);
    memo.insert(&input, answer(40.0));
    assert_eq!(memo.get(&input), Some(answer(40.0)));
}

#[test]
fn a_different_containing_block_is_a_different_question() {
    let mut memo = Measured::default();
    memo.insert(
        &probe(Some(100.0), AvailableSpace::MinContent),
        answer(40.0),
    );
    assert_eq!(
        memo.get(&probe(Some(200.0), AvailableSpace::MinContent)),
        None
    );
}

#[test]
fn a_different_available_space_is_a_different_question() {
    let mut memo = Measured::default();
    memo.insert(
        &probe(Some(100.0), AvailableSpace::MinContent),
        answer(40.0),
    );
    assert_eq!(
        memo.get(&probe(Some(100.0), AvailableSpace::MaxContent)),
        None
    );
    assert_eq!(
        memo.get(&probe(Some(100.0), AvailableSpace::Definite(100.0))),
        None
    );
}

/// Each kind of constraint carries its own discriminant, so no space however large is ever taken
/// for the keyword that a packed key would have written with the same bits.
#[test]
fn a_definite_space_never_collides_with_a_keyword() {
    for space in [f32::INFINITY, f32::NEG_INFINITY, 0.0, -0.0, f32::MAX] {
        let mut memo = Measured::default();
        memo.insert(&probe(None, AvailableSpace::Definite(space)), answer(1.0));
        assert_eq!(
            memo.get(&probe(None, AvailableSpace::MaxContent)),
            None,
            "definite {space} against max-content"
        );
        assert_eq!(
            memo.get(&probe(None, AvailableSpace::MinContent)),
            None,
            "definite {space} against min-content"
        );
    }
}

/// A dimension that is already decided and one that is merely available are different answers to
/// give, and a key that folded them together would hand back the wrong one.
#[test]
fn a_known_dimension_is_not_the_same_question_as_the_space_available() {
    let mut memo = Measured::default();
    let mut known = probe(Some(100.0), AvailableSpace::Definite(80.0));
    known.known_dimensions.width = Some(80.0);
    memo.insert(&known, answer(80.0));
    assert_eq!(
        memo.get(&probe(Some(100.0), AvailableSpace::Definite(80.0))),
        None
    );
    assert_eq!(memo.get(&known), Some(answer(80.0)));
}

#[test]
fn the_axis_asked_about_is_part_of_the_question() {
    let mut memo = Measured::default();
    let mut vertical = probe(Some(100.0), AvailableSpace::MinContent);
    memo.insert(&vertical, answer(40.0));
    vertical.axis = RequestedAxis::Vertical;
    assert_eq!(memo.get(&vertical), None);
}

#[test]
fn ignoring_the_boxs_own_sizing_is_part_of_the_question() {
    let mut memo = Measured::default();
    let mut content = probe(Some(100.0), AvailableSpace::MinContent);
    memo.insert(&content, answer(40.0));
    content.sizing_mode = SizingMode::ContentSize;
    assert_eq!(memo.get(&content), None);
}

#[test]
fn re_answering_one_question_overwrites_rather_than_growing() {
    let mut memo = Measured::default();
    let input = probe(Some(100.0), AvailableSpace::MinContent);
    memo.insert(&input, answer(40.0));
    memo.insert(&input, answer(50.0));
    assert_eq!(memo.get(&input), Some(answer(50.0)));
    assert_eq!(memo.held(), 1);
}

#[test]
fn the_memo_is_bounded_and_keeps_the_newest() {
    let mut memo = Measured::default();
    for index in 0..CAPACITY * 3 {
        memo.insert(
            &probe(Some(index as f32), AvailableSpace::MinContent),
            answer(index as f32),
        );
    }
    assert_eq!(memo.held(), CAPACITY);
    let newest = CAPACITY * 3 - 1;
    assert_eq!(
        memo.get(&probe(Some(newest as f32), AvailableSpace::MinContent)),
        Some(answer(newest as f32))
    );
}

#[test]
fn clearing_forgets_everything() {
    let mut memo = Measured::default();
    let input = probe(Some(100.0), AvailableSpace::MinContent);
    memo.insert(&input, answer(40.0));
    assert!(!memo.is_empty());
    memo.clear();
    assert!(memo.is_empty());
    assert_eq!(memo.get(&input), None);
}

#[test]
fn a_box_whose_margins_may_collapse_is_a_different_question() {
    // The one field of the engine's question that changes an answer without changing a single
    // number in it. A block box whose vertical margins collapse into its parent's is that much
    // shorter than the same box measured on its own, and every other part of the probe — the
    // constraints, the containing block, the axis, the sizing mode — is identical between the two.
    let mut memo = Measured::default();
    let alone = probe(Some(100.0), AvailableSpace::MaxContent);
    let collapsing = LayoutInput {
        vertical_margins_are_collapsible: Line::TRUE,
        ..alone
    };
    memo.insert(&alone, answer(40.0));
    assert_eq!(memo.get(&collapsing), None);
    memo.insert(&collapsing, answer(24.0));
    assert_eq!(memo.get(&alone), Some(answer(40.0)));
    assert_eq!(memo.get(&collapsing), Some(answer(24.0)));
}
