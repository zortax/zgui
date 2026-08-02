//! The diagnostics suite.
//!
//! The table below is the whole obligation: every mistake the spelling this grammar replaced had a
//! message for is a row, and each row says what covers it now — a fixture with its message checked
//! in beside it, a statement that the class of mistake cannot be made any more, or the reason the
//! message is not reachable from this crate. A row with none of those is a message that was lost
//! quietly, which is the only failure this file exists to catch.
//!
//! A class that has gone names the shapes that must now compile. A deleted error with no passing
//! counterpart is an error that might have become a different error one token later.

use std::collections::BTreeSet;
use std::path::Path;

/// What covers one class of mistake.
enum Cover {
    /// Fixtures whose checked-in message is the assertion.
    Fixtures(&'static [&'static str]),
    /// The mistake cannot be made, with the shapes that consequently compile.
    Gone {
        /// Why nothing raises it any more.
        because: &'static str,
        /// The fixtures that must compile in its place.
        pass: &'static [&'static str],
    },
    /// The message survives but is not raised from here, with what asserts it instead.
    Elsewhere(&'static str),
}

/// One class of mistake, and what covers it.
struct Row {
    /// What the author did.
    situation: &'static str,
    /// What holds the message to account.
    cover: Cover,
}

/// Every class of mistake the tag spelling had a message for.
const ROWS: &[Row] = &[
    Row {
        situation: "a bare word in node position",
        cover: Cover::Fixtures(&["bare_word_text"]),
    },
    Row {
        situation: "an element with no closing tag",
        cover: Cover::Gone {
            because: "an unbalanced brace is a lexer error, raised before the macro is called",
            pass: &[],
        },
    },
    Row {
        situation: "an opening tag that is never closed",
        cover: Cover::Gone {
            because: "an unbalanced parenthesis is a lexer error, raised before the macro is \
                      called",
            pass: &[],
        },
    },
    Row {
        situation: "a closing tag that does not match what is open",
        cover: Cover::Gone {
            because: "nothing closes a node but the brace the lexer has already matched",
            pass: &[],
        },
    },
    Row {
        situation: "a braced child holding more than one expression",
        cover: Cover::Fixtures(&["braced_child_is_one_expression"]),
    },
    Row {
        situation: "a braced attribute that does not spread a bundle",
        cover: Cover::Fixtures(&["malformed_spread"]),
    },
    Row {
        situation: "a spread with nothing to spread",
        cover: Cover::Fixtures(&["spread_of_nothing"]),
    },
    Row {
        situation: "an attribute namespace that does not exist",
        cover: Cover::Fixtures(&["unknown_attribute"]),
    },
    Row {
        situation: "an event that does not exist",
        cover: Cover::Fixtures(&["unknown_event"]),
    },
    Row {
        situation: "a listener with no handler",
        cover: Cover::Fixtures(&["listener_without_a_handler"]),
    },
    Row {
        situation: "a listener modifier that does not exist, and two that contradict",
        cover: Cover::Fixtures(&["unknown_listener_modifier", "passive_with_prevent"]),
    },
    Row {
        situation: "a state the input system computes, asserted by a view",
        cover: Cover::Fixtures(&["state_not_assertable"]),
    },
    Row {
        situation: "a custom property whose name does not start with `--`",
        cover: Cover::Elsewhere(
            "the message is the element lowering's, and the element vocabulary is not a \
             dependency of this crate; a unit test asserts it in both spellings",
        ),
    },
    Row {
        situation: "a shorthand whose name is not an identifier",
        cover: Cover::Fixtures(&["shorthand_on_a_non_identifier"]),
    },
    Row {
        situation: "a hyphenated name written where a prop belongs",
        cover: Cover::Fixtures(&["name_is_not_an_identifier"]),
    },
    Row {
        situation: "a slot child given to a component that takes none",
        cover: Cover::Fixtures(&["slot_on_a_plain_component"]),
    },
    Row {
        situation: "an element's attribute written on a slot",
        cover: Cover::Fixtures(&["slot_takes_props_of_its_own"]),
    },
    Row {
        situation: "`let:` naming an argument for children that are not there",
        cover: Cover::Fixtures(&["let_without_children"]),
    },
    Row {
        situation: "a whole style attribute forwarded to a component",
        cover: Cover::Fixtures(&["style_on_a_component"]),
    },
    Row {
        situation: "the six errors rustc raises against the builder chain",
        cover: Cover::Fixtures(&[
            "unknown_prop",
            "wrong_prop_type",
            "missing_required_prop",
            "listener_returns_a_value",
            "listener_takes_the_wrong_event",
            "memo_over_a_local_value",
        ]),
    },
    Row {
        situation: "a component whose name is not upper camel case",
        cover: Cover::Fixtures(&["lower_case_component"]),
    },
    Row {
        situation: "a comparison read as the end of an opening tag",
        cover: Cover::Gone {
            because: "`flag = a > b` is a comparison, and a value ends at the `,` or `)` after it",
            pass: &["cut_pass_comparison"],
        },
    },
    Row {
        situation: "a comparison inside a closure, read as the end of an opening tag",
        cover: Cover::Gone {
            because: "the closure is one expression like any other",
            pass: &["cut_pass_closure_comparison"],
        },
    },
    Row {
        situation: "a generic argument list written without a turbofish",
        cover: Cover::Fixtures(&["cut_generic_without_turbofish"]),
    },
    Row {
        situation: "a shift read as two closing tags",
        cover: Cover::Gone {
            because: "`bits >> 2` is a shift",
            pass: &["cut_pass_shift"],
        },
    },
    Row {
        situation: "a bare call written in child position",
        cover: Cover::Elsewhere(
            "a call is a node, so the name resolves against the element vocabulary and the error \
             is the resolver's",
        ),
    },
];

