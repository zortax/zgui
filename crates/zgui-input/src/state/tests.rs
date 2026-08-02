//! What the three interaction states do to a real document.

use zgui_dom::{Document, EverythingMatters, NodeKey};
use zgui_interned::ElementName;
use zgui_vocab::UiState;

use crate::hit::HitChain;
use crate::state::focus::FocusSource;
use crate::state::{Active, Focus, Hover};

/// `root > field > inner`, as keys, and the document holding them.
fn document() -> (Document, [NodeKey; 3]) {
    let document = Document::new();
    let indices = document
        .edit(&EverythingMatters, |edit| {
            let root = edit.create_element(ElementName::new("root"));
            edit.insert_before(document.document_index(), root, None);
            let field = edit.create_element(ElementName::new("field"));
            edit.insert_before(root, field, None);
            let inner = edit.create_element(ElementName::new("editor"));
            edit.insert_before(field, inner, None);
            [root, field, inner]
        })
        .expect("not poisoned");
    let keys = indices.map(|index| document.store().key_of(index));
    (document, keys)
}

/// Whether `key` carries `bit`.
fn has(document: &Document, key: NodeKey, bit: UiState) -> bool {
    let index = document.store().index_of(key).expect("a live node");
    document.store().core(index).ui_state().contains(bit)
}

#[test]
fn hover_is_written_up_the_whole_path_and_taken_off_it_again() {
    let (document, [root, field, inner]) = document();
    let chain = HitChain::to_root(document.store(), inner);
    let mut hover = Hover::default();

    let moved = hover.move_to(&document, &EverythingMatters, &chain);
    assert_eq!(moved.entered.len(), 3);
    assert_eq!(hover.target(), Some(inner));
    for key in [root, field, inner] {
        assert!(has(&document, key, UiState::HOVER));
    }

    hover.clear(&document, &EverythingMatters);
    for key in [root, field, inner] {
        assert!(!has(&document, key, UiState::HOVER));
    }
}

#[test]
fn a_press_that_is_cancelled_releases_exactly_as_one_that_is_let_go_does() {
    let (document, [root, _, inner]) = document();
    let chain = HitChain::to_root(document.store(), inner);
    let mut active = Active::default();

    active.press(&document, &EverythingMatters, &chain);
    assert!(active.is_pressed());
    assert!(has(&document, root, UiState::ACTIVE));

    active.release(&document, &EverythingMatters);
    assert!(!active.is_pressed());
    assert!(!has(&document, inner, UiState::ACTIVE));
    assert!(!has(&document, root, UiState::ACTIVE));
}

#[test]
fn focus_sets_one_element_and_focus_within_sets_its_ancestors() {
    let (document, [root, field, inner]) = document();
    let chain = HitChain::to_root(document.store(), inner);
    let mut focus = Focus::default();

    let (lost, gained) =
        focus.move_to(&document, &EverythingMatters, &chain, FocusSource::Keyboard);
    assert_eq!((lost, gained), (None, Some(inner)));

    assert!(has(&document, inner, UiState::FOCUS));
    assert!(
        !has(&document, field, UiState::FOCUS),
        ":focus is one element"
    );
    for key in [root, field, inner] {
        assert!(
            has(&document, key, UiState::FOCUS_WITHIN),
            ":focus-within is the element and everything containing it"
        );
    }
}

#[test]
fn the_ring_follows_how_focus_arrived_and_not_where_it_landed() {
    let (document, [_, _, inner]) = document();
    let chain = HitChain::to_root(document.store(), inner);
    let mut focus = Focus::default();

    focus.move_to(&document, &EverythingMatters, &chain, FocusSource::Pointer);
    assert!(has(&document, inner, UiState::FOCUS));
    assert!(
        !has(&document, inner, UiState::FOCUS_RING),
        "a pointer user has just pointed at it"
    );

    focus.move_to(&document, &EverythingMatters, &chain, FocusSource::Keyboard);
    assert!(
        has(&document, inner, UiState::FOCUS_RING),
        "reaching the same element by keyboard shows the ring without focus moving at all"
    );
    assert!(focus.shows_ring());

    focus.move_to(&document, &EverythingMatters, &chain, FocusSource::Pointer);
    assert!(
        !has(&document, inner, UiState::FOCUS_RING),
        "and clicking it takes the ring away again"
    );
}

#[test]
fn moving_focus_away_clears_every_bit_it_left_behind() {
    let (document, [root, field, inner]) = document();
    let mut focus = Focus::default();
    focus.move_to(
        &document,
        &EverythingMatters,
        &HitChain::to_root(document.store(), inner),
        FocusSource::Keyboard,
    );

    let (lost, gained) = focus.move_to(
        &document,
        &EverythingMatters,
        &HitChain::to_root(document.store(), field),
        FocusSource::Pointer,
    );
    assert_eq!((lost, gained), (Some(inner), Some(field)));

    assert!(!has(&document, inner, UiState::FOCUS));
    assert!(!has(&document, inner, UiState::FOCUS_RING));
    assert!(
        !has(&document, inner, UiState::FOCUS_WITHIN),
        "the old element is no longer inside the focused one"
    );
    assert!(has(&document, field, UiState::FOCUS));
    assert!(has(&document, root, UiState::FOCUS_WITHIN));
}

#[test]
fn clearing_focus_leaves_no_element_carrying_anything() {
    let (document, [root, field, inner]) = document();
    let mut focus = Focus::default();
    focus.move_to(
        &document,
        &EverythingMatters,
        &HitChain::to_root(document.store(), inner),
        FocusSource::Keyboard,
    );

    let (lost, gained) = focus.clear(&document, &EverythingMatters);
    assert_eq!((lost, gained), (Some(inner), None));
    assert_eq!(focus.focused(), None);
    assert!(focus.within().is_empty());
    for key in [root, field, inner] {
        for bit in [UiState::FOCUS, UiState::FOCUS_RING, UiState::FOCUS_WITHIN] {
            assert!(!has(&document, key, bit));
        }
    }
}
