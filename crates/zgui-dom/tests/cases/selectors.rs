//! Does every selector shape match exactly the elements it should?
//!
//! Each pair below is a selector and the elements it must match, over one of two fixtures whose
//! shapes are described where they are built. The instrument is a computed value rather than a call
//! into the matching machinery: the selector is given a rule setting a **reset** property, the
//! document is styled for real, and the elements that came out with a non-initial value are the
//! elements the selector matched. A reset property rather than an inherited one, because an
//! inherited one cannot tell an element a rule matched from a descendant that merely inherited.
//!
//! **Every pair is checked at parse first.** A selector this build rejects takes its whole rule with
//! it, so a matching expectation of "nothing" would be satisfied by applying an empty sheet. The
//! syntaxes this build rejects are pinned in their own module, and none of them appears here.
//!
//! Two expectations are worth pointing at because they look like bugs and are not. `[id]` and
//! `[class]` match nothing: an element's identifier and its classes are not in its attribute table —
//! they live in the node record, where matching asks about them far more cheaply — so an attribute
//! selector naming either finds no attribute. And every positional expectation is written as though
//! the text nodes and the marker in the fixtures were not there, because they take no position:
//! a node between two elements must not shift either one, or every sibling combinator in a document
//! containing text answers differently from one that does not.

use crate::support::engine::Engine;
use crate::support::fixture::{self, Tree};
use crate::support::read::radius;

/// Which elements of `tree` the rule `selector` matches.
///
/// # Panics
///
/// Panics if `selector` does not parse, because a dropped rule would make every expectation of
/// "nothing" pass for the wrong reason.
fn matched(tree: &mut Tree, selector: &str) -> Vec<&'static str> {
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(&format!("{selector} {{ border-top-left-radius: 3px }}"));
    assert!(
        engine.errors().messages().is_empty(),
        "`{selector}` did not parse in this build, so the rule it is in was dropped whole: {:?}",
        engine.errors().messages()
    );
    engine.restyle(&mut tree.document, None);
    tree.names_where(|document, index| radius(document, index) != 0.0)
}