/// The messages the call-and-block spelling gains, each with the fixture that asserts it.
const GAINED: &[(&str, &str)] = &[
    ("a name that is not a node", "bare_word_text"),
    ("two attributes with no comma", "missing_comma"),
    (
        "a struct literal written as a value",
        "struct_literal_value",
    ),
    ("a second attribute list", "second_attribute_list"),
    ("a tag written where a node belongs", "tag_in_node_position"),
    ("a word control flow uses, as a name", "reserved_head"),
    (
        "a spread written among the children",
        "spread_among_the_children",
    ),
    ("a macro call written as a child", "macro_child"),
];

/// The control flow the keywords refuse, each with the fixture that asserts what it is told.
///
/// A keyword is safe because the head it takes is a closure by token rather than by type, so the
/// spelling that reads its collection or its condition once has no way of being written. These
/// rows are what that costs and what each mistake is given in exchange.
const FLOW: &[(&str, &str)] = &[
    ("a collection read once", "flow_collection_read_once"),
    (
        "a Rust range written as a collection",
        "flow_collection_is_a_range",
    ),
    ("a condition read once", "flow_condition_read_once"),
    (
        "a bare name written as a condition",
        "flow_condition_is_a_name",
    ),
    ("a list with no key", "flow_list_without_a_key"),
    ("a row bound to a pattern", "flow_row_binds_one_name"),
    (
        "an alternative written after a list",
        "flow_list_with_an_alternative",
    ),
    ("a pattern match written as a condition", "flow_if_let"),
    (
        "an attribute list written on control flow",
        "flow_control_takes_no_attributes",
    ),
    ("a list with no row", "flow_list_without_a_row"),
    (
        "a conditional with no body",
        "flow_conditional_without_a_body",
    ),
    (
        "one conditional chained onto another",
        "flow_chained_conditional",
    ),
    (
        "a conditional written where its component is not in scope",
        "flow_conditional_needs_show",
    ),
];

/// The shapes that must stay legal, each one a hazard that a refactor could turn into an error.
const HAZARDS: &[&str] = &["adjacent_braced_sibling", "doubled_brace"];

