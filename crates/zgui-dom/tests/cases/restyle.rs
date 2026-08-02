//! Does a real traversal over our traits produce the right computed values?
//!
//! Every assertion here is a computed value read back off the tree after a real restyle, not a call
//! into the matching machinery. The question is whether the engine, driven through this crate's DOM
//! traits, arrives at the right answer — and it is the question that no unit test in the crate can
//! reach, because all of the interesting machinery is on the engine's side of the traits.

use style::values::computed::Display;
use zgui_dom::Document;
use zgui_interned::ClassName;
use zgui_vocab::UiState;

use crate::support::edit;
use crate::support::engine::Engine;
use crate::support::fixture;
use crate::support::read::{color, display, font_size, radius};

/// An engine over `document` with `css` applied.
fn engine_for(document: &Document, css: &str) -> Engine {
    let mut engine = Engine::new(document);
    engine.add_author_sheet(css);
    engine
}

#[test]
fn combinators_classes_and_state_all_match() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        r"
        root                { color: rgb(1, 1, 1); display: block }
        .card .item         { color: rgb(2, 2, 2) }
        .bar > label        { color: rgb(3, 3, 3) }
        .hot + box.item     { color: rgb(4, 4, 4) }
        .hot ~ label        { font-size: 20px }
        box.item:hover      { color: rgb(6, 6, 6) }
        nav:nth-child(2)    { display: flex }
        label:not(.title)   { border-top-left-radius: 3px }
        [data-kind='leaf']  { border-top-right-radius: 5px }
        ",
    );

    let pass = engine.restyle(&mut tree.document, None);
    assert!(pass.traversed, "the first traversal always runs");
    assert_eq!(
        pass.styled,
        tree.all().count(),
        "every element in the fixture comes out with a style"
    );
    assert_eq!(
        pass.restyled, 0,
        "the engine's restyled flag means *re*styled: it is set only for an element that already \
         had a style, so the first pass reports none"
    );

    // Inheritance: the header has no rule of its own and takes the root's colour.
    assert_eq!(color(&tree.document, tree.at("header")), (1, 1, 1));
    // Descendant, across two levels.
    assert_eq!(color(&tree.document, tree.at("i1")), (2, 2, 2));
    assert_eq!(color(&tree.document, tree.at("deep")), (2, 2, 2));
    // Child, and not descendant.
    assert_eq!(color(&tree.document, tree.at("title")), (3, 3, 3));
    // Next-sibling, stepping over nothing.
    assert_eq!(color(&tree.document, tree.at("i3")), (4, 4, 4));
    // Subsequent-sibling, stepping over the marker between them.
    assert_eq!(font_size(&tree.document, tree.at("leaf")), 20.0);
    assert_eq!(font_size(&tree.document, tree.at("title")), 16.0);
    // State.
    assert_eq!(color(&tree.document, tree.at("i2")), (6, 6, 6));
    // Positional: the nav is the root's second element child, and the text node before it is not
    // one. Its children come out `block` rather than `inline`, because the engine blockifies the
    // children of a flex container.
    assert_eq!(display(&tree.document, tree.at("nav")), Display::Flex);
    assert_eq!(display(&tree.document, tree.at("linkA")), Display::Block);
    // Negation excludes the title and matches the other label.
    assert_eq!(radius(&tree.document, tree.at("leaf")), 3.0);
    assert_eq!(radius(&tree.document, tree.at("title")), 0.0);
}

#[test]
fn specificity_and_source_order_decide() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        r"
        .item     { color: rgb(10, 0, 0) }
        box.item  { color: rgb(20, 0, 0) }
        .item     { color: rgb(30, 0, 0) }
        #main     { color: rgb(40, 0, 0) }
        box.card  { color: rgb(50, 0, 0) }
        ",
    );
    engine.restyle(&mut tree.document, None);

    assert_eq!(
        color(&tree.document, tree.at("i1")),
        (20, 0, 0),
        "two classes beat one class whatever the source order"
    );
    assert_eq!(
        color(&tree.document, tree.at("card")),
        (40, 0, 0),
        "an identifier beats a class plus a tag"
    );
}

