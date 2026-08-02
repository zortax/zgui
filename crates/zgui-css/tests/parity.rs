//! The parity register, checked against the engine it describes.
//!
//! Every assertion here asks the running engine a question rather than restating an answer. That is
//! the whole point of the register: a row that has quietly stopped being true — because a flag was
//! flipped, because the engine was upgraded, or because someone patched it — has to fail here
//! rather than go on describing a build that no longer exists.

use zgui_css::parity::gap::inherited_svg;
use zgui_css::parity::{
    AbsentReason, EngineStatus, Expectation, GAPS, GapProbe, ParityError, Registration, Registry,
    Support, complaints, selector_is_accepted, status_of,
};
use zgui_css::prefs::enable_css_features;

/// The register describes the engine as this framework configures it, so it has to be configured.
fn configured() {
    enable_css_features();
}

#[test]
fn every_gap_row_still_describes_this_build() {
    configured();
    let stale: Vec<&str> = GAPS
        .iter()
        .filter(|gap| !gap.holds())
        .map(|gap| gap.subject)
        .collect();
    assert!(
        stale.is_empty(),
        "these became reachable and nothing downstream was told: {stale:?}"
    );
}

#[test]
fn the_seeded_rows_cover_every_parity_fact_running_code_established() {
    configured();
    let subjects: Vec<&str> = GAPS.iter().map(|gap| gap.subject).collect();
    assert!(subjects.contains(&":has()"));
    assert!(subjects.contains(&":nth-child(An+B of S)"));
    assert!(subjects.contains(&"::first-line"));
    assert!(GAPS.iter().any(|gap| matches!(
        gap.probe,
        GapProbe::LonghandsUnknown(rows) if rows.len() == 21
    )));
}

#[test]
fn a_rejected_selector_takes_its_whole_rule_with_it() {
    configured();
    // This is what makes the register worth having. A test written against `:has()` in this build
    // asserts something about an *empty* sheet, and passes, because the rule never existed — so the
    // question has to be asked of the parser before it is asked of the matcher.
    let dropped = complaints(".card:has(.item) { border-top-left-radius: 3px }");
    assert_eq!(
        dropped.len(),
        1,
        "the parser has to complain; if it stops, the feature started working: {dropped:?}"
    );
    assert!(
        selector_is_accepted(".card .item"),
        "an ordinary descendant combinator still parses"
    );
}

#[test]
fn every_svg_paint_longhand_is_a_name_this_build_has_never_heard_of() {
    configured();
    assert_eq!(inherited_svg::REGISTERED.len(), 21);
    for row in inherited_svg::REGISTERED {
        assert_eq!(
            row.support(),
            Support::Absent(AbsentReason::GeckoOnly),
            "{} is declared as something else",
            row.css_name()
        );
        assert_eq!(
            status_of(&row.css_name()),
            EngineStatus::Unknown,
            "{} became a property this build knows",
            row.css_name()
        );
        row.check().expect("the declaration matches the engine");
    }
}

#[test]
fn a_registry_of_the_seeded_declarations_is_consistent_with_the_engine() {
    configured();
    let mut registry = Registry::new();
    registry
        .extend(inherited_svg::REGISTERED)
        .expect("no longhand is declared twice");

    assert_eq!(registry.len(), 21);
    assert_eq!(registry.counts().absent, 21);
    assert_eq!(registry.counts().implemented, 0);
    assert_eq!(registry.counts().total(), 21);
    assert!(registry.check().is_empty());
}

#[test]
fn a_declaration_the_engine_contradicts_is_an_error_and_not_a_comment() {
    configured();

    // Claiming a live longhand is unavailable.
    let wrong_way = Registration::new("color", Support::Absent(AbsentReason::GeckoOnly));
    assert_eq!(
        wrong_way.check(),
        Err(ParityError::Stale {
            css_name: "color".to_owned(),
            expected: Expectation::Unknown,
            found: Expectation::Enabled,
        })
    );

    // Claiming a property this build does not generate is consumed.
    let other_way = Registration::new("fill_opacity", Support::Implemented("zgui-paint"));
    assert_eq!(
        other_way.check(),
        Err(ParityError::Stale {
            css_name: "fill-opacity".to_owned(),
            expected: Expectation::Enabled,
            found: Expectation::Unknown,
        })
    );

    // Naming a shorthand at all.
    let not_a_longhand = Registration::new("margin", Support::Implemented("zgui-layout"));
    assert_eq!(
        not_a_longhand.check(),
        Err(ParityError::NotALonghand {
            css_name: "margin".to_owned()
        })
    );
}

#[test]
fn a_longhand_nobody_declared_is_the_case_the_gate_exists_for() {
    configured();
    let mut registry = Registry::new();
    registry
        .insert(Registration::new(
            "color",
            Support::Implemented("zgui-paint"),
        ))
        .expect("a fresh register");

    assert_eq!(
        registry.unclassified(["color", "opacity", "border-top-left-radius"]),
        vec!["opacity".to_owned(), "border-top-left-radius".to_owned()]
    );
}

#[test]
fn one_longhand_cannot_have_two_answers() {
    let mut registry = Registry::new();
    let first = Registration::new("quotes", Support::Ignored("no content lowering yet"));
    registry.insert(first).expect("a fresh register");

    // The same answer twice is one answer, not a conflict.
    registry.insert(first).expect("re-declaring identically");
    assert_eq!(registry.len(), 1);

    let conflict = registry
        .insert(Registration::new(
            "quotes",
            Support::Implemented("zgui-paint"),
        ))
        .expect_err("two different answers for one property");
    assert_eq!(conflict.css_name, "quotes");
}