/// The shapes that must compile because everything about them is right.
///
/// A suite made only of mistakes proves that the wrong thing fails and says nothing about whether
/// the right thing still works, so the authoring surface's densest correct use is a case here: a
/// component with a variants table, a scoped sheet, defaulted and converted props, a forwarded
/// bundle, a state binding and a typed listener.
const SHAPES: &[&str] = &[
    "button",
    "listener_bound_to_a_name",
    "cut_pass_a11y_role",
    "cut_pass_state_closure",
];

/// Where the fixtures live.
const DIRECTORY: &str = "tests/ui";

/// The fixtures whose message is checked in.
fn failing() -> BTreeSet<&'static str> {
    let rows = ROWS.iter().filter_map(|row| match row.cover {
        Cover::Fixtures(fixtures) => Some(fixtures),
        _ => None,
    });
    rows.flatten()
        .copied()
        .chain(GAINED.iter().map(|(_, fixture)| *fixture))
        .chain(FLOW.iter().map(|(_, fixture)| *fixture))
        .collect()
}

/// The fixtures that must compile.
fn passing() -> BTreeSet<&'static str> {
    let rows = ROWS.iter().filter_map(|row| match row.cover {
        Cover::Gone { pass, .. } => Some(pass),
        _ => None,
    });
    rows.flatten()
        .copied()
        .chain(HAZARDS.iter().copied())
        .chain(SHAPES.iter().copied())
        .collect()
}

#[test]
fn the_diagnostics_say_what_is_wrong() {
    let suite = trybuild::TestCases::new();
    for fixture in failing() {
        suite.compile_fail(format!("{DIRECTORY}/{fixture}.rs"));
    }
    for fixture in passing() {
        suite.pass(format!("{DIRECTORY}/{fixture}.rs"));
    }
}

/// The table is exhaustive over the tag spelling's messages, so its length is asserted rather than
/// left to grow by accident: a row that goes missing is a message nobody has to answer for.
#[test]
fn every_message_the_tag_spelling_had_is_answered_for() {
    assert_eq!(ROWS.len(), 26);
    assert_eq!(GAINED.len(), 8);
    assert_eq!(FLOW.len(), 13);
    let mut situations = BTreeSet::new();
    for row in ROWS {
        assert!(
            situations.insert(row.situation),
            "{} is two rows",
            row.situation
        );
        // A cover that says nothing covers nothing: a class of mistake that has gone owes the
        // reason it cannot be made, and a message raised elsewhere owes what asserts it there.
        let stated = match row.cover {
            Cover::Fixtures(fixtures) => !fixtures.is_empty(),
            Cover::Gone { because, .. } => !because.is_empty(),
            Cover::Elsewhere(instead) => !instead.is_empty(),
        };
        assert!(stated, "{}: nothing covers it", row.situation);
    }
}

#[test]
fn every_fixture_the_table_names_is_checked_in_with_its_message() {
    for fixture in failing() {
        let source = format!("{DIRECTORY}/{fixture}.rs");
        let message = format!("{DIRECTORY}/{fixture}.stderr");
        assert!(Path::new(&source).exists(), "{source} is missing");
        assert!(Path::new(&message).exists(), "{message} is missing");
    }
    for fixture in passing() {
        let source = format!("{DIRECTORY}/{fixture}.rs");
        assert!(Path::new(&source).exists(), "{source} is missing");
        let message = format!("{DIRECTORY}/{fixture}.stderr");
        assert!(!Path::new(&message).exists(), "{message} must compile");
    }
}

/// A fixture the table does not name is a fixture nothing runs, which is worse than not having it.
#[test]
fn every_fixture_checked_in_is_named_by_the_table() {
    let named: BTreeSet<&str> = failing().union(&passing()).copied().collect();
    for entry in std::fs::read_dir(DIRECTORY).expect("the fixtures are there") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("a fixture has a name");
        assert!(named.contains(stem), "{stem} is named by no row");
    }
}
