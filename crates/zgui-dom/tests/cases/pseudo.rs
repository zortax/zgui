//! Does an element get a style for `::before`, and only when a rule says so?
//!
//! There is no pseudo-element node in this document, so the only observable evidence that generated
//! content will ever exist is a style stored against the originating element. That is what the first
//! case checks, and it checks both directions: a rule that names the pseudo-element produces one, and
//! a rule that does not produces none. Answering "no" to whether an element may generate a
//! pseudo-element would make the first half permanently empty, and nothing else in the pipeline would
//! notice until a box failed to appear.
//!
//! The rest are the four shapes in which a pseudo-element *in the sibling chain* would corrupt
//! matching. Under a design with no such node they pass by construction; they are here as the guard
//! against a future change that re-materialises one, because the failure would be a wrong `:nth-child`
//! answer rather than an error.

use style::selector_parser::PseudoElement;

use crate::support::engine::{Engine, has_pseudo_style};
use crate::support::fixture;
use crate::support::read::radius;

#[test]
fn a_rule_naming_before_stores_a_style_for_it_and_a_rule_that_does_not_stores_none() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(".item::before { content: \"\\2022\" } .card { color: rgb(1, 1, 1) }");
    engine.restyle(&mut tree.document, None);

    assert!(
        has_pseudo_style(&tree.document, tree.at("i1"), &PseudoElement::Before),
        "an element a `::before` rule matches has to come out with a style for it, or there is \
         nothing for generated content to be built from"
    );
    assert!(
        !has_pseudo_style(&tree.document, tree.at("card"), &PseudoElement::Before),
        "and an element no such rule matches must not, or every element in the document would \
         claim to generate content"
    );
}

#[test]
fn a_first_child_rule_is_unaffected_by_a_before_rule_on_the_same_elements() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".card > .item::before { content: \"x\" }
         .card > .item:first-child { border-top-left-radius: 3px }",
    );
    engine.restyle(&mut tree.document, None);

    assert_eq!(radius(&tree.document, tree.at("i1")), 3.0);
    assert_eq!(radius(&tree.document, tree.at("i2")), 0.0);
}

#[test]
fn an_nth_child_rule_is_unaffected_by_a_before_rule_on_the_same_elements() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".card > *::before { content: \"x\" }
         .card > :nth-child(2n) { border-top-left-radius: 3px }",
    );
    engine.restyle(&mut tree.document, None);

    // The card's element children are, in order: i1 i2 i3 i4 leaf. A text node and a marker sit
    // among them and neither takes a position.
    assert_eq!(radius(&tree.document, tree.at("i1")), 0.0);
    assert_eq!(radius(&tree.document, tree.at("i2")), 3.0);
    assert_eq!(radius(&tree.document, tree.at("i3")), 0.0);
    assert_eq!(radius(&tree.document, tree.at("i4")), 3.0);
    assert_eq!(radius(&tree.document, tree.at("leaf")), 0.0);
}

#[test]
fn an_empty_rule_alongside_a_before_rule_does_not_oscillate() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        "box:empty { border-top-left-radius: 7px }
         box::before { content: \"x\" }",
    );
    let first = engine.restyle(&mut tree.document, None);
    assert!(first.traversed);
    assert_eq!(radius(&tree.document, tree.at("empty")), 7.0);

    let second = engine.restyle(&mut tree.document, None);
    assert_eq!(
        second.restyled, 0,
        "generating content must not make an element stop being empty, or the two rules would \
         restyle each other for ever"
    );
    assert_eq!(radius(&tree.document, tree.at("empty")), 7.0);
}

#[test]
fn a_next_sibling_rule_is_unaffected_by_a_before_rule_on_the_same_elements() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".item::before { content: \"x\" }
         .hot + .item { border-top-left-radius: 3px }",
    );
    engine.restyle(&mut tree.document, None);

    assert_eq!(radius(&tree.document, tree.at("i3")), 3.0);
    assert_eq!(radius(&tree.document, tree.at("i2")), 0.0);
}