/// Runs one table of pairs against a freshly built fixture each time.
fn check(build: fn() -> Tree, pairs: &[(&str, &[&str])]) {
    let mut wrong = Vec::new();
    for (selector, expected) in pairs {
        let mut tree = build();
        let got = matched(&mut tree, selector);
        if got != *expected {
            wrong.push(format!(
                "{selector}\n  expected {expected:?}\n  matched  {got:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} selector pairs disagreed:\n{}",
        wrong.len(),
        pairs.len(),
        wrong.join("\n")
    );
}

/// Selector and expected match set, over the page fixture.
const PAGE: [(&str, &[&str]); 164] = [
    (r#"root"#, &["root"]),
    (r#"header"#, &["header"]),
    (r#"label"#, &["title", "leaf"]),
    (r#"span"#, &["badge"]),
    (r#"nav"#, &["nav"]),
    (r#"a"#, &["linkA", "linkB"]),
    (
        r#"box"#,
        &[
            "card", "i1", "i2", "i3", "i4", "empty", "card2", "i5", "deep",
        ],
    ),
    (r#"button"#, &["save", "cancel"]),
    (r#"input"#, &["check"]),
    (r#"form"#, &["form"]),
    (r#"nosuchtag"#, &[]),
    (
        r#"*"#,
        &[
            "root", "header", "title", "badge", "nav", "linkA", "linkB", "card", "i1", "i2", "i3",
            "i4", "leaf", "empty", "card2", "i5", "deep", "form", "save", "cancel", "check",
        ],
    ),
    (
        r#"root *"#,
        &[
            "header", "title", "badge", "nav", "linkA", "linkB", "card", "i1", "i2", "i3", "i4",
            "leaf", "empty", "card2", "i5", "deep", "form", "save", "cancel", "check",
        ],
    ),
    (
        r#"root > *"#,
        &["header", "nav", "card", "empty", "card2", "form"],
    ),
    (r#".bar"#, &["header", "nav"]),
    (r#".title"#, &["title"]),
    (r#".badge"#, &["badge"]),
    (r#".sticky"#, &["nav"]),
    (r#".link"#, &["linkA", "linkB"]),
    (r#".active"#, &["linkB"]),
    (r#".card"#, &["card", "empty", "card2"]),
    (r#".item"#, &["i1", "i2", "i3", "i4", "i5"]),
    (r#".hot"#, &["i2"]),
    (r#".last"#, &["i4"]),
    (r#".deep"#, &["deep"]),
    (r#".ctl"#, &["save", "cancel", "check"]),
    (r#".missing"#, &[]),
    (r#".link.active"#, &["linkB"]),
    (r#".card.empty"#, &["empty"]),
    (r#".item.hot"#, &["i2"]),
    (r#"#heading"#, &["title"]),
    (r#"#main"#, &["card"]),
    (r#"#save"#, &["save"]),
    (r#"#nope"#, &[]),
    (r#"#main.card"#, &["card"]),
    (r#"box.card"#, &["card", "empty", "card2"]),
    (r#"label.title"#, &["title"]),
    (r#"a.link"#, &["linkA", "linkB"]),
    (r#"span.badge"#, &["badge"]),
    (r#"button.ctl"#, &["save", "cancel"]),
    (r#"[href]"#, &["linkA", "linkB"]),
    (r#"[data-state]"#, &["linkB"]),
    (r#"[data-kind]"#, &["leaf"]),
    (r#"[missing]"#, &[]),
    (r#"[id]"#, &[]),
    (r#"[class]"#, &[]),
    (r#"[href='/a']"#, &["linkA"]),
    (r#"[href='/b']"#, &["linkB"]),
    (r#"[href^='/']"#, &["linkA", "linkB"]),
    (r#"[href$='a']"#, &["linkA"]),
    (r#"[href*='b']"#, &["linkB"]),
    (r#"[data-kind='leaf']"#, &["leaf"]),
    (r#"[data-kind~='leaf']"#, &["leaf"]),
    (r#"[data-kind|='leaf']"#, &["leaf"]),
    (r#"[data-state='OPEN' i]"#, &["linkB"]),
    (r#"[data-state='OPEN']"#, &[]),
    (r#"[data-state='open' s]"#, &["linkB"]),
    (r#"a[href='/a']"#, &["linkA"]),
    (r#"[href][data-state]"#, &["linkB"]),
    (r#"root label"#, &["title", "leaf"]),
    (r#".card .item"#, &["i1", "i2", "i3", "i4", "i5"]),
    (r#".card box"#, &["i1", "i2", "i3", "i4", "i5", "deep"]),
    (r#"nav a"#, &["linkA", "linkB"]),
    (r#"header label"#, &["title"]),
    (r#".item .deep"#, &["deep"]),
    (r#"root .ctl"#, &["save", "cancel", "check"]),
    (r#".empty box"#, &[]),
    (r#"form button"#, &["save", "cancel"]),
    (r#"box box box"#, &["deep"]),
    (r#"root > label"#, &[]),
    (r#"header > label"#, &["title"]),
    (r#".card > .item"#, &["i1", "i2", "i3", "i4", "i5"]),
    (r#".item > .deep"#, &["deep"]),
    (r#"root > box"#, &["card", "empty", "card2"]),
    (r#"nav > a"#, &["linkA", "linkB"]),
    (r#"form > *"#, &["save", "cancel", "check"]),
    (r#".card > box"#, &["i1", "i2", "i3", "i4", "i5"]),
    (r#"root > root"#, &[]),
    (r#".title + span"#, &["badge"]),
    (r#"a + a"#, &["linkB"]),
    (r#".item + .item"#, &["i2", "i3", "i4"]),
    (r#".hot + .item"#, &["i3"]),
    (r#".item + label"#, &["leaf"]),
    (r#"header + nav"#, &["nav"]),
    (r#"nav + box"#, &["card"]),
    (r#"box + box"#, &["i2", "i3", "i4", "empty", "card2"]),
    (r#"button + button"#, &["cancel"]),
    (r#"button + input"#, &["check"]),
    (r#".title ~ span"#, &["badge"]),
    (r#".hot ~ label"#, &["leaf"]),
    (r#".item ~ .item"#, &["i2", "i3", "i4"]),
    (r#"header ~ box"#, &["card", "empty", "card2"]),
    (r#"a ~ a"#, &["linkB"]),
    (r#".card ~ .card"#, &["empty", "card2"]),
    (r#"nav ~ form"#, &["form"]),
    (r#"input ~ button"#, &[]),
    (r#"label:not(.title)"#, &["leaf"]),
    (r#"box:not(.card)"#, &["i1", "i2", "i3", "i4", "i5", "deep"]),
    (r#".item:not(.hot)"#, &["i1", "i3", "i4", "i5"]),
    (r#"a:not([href])"#, &[]),
    (r#".ctl:not(#save)"#, &["cancel", "check"]),
    (r#"root > *:not(box)"#, &["header", "nav", "form"]),
    (r#".card:not(.empty)"#, &["card", "card2"]),
    (r#"*:not(*)"#, &[]),
    (r#":is(.hot, .last)"#, &["i2", "i4"]),
    (r#":is(label, span)"#, &["title", "badge", "leaf"]),
    (
        r#".card :is(.item, label)"#,
        &["i1", "i2", "i3", "i4", "leaf", "i5"],
    ),
    (r#":where(.hot)"#, &["i2"]),
    (r#":is(#main, #save)"#, &["card", "save"]),
    (r#":is(box):is(.card)"#, &["card", "empty", "card2"]),
    (
        r#":where(nav, header) > *"#,
        &["title", "badge", "linkA", "linkB"],
    ),
    (r#":is(.nothing, .missing)"#, &[]),
    (r#":root"#, &["root"]),
    (
        r#":empty"#,
        &[
            "title", "badge", "linkA", "linkB", "i1", "i2", "i3", "i4", "leaf", "empty", "deep",
            "save", "cancel", "check",
        ],
    ),
    (r#"box:empty"#, &["i1", "i2", "i3", "i4", "empty", "deep"]),
    (
        r#":first-child"#,
        &[
            "root", "header", "title", "linkA", "i1", "i5", "deep", "save",
        ],
    ),
    (
        r#":last-child"#,
        &[
            "root", "badge", "linkB", "leaf", "i5", "deep", "form", "check",
        ],
    ),
    (r#":only-child"#, &["root", "i5", "deep"]),
    (r#"box:only-child"#, &["i5", "deep"]),
    (
        r#":nth-child(1)"#,
        &[
            "root", "header", "title", "linkA", "i1", "i5", "deep", "save",
        ],
    ),
    (
        r#":nth-child(2)"#,
        &["badge", "nav", "linkB", "i2", "cancel"],
    ),
    (
        r#":nth-child(2n)"#,
        &[
            "badge", "nav", "linkB", "i2", "i4", "empty", "form", "cancel",
        ],
    ),
    (
        r#":nth-child(2n+1)"#,
        &[
            "root", "header", "title", "linkA", "card", "i1", "i3", "leaf", "card2", "i5", "deep",
            "save", "check",
        ],
    ),
    (r#":nth-child(3)"#, &["card", "i3", "check"]),
    (
        r#":nth-last-child(1)"#,
        &[
            "root", "badge", "linkB", "leaf", "i5", "deep", "form", "check",
        ],
    ),
    (
        r#":nth-last-child(2)"#,
        &["title", "linkA", "i4", "card2", "cancel"],
    ),
    (
        r#":first-of-type"#,
        &[
            "root", "header", "title", "badge", "nav", "linkA", "card", "i1", "leaf", "i5", "deep",
            "form", "save", "check",
        ],
    ),
    (
        r#":last-of-type"#,
        &[
            "root", "header", "title", "badge", "nav", "linkB", "i4", "leaf", "card2", "i5",
            "deep", "form", "cancel", "check",
        ],
    ),
    (
        r#":only-of-type"#,
        &[
            "root", "header", "title", "badge", "nav", "leaf", "i5", "deep", "form", "check",
        ],
    ),
    (r#"box:nth-of-type(2)"#, &["i2", "empty"]),
    (r#"a:nth-of-type(2)"#, &["linkB"]),
    (r#"box:nth-last-of-type(1)"#, &["i4", "card2", "i5", "deep"]),
    (
        r#":nth-child(odd)"#,
        &[
            "root", "header", "title", "linkA", "card", "i1", "i3", "leaf", "card2", "i5", "deep",
            "save", "check",
        ],
    ),
    (
        r#":nth-child(even)"#,
        &[
            "badge", "nav", "linkB", "i2", "i4", "empty", "form", "cancel",
        ],
    ),
    (r#":hover"#, &["i2"]),
    (r#".item:hover"#, &["i2"]),
    (r#":enabled"#, &["save"]),
    (r#":disabled"#, &["cancel"]),
    (r#":checked"#, &["check"]),
    (r#":link"#, &["linkA"]),
    (r#":any-link"#, &["linkA", "linkB"]),
    (r#"a:link"#, &["linkA"]),
    (r#":visited"#, &["linkB"]),
    (r#":not(:hover).item"#, &["i1", "i3", "i4", "i5"]),
    (r#":active"#, &[]),
    (r#":focus"#, &[]),
    (r#":required"#, &[]),
    (r#".hot, .last"#, &["i2", "i4"]),
    (r#"label, span"#, &["title", "badge", "leaf"]),
    (r#"#main, #save"#, &["card", "save"]),
    (r#".card > .item:first-child"#, &["i1", "i5"]),
    (r#".card > .item:last-child"#, &["i5"]),
    (r#"nav .link:not(.active)"#, &["linkA"]),
    (r#".bar > *"#, &["title", "badge", "linkA", "linkB"]),
    (r#"root > box:nth-child(3)"#, &["card"]),
    (r#".item:nth-child(2n)"#, &["i2", "i4"]),
    (r#"box.card .item + .item"#, &["i2", "i3", "i4"]),
    (r#"[data-kind]:only-of-type"#, &["leaf"]),
    (r#".ctl:enabled"#, &["save"]),
    (r#"form :checked"#, &["check"]),
    (r#".card:empty"#, &["empty"]),
    (r#":root > :first-child"#, &["header"]),
    (
        r#":root :last-child"#,
        &["badge", "linkB", "leaf", "i5", "deep", "form", "check"],
    ),
    (r#"box:not(:empty)"#, &["card", "card2", "i5"]),
];

/// Selector and expected match set, over the list fixture.
const LIST: [(&str, &[&str]); 42] = [
    (
        r#".row"#,
        &["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9"],
    ),
    (r#".even"#, &["r0", "r2", "r4", "r6", "r8"]),
    (r#"li"#, &["r0", "r3", "r6", "r9"]),
    (r#"div"#, &["r1", "r2", "r4", "r5", "r7", "r8"]),
    (r#".list > :first-child"#, &["r0"]),
    (r#".list > :last-child"#, &["r9"]),
    (r#":nth-child(1)"#, &["root", "list", "r0"]),
    (r#".row:nth-child(4)"#, &["r3"]),
    (r#".row:nth-child(2n)"#, &["r1", "r3", "r5", "r7", "r9"]),
    (r#".row:nth-child(2n+1)"#, &["r0", "r2", "r4", "r6", "r8"]),
    (r#".row:nth-child(3n)"#, &["r2", "r5", "r8"]),
    (r#".row:nth-last-child(1)"#, &["r9"]),
    (r#".row:nth-last-child(2)"#, &["r8"]),
    (r#".row:nth-last-child(3n+1)"#, &["r0", "r3", "r6", "r9"]),
    (r#"li:nth-of-type(2)"#, &["r3"]),
    (r#"div:nth-of-type(3)"#, &["r4"]),
    (r#"li:last-of-type"#, &["r9"]),
    (r#"li:first-of-type"#, &["r0"]),
    (r#".even + .row"#, &["r1", "r3", "r5", "r7", "r9"]),
    (r#".row + .even"#, &["r2", "r4", "r6", "r8"]),
    (
        r#".row ~ .row"#,
        &["r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9"],
    ),
    (r#".list :not(.even)"#, &["r1", "r3", "r5", "r7", "r9"]),
    (r#"ul > li"#, &["r0", "r3", "r6", "r9"]),
    (
        r#".row:empty"#,
        &["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9"],
    ),
    (r#":root > *"#, &["list"]),
    (r#"ul:only-child"#, &["list"]),
    (r#".row:only-of-type"#, &[]),
    (r#"li ~ div"#, &["r1", "r2", "r4", "r5", "r7", "r8"]),
    (r#"div + li"#, &["r3", "r6", "r9"]),
    (r#".list > .row:nth-child(-n+3)"#, &["r0", "r1", "r2"]),
    (r#".list > .row:nth-child(n+8)"#, &["r7", "r8", "r9"]),
    (r#".row:nth-child(4n+1)"#, &["r0", "r4", "r8"]),
    (r#"li:nth-last-of-type(1)"#, &["r9"]),
    (r#"li:nth-last-of-type(2)"#, &["r6"]),
    (
        r#".list :is(li, .even)"#,
        &["r0", "r2", "r3", "r4", "r6", "r8", "r9"],
    ),
    (r#".row:not(li):not(.even)"#, &["r1", "r5", "r7"]),
    (
        r#"*"#,
        &[
            "root", "list", "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9",
        ],
    ),
    (r#".list > *:first-child"#, &["r0"]),
    (r#".list > *:last-child"#, &["r9"]),
    (r#".even:nth-of-type(1)"#, &["r0"]),
    (r#".row[class]"#, &[]),
    (r#"ul.list li.row"#, &["r0", "r3", "r6", "r9"]),
];

#[test]
fn every_selector_shape_matches_what_it_should_on_the_page() {
    check(fixture::page, &PAGE);
}

#[test]
fn every_selector_shape_matches_what_it_should_on_the_list() {
    check(fixture::list, &LIST);
}

#[test]
fn the_corpus_is_at_least_two_hundred_pairs() {
    assert!(
        PAGE.len() + LIST.len() >= 200,
        "the corpus is what makes the trait surface a measurement rather than a claim"
    );
}
