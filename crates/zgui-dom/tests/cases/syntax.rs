//! Which selector syntaxes does this build of the engine actually accept?
//!
//! A selector the parser rejects is not a rule that fails to match, it is a rule that is not there —
//! the whole rule is dropped, every declaration in it with it. So every matching test in this target
//! passes trivially if its rule was dropped, and the question "does this syntax exist at all" has to
//! be asked separately, of the parser rather than of the matcher.
//!
//! Two of the answers are hardcoded in this build rather than read from a preference, so no runtime
//! flip reaches them. Pinning the whole set here means that a future release changing any of them
//! changes a test result instead of going unnoticed.

use selectors::matching::ElementSelectorFlags;
use zgui_dom::Document;

use crate::support::engine::{Engine, for_each_element};
use crate::support::fixture;
use crate::support::read::radius;
use crate::support::sheets::selector_parses;

/// Every syntax a framework aiming at CSS parity is expected to have, and whether this build takes
/// it.
const SYNTAXES: [(&str, bool); 12] = [
    ("box:has(label)", false),
    ("box:has(> label)", false),
    ("box:has(+ label)", false),
    ("box:nth-child(2 of .item)", false),
    ("box:dir(rtl)", false),
    ("box::first-line", false),
    ("box:is(.card, .btn)", true),
    ("box:where(.card)", true),
    ("box:not(.muted)", true),
    ("box .item", true),
    ("box:nth-child(2n + 1)", true),
    ("box[data-kind='a' i]", true),
];

#[test]
fn the_accepted_selector_syntaxes_are_exactly_these() {
    for (selector, expected) in SYNTAXES {
        assert_eq!(
            selector_parses(selector),
            expected,
            "`{selector}` changed whether it parses in this build of the engine"
        );
    }
}

#[test]
fn a_relative_selector_test_would_pass_while_applying_an_empty_sheet() {
    // This is the trap the matrix above exists to keep out of every other case: the rule below is
    // dropped whole at parse, so a test asserting "nothing matched" would pass without the matcher
    // ever running.
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(".card:has(.item) { border-top-left-radius: 3px }");

    assert!(
        !engine.errors().messages().is_empty(),
        "the parser has to have complained; if it stops complaining the rule started working"
    );
    engine.restyle(&mut tree.document, None);
    assert_eq!(radius(&tree.document, tree.at("card")), 0.0);
}

#[test]
fn no_element_ever_carries_a_relative_selector_search_direction() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".card:has(.item) { color: rgb(9, 0, 0) }
         .card .item { color: rgb(1, 1, 1) }
         .item:nth-child(2 of .item) { color: rgb(2, 2, 2) }",
    );
    engine.restyle(&mut tree.document, None);

    let mut offenders = Vec::new();
    for_each_element(&tree.document, |node| {
        let flags = node.record().selector_flags();
        if flags
            .intersects(ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_ANCESTOR_SIBLING)
        {
            offenders.push(node.index().get());
        }
    });
    assert!(
        offenders.is_empty(),
        "relative-selector bookkeeping is dead code until the parser accepts relative selectors; \
         an element carrying one means it stopped being dead and the invalidation half is missing"
    );
}

#[test]
fn a_sheet_installs_even_when_one_of_its_rules_is_dropped() {
    // Error recovery: the valid rule has to apply, and the dropped one has to be reported rather
    // than silently swallowed.
    let mut tree: fixture::Tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".card:has(.item) { border-top-left-radius: 9px }
         .item { border-top-left-radius: 4px }
         .item { not-a-property: 1px }",
    );
    engine.restyle(&mut tree.document, None);

    let document: &Document = &tree.document;
    assert_eq!(
        radius(document, tree.at("i1")),
        4.0,
        "the valid rule applies"
    );
    assert_eq!(radius(document, tree.at("card")), 0.0);
    let messages = engine.errors().messages();
    assert_eq!(
        messages.len(),
        2,
        "both the rejected selector and the unknown property are reported: {messages:?}"
    );
}