#[test]
fn later_rule_of_equal_specificity_wins() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        ".item { color: rgb(1, 0, 0) } .item { color: rgb(2, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    assert_eq!(color(&tree.document, tree.at("i1")), (2, 0, 0));
}

#[test]
fn origins_cascade_and_important_reverses_them() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_sheet(
        ".item { color: rgb(9, 0, 0) } .card { color: rgb(8, 0, 0) !important }",
        style::stylesheets::Origin::UserAgent,
    );
    engine.add_author_sheet(".item { color: rgb(7, 0, 0) } .card { color: rgb(6, 0, 0) }");
    engine.restyle(&mut tree.document, None);

    assert_eq!(
        color(&tree.document, tree.at("i1")),
        (7, 0, 0),
        "author beats user-agent for a normal declaration"
    );
    assert_eq!(
        color(&tree.document, tree.at("card")),
        (8, 0, 0),
        "and loses to an important user-agent one"
    );
}

#[test]
fn custom_properties_inherit_and_resolve_through_var() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        r"
        :root  { --accent: rgb(11, 22, 33) }
        .card  { --accent: rgb(44, 55, 66) }
        .item  { color: var(--accent) }
        label  { color: var(--missing, rgb(77, 88, 99)) }
        ",
    );
    engine.restyle(&mut tree.document, None);

    assert_eq!(
        color(&tree.document, tree.at("i1")),
        (44, 55, 66),
        "the nearer definition wins and the value is resolved, not left as a token stream"
    );
    assert_eq!(
        color(&tree.document, tree.at("title")),
        (77, 88, 99),
        "the fallback arm of a variable reference"
    );
}

#[test]
fn a_class_change_restyles_exactly_what_the_rule_reaches() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        ".item { color: rgb(1, 1, 1) } .item.chosen { color: rgb(9, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    assert_eq!(color(&tree.document, tree.at("i1")), (1, 1, 1));

    let target = tree.at("i1");
    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("chosen")],
    );
    let pass = engine.restyle(&mut tree.document, None);

    assert!(pass.traversed);
    assert_eq!(
        color(&tree.document, target),
        (9, 0, 0),
        "the changed element takes the new rule"
    );
    assert_eq!(
        pass.restyled, 1,
        "and it is the only element the engine had to restyle"
    );
}

#[test]
fn skipping_the_ancestor_marking_restyles_nothing_at_all() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        ".item { color: rgb(1, 1, 1) } .item.chosen { color: rgb(9, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");
    edit::set_classes_without_marking(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("chosen")],
    );
    let pass = engine.restyle(&mut tree.document, None);

    assert_eq!(
        pass.restyled, 0,
        "without the ancestor marking the traversal never descends to the changed element"
    );
    assert_eq!(
        color(&tree.document, target),
        (1, 1, 1),
        "and the failure is a silently stale colour, not an error"
    );
}

#[test]
fn a_state_write_restyles_the_element_it_was_written_on() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        ".item { color: rgb(1, 1, 1) } .item:hover { color: rgb(9, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");
    edit::set_state(
        &tree.document,
        target,
        zgui_dom::node::element::state::to_engine(UiState::HOVER),
        true,
    );
    engine.restyle(&mut tree.document, None);
    assert_eq!(color(&tree.document, target), (9, 0, 0));
}

#[test]
fn removing_a_class_re_matches_the_sibling_that_matched_through_a_combinator() {
    let mut tree = fixture::page();
    let mut engine = engine_for(
        &tree.document,
        ".item { color: rgb(1, 1, 1) } .hot + .item { color: rgb(4, 4, 4) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);
    assert_eq!(color(&tree.document, tree.at("i3")), (4, 4, 4));

    // The change is on `i2`, and the element whose style it changes is `i3`. Nothing marks `i3`,
    // so the whole of that half comes from the engine's own invalidation over the recorded
    // snapshot — which is reachable only because the mark on `i2` made the traversal descend to
    // their shared parent.
    let hot = tree.at("i2");
    edit::set_classes(&tree.document, hot, &[ClassName::new("item")]);
    engine.restyle(&mut tree.document, None);
    assert_eq!(
        color(&tree.document, tree.at("i3")),
        (1, 1, 1),
        "the sibling stopped matching and has to lose the colour it had"
    );
}
