//! Focus traversal and trapping, from outside the crate.
//!
//! This target exists to prove the surface is usable by a consumer that has no access to anything
//! internal, because that is what a component library is: `FocusScope`, `RovingFocus`, dialog,
//! menu and command palette are all written against exactly these calls, and a traversal that were
//! private would make every one of them unwritable.

mod support;

use support::{Element, Fixture, Session};
use zgui_input::FocusSource;
use zgui_input::focus::order::{self, FocusDirection};
use zgui_input::focus::{FocusTraps, TrapOptions};
use zgui_vocab::UiState;

/// A toolbar of three controls, a dialog of two, and one thing that cannot be focused.
fn page() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("row").children(vec![
                Element::new("control").class("a"),
                Element::new("box").class("decoration"),
                Element::new("control").class("b"),
            ]),
            Element::new("surface").children(vec![
                Element::new("control").class("c"),
                Element::new("field").class("d"),
            ]),
        ]),
        "root, row, surface { display: block; width: 300px }
         control, field, box { display: block; height: 20px }",
    )
}

/// Every focusable element of the page, in sequential order, by class.
fn sequence(fixture: &Fixture) -> Vec<String> {
    let store = fixture.document.store();
    order::focusables(store, Some(&fixture.layout), store.key_of(fixture.root))
        .into_iter()
        .map(|node| {
            let index = store.index_of(node).expect("a live element");
            store
                .classes_of(index)
                .first()
                .map(|class| class.to_string())
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn sequential_order_is_document_order_over_what_can_be_focused() {
    let fixture = page();
    assert_eq!(
        sequence(&fixture),
        vec!["a", "b", "c", "d"],
        "the decoration between the first two controls is not a tab stop"
    );
}

#[test]
fn tabbing_walks_the_sequence_and_stops_at_the_end_without_a_trap() {
    let fixture = page();
    let store = fixture.document.store();
    let all = order::focusables(store, Some(&fixture.layout), store.key_of(fixture.root));

    let mut at = None;
    let mut visited = Vec::new();
    while let Some(next) = order::step(&all, at, FocusDirection::Next, false) {
        visited.push(next);
        at = Some(next);
    }
    assert_eq!(visited, all, "tab reaches every stop, once, in order");
    assert_eq!(
        order::step(&all, at, FocusDirection::Next, false),
        None,
        "and then leaves, because nothing is confining it"
    );
}

#[test]
fn a_trap_keeps_tab_inside_it_however_many_times_it_is_pressed() {
    let fixture = page();
    let store = fixture.document.store();
    let dialog = fixture.key("surface");

    let mut traps = FocusTraps::default();
    let id = traps.push(dialog, TrapOptions::MODAL, Some(fixture.key("row")));
    let inside = order::focusables(store, Some(&fixture.layout), dialog);
    assert_eq!(inside.len(), 2, "the dialog holds two focusable elements");

    let mut at = order::step(&inside, None, FocusDirection::First, true);
    for _ in 0..12 {
        at = order::step(&inside, at, FocusDirection::Next, TrapOptions::MODAL.wrap);
        let reached = at.expect("a trap with wrapping always answers");
        assert!(
            traps.confines(store, reached),
            "twelve presses of tab never leave the dialog"
        );
    }

    // Shift-tab wraps backwards for the same reason.
    let first = order::step(&inside, None, FocusDirection::First, true);
    let before_first = order::step(&inside, first, FocusDirection::Prev, true);
    assert_eq!(
        before_first,
        inside.last().copied(),
        "and going back past the first reaches the last"
    );

    // Dismissing puts focus back where the trap found it.
    let removed = traps.pop(id).expect("it was installed");
    assert_eq!(removed.restore_to, Some(fixture.key("row")));
    assert!(
        traps.confines(store, fixture.key("control")),
        "with the trap gone, the rest of the page is reachable again"
    );
}

#[test]
fn a_roving_toolbar_can_take_its_items_out_of_the_sequence_and_still_focus_them() {
    // What `RovingFocus` is: the group is one tab stop, and the arrow keys move within it. That
    // needs "focusable but not sequential", which is what a negative index means.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("row").children(vec![
                Element::new("control").class("first").attr("tabindex", "0"),
                Element::new("control").class("second").attr("tabindex", "-1"),
                Element::new("control").class("third").attr("tabindex", "-1"),
            ])]),
        "root, row { display: block; width: 300px }
         control { display: block; height: 20px }",
    );
    let store = fixture.document.store();
    assert_eq!(
        sequence(&fixture),
        vec!["first"],
        "the whole group is one tab stop"
    );

    let second = fixture.key("control");
    let _ = second;
    let all: Vec<_> = ["first", "second", "third"]
        .iter()
        .map(|class| {
            let index = find_by_class(&fixture, class);
            store.key_of(index)
        })
        .collect();
    for node in &all {
        assert!(
            order::is_focusable(store, Some(&fixture.layout), *node),
            "and every item can still be focused by the arrow keys"
        );
    }
}

#[test]
fn focusing_by_keyboard_shows_a_ring_and_focusing_by_pointer_does_not() {
    let mut session = Session::new(page());
    let control = session.fixture.key("control");

    {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session
            .router
            .focus(&world, Some(control), FocusSource::Keyboard);
    }
    assert!(state_of(&session, control).contains(UiState::FOCUS));
    assert!(state_of(&session, control).contains(UiState::FOCUS_RING));

    {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session
            .router
            .focus(&world, Some(control), FocusSource::Pointer);
    }
    assert!(state_of(&session, control).contains(UiState::FOCUS));
    assert!(
        !state_of(&session, control).contains(UiState::FOCUS_RING),
        "the same element, reached by pointer, shows no ring"
    );
}

#[test]
fn pressing_a_control_focuses_it_through_the_router() {
    let mut session = Session::new(page());
    let control = session.fixture.key("control");
    let point = session.fixture.centre_of("control");

    let default = session.default_at(point, zgui_vocab::PointerAction::Pressed);
    let zgui_input::FrameworkDefault::Focus { node, source } =
        default.expect("a press asks for focus")
    else {
        panic!("a press focuses");
    };
    assert_eq!(node, Some(control));

    {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session.router.focus(&world, node, source);
    }
    assert_eq!(session.router.interaction().focus.focused(), Some(control));
    assert!(state_of(&session, control).contains(UiState::FOCUS));
    assert!(
        state_of(&session, session.fixture.key("row")).contains(UiState::FOCUS_WITHIN),
        "and the toolbar around it knows the focus is inside"
    );
}

/// One element's interaction state.
fn state_of(session: &Session, node: zgui_dom::NodeKey) -> UiState {
    let index = session
        .fixture
        .document
        .store()
        .index_of(node)
        .expect("a live element");
    session.fixture.document.store().core(index).ui_state()
}

/// The first element carrying `class`.
fn find_by_class(fixture: &Fixture, class: &str) -> zgui_dom::NodeIndex {
    let store = fixture.document.store();
    let mut found = None;
    let mut stack = vec![fixture.root];
    while let Some(index) = stack.pop() {
        if store
            .classes_of(index)
            .iter()
            .any(|name| name.to_string() == class)
        {
            found = Some(index);
            break;
        }
        let mut child = store.core(index).first_child();
        while let Some(current) = child {
            stack.push(current);
            child = store.core(current).next_sibling();
        }
    }
    found.unwrap_or_else(|| panic!("no element with class `{class}`"))
}
