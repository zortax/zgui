//! The surface, the sheets, and the two ways a change to either reaches a document.
//!
//! Every case here is about an input that marks *nothing* on any node — a resize, a media-query
//! boundary, a sheet installed or replaced — which is exactly the class of input a pipeline can
//! silently ignore. The assertions are therefore never "the diagnostics say so"; they are always
//! "the computed value changed".

#[path = "support/mod.rs"]
mod support;

use style::values::computed::Display;
use support::{Harness, color, display, radius, width};
use zgui_bits::Dirty;
use zgui_style::{DropKind, SheetOrigin};

#[test]
fn the_user_agent_sheet_gives_the_element_vocabulary_its_defaults() {
    let mut harness = Harness::new();
    let row = harness.append(harness.root, "row");
    let label = harness.append(harness.root, "label");
    let hidden = harness.append(harness.root, "box");
    harness.edit(|edit| {
        edit.set_attribute(
            hidden,
            zgui_interned::AttrName::new("hidden"),
            Some(zgui_vocab::SharedString::from("")),
        );
    });

    harness.frame();

    assert_eq!(display(&harness, row).inside(), Display::Flex.inside());
    assert_eq!(display(&harness, label), Display::Inline);
    assert!(
        display(&harness, hidden).is_none(),
        "the `[hidden]` rule is what makes a hidden element generate no box"
    );
}

#[test]
fn a_sheet_installs_whole_and_names_every_item_it_dropped_with_its_location() {
    let mut harness = Harness::new();
    let diagnostics = harness.add_author(
        "root { not-a-property: 3 }\n\
         .card:has(.title) { color: rgb(9, 0, 0) }\n\
         @container (min-width: 10px) { root { color: rgb(8, 0, 0) } }\n\
         root { color: rgb(1, 2, 3) }\n",
    );

    harness.frame();

    assert_eq!(
        color(&harness, harness.root),
        (1, 2, 3),
        "the valid rule applies, so the sheet installed rather than being rejected whole"
    );

    let kinds: Vec<DropKind> = diagnostics.iter().map(|entry| entry.kind).collect();
    assert!(
        kinds.contains(&DropKind::Declaration),
        "the unknown property has to be reported: {diagnostics:?}"
    );
    assert!(
        kinds.contains(&DropKind::Rule),
        "a rejected selector drops the whole rule, which has to be reported: {diagnostics:?}"
    );
    assert!(
        kinds.contains(&DropKind::AtRule),
        "an at-rule this build does not implement has to be reported: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|entry| entry.location.line < 4),
        "every entry carries the line it was dropped at: {diagnostics:?}"
    );
    // The three are on three different lines, which is the half that makes the locations useful.
    let mut lines: Vec<u32> = diagnostics
        .iter()
        .map(|entry| entry.location.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    assert_eq!(
        lines.len(),
        3,
        "one location per dropped item: {diagnostics:?}"
    );
}

#[test]
fn a_has_selector_is_dropped_by_this_build_rather_than_merely_never_matching() {
    // The vacuity guard for the case above and for anything else written against `:has()`: the
    // parser rejects it, so a test that asserted "nothing matched" would be asserting that an
    // empty sheet applied.
    let mut harness = Harness::new();
    let diagnostics = harness.add_author(".card:has(.title) { color: rgb(9, 0, 0) }");
    assert!(
        diagnostics.iter().any(|entry| entry.kind == DropKind::Rule),
        "this build's selector parser answers `false` for `:has()`, so the rule is dropped"
    );
}

#[test]
fn resize_without_crossing_a_query_boundary_restyles_nothing() {
    let mut harness = Harness::new();
    harness.add_author("@media (min-width: 768px) { root { color: rgb(9, 0, 0) } }");
    harness.frame();
    harness.retire_all();

    let epoch = harness.resize(1000.0, 800.0);
    assert!(epoch.changed);
    assert!(
        epoch.origins.is_empty(),
        "1280 and 1000 are on the same side of the boundary, so no origin was disturbed"
    );
    assert!(!epoch.viewport_units, "nothing resolved a viewport unit");

    let pass = harness.frame();
    assert_eq!(pass.styled, 0, "nothing needed styling again");
    assert!(!pass.traversed);
}

#[test]
fn a_resize_marks_the_relayout_the_later_stages_are_gated_on() {
    let mut harness = Harness::new();
    harness.frame();
    harness.retire_all();

    let epoch = harness.resize(1000.0, 800.0);
    assert_eq!(
        epoch.relaid_out, 1,
        "the root, and the layout cache does the rest"
    );
    assert!(
        harness.owed(harness.root).contains(Dirty::RELAYOUT),
        "without this bit nothing downstream re-runs and a resized window keeps its old layout"
    );
}

#[test]
fn a_pixel_ratio_change_relays_out_every_element_rather_than_only_the_root() {
    let mut harness = Harness::new();
    let child = harness.append(harness.root, "box");
    let grandchild = harness.append(child, "box");
    harness.frame();
    harness.retire_all();

    let epoch = harness.rescale(2.0);
    assert_eq!(epoch.relaid_out, 3);
    for node in [harness.root, child, grandchild] {
        assert!(
            harness.owed(node).contains(Dirty::RELAYOUT),
            "every box is snapped to a different device pixel grid"
        );
    }
}

#[test]
fn crossing_a_media_boundary_restyles_and_flips_the_matched_rule() {
    let mut harness = Harness::new();
    let sidebar = harness.append(harness.root, "box");
    harness.set_classes(sidebar, &["sidebar"]);
    harness.add_author("@media (min-width: 768px) { .sidebar { display: none } }");
    harness.frame();
    assert!(display(&harness, sidebar).is_none());
    harness.retire_all();

    let epoch = harness.resize(500.0, 800.0);
    assert!(
        !epoch.origins.is_empty(),
        "the query answers differently now, so its origin was disturbed"
    );

    let pass = harness.frame();
    assert!(
        !display(&harness, sidebar).is_none(),
        "the rule stopped matching, so the computed display went back to the vocabulary default"
    );
    assert_eq!(
        pass.styled,
        harness.element_count(),
        "the honest cost of re-collecting a whole origin's rules"
    );
}

#[test]
fn a_resize_re_resolves_the_viewport_units_the_previous_device_answered() {
    let mut harness = Harness::new();
    harness.add_author("root { width: 50vw }");
    harness.frame();
    assert_eq!(width(&harness, harness.root), 640.0);
    harness.retire_all();

    let epoch = harness.resize(800.0, 600.0);
    assert!(
        epoch.viewport_units,
        "the outgoing device answered a viewport-unit question, which is why it is asked and not \
         the fresh one"
    );
    assert_eq!(epoch.units_invalidated, 1);

    let pass = harness.frame();
    assert!(pass.traversed);
    assert_eq!(
        width(&harness, harness.root),
        400.0,
        "a viewport unit resolves at computed-value time, so relaying out could never fix it"
    );
}

#[test]
fn replace_sheet_alone_applies_the_changed_rule() {
    let mut harness = Harness::new();
    harness.add_author("root { color: rgb(1, 2, 3) }");
    harness.frame();
    assert_eq!(color(&harness, harness.root), (1, 2, 3));
    harness.retire_all();

    // No mutation, no resize, no input of any kind: the frame restyles because the rule set says
    // its sheets changed, and nothing on any node says anything.
    let diagnostics = harness.replace(0, "root { color: rgb(4, 5, 6) }");
    assert!(diagnostics.is_empty());

    let pass = harness.frame();
    assert!(pass.traversed, "the sheet change is the frame's only input");
    assert_eq!(color(&harness, harness.root), (4, 5, 6));
}

#[test]
fn replacing_a_sheet_keeps_its_place_in_the_cascade() {
    let mut harness = Harness::new();
    harness.add_author("root { color: rgb(1, 0, 0) }");
    harness.add_author("root { color: rgb(2, 0, 0) }");
    harness.frame();
    assert_eq!(
        color(&harness, harness.root),
        (2, 0, 0),
        "the later sheet wins at equal specificity"
    );

    harness.replace(0, "root { color: rgb(3, 0, 0) }");
    harness.frame();
    assert_eq!(
        color(&harness, harness.root),
        (2, 0, 0),
        "a replacement that moved to the end of the origin would win here, and must not"
    );
}

#[test]
fn dropping_a_handle_removes_the_sheet_from_the_next_frame() {
    let mut harness = Harness::new();
    harness.add_author("root { color: rgb(7, 0, 0) }");
    harness.frame();
    assert_eq!(color(&harness, harness.root), (7, 0, 0));

    harness.drop_sheet(0);
    let pass = harness.frame();
    assert!(pass.traversed, "the removal is the frame's only input");
    assert_ne!(
        color(&harness, harness.root),
        (7, 0, 0),
        "the rule went with the sheet"
    );
}

#[test]
fn the_origins_cascade_in_order_and_reverse_for_important_declarations() {
    let mut harness = Harness::new();
    harness.add_sheet(SheetOrigin::UserAgent, "root { color: rgb(1, 0, 0) }");
    harness.add_sheet(SheetOrigin::User, "root { color: rgb(2, 0, 0) }");
    harness.add_sheet(SheetOrigin::Author, "root { color: rgb(3, 0, 0) }");
    harness.frame();
    assert_eq!(color(&harness, harness.root), (3, 0, 0));

    harness.add_sheet(
        SheetOrigin::UserAgent,
        "root { color: rgb(4, 0, 0) !important }",
    );
    harness.frame();
    assert_eq!(
        color(&harness, harness.root),
        (4, 0, 0),
        "the order reverses for important declarations"
    );
}

#[test]
fn a_named_sheet_comes_from_the_loader_installed_on_the_document() {
    use std::sync::Arc;
    use zgui_style::EmbeddedSheets;

    let mut harness = Harness::new();
    harness.document.install_sheet_loader(Arc::new(
        EmbeddedSheets::new().with("theme.css", "root { border-top-left-radius: 4px }"),
    ));
    harness.add_named(SheetOrigin::Author, "theme.css");

    harness.frame();
    assert_eq!(radius(&harness, harness.root), 4.0);
}

#[test]
fn an_import_is_resolved_through_the_same_loader_during_the_parse() {
    use std::sync::Arc;
    use zgui_style::EmbeddedSheets;

    let mut harness = Harness::new();
    harness.document.install_sheet_loader(Arc::new(
        EmbeddedSheets::new().with("imported.css", "root { color: rgb(5, 5, 5) }"),
    ));
    let diagnostics = harness.add_author("@import url(\"imported.css\");");

    assert!(
        diagnostics.is_empty(),
        "the loader answered, so nothing was dropped: {diagnostics:?}"
    );
    harness.frame();
    assert_eq!(color(&harness, harness.root), (5, 5, 5));
}

#[test]
fn a_sheet_inserted_before_another_loses_to_it_at_equal_specificity() {
    let mut harness = Harness::new();
    // Two rules of equal specificity in one origin: the later sheet wins, so where a sheet is
    // placed is the whole of the answer.
    harness.add_author("root { color: rgb(7, 7, 7) }");
    let earlier = harness.insert_before(SheetOrigin::Author, "root { color: rgb(3, 3, 3) }", 0);
    assert!(earlier.is_empty(), "both rules parse: {earlier:?}");

    harness.frame();
    assert_eq!(
        color(&harness, harness.root),
        (7, 7, 7),
        "the sheet inserted before the first one comes earlier in the cascade, so it loses"
    );

    // The control: appended instead of inserted, the same declaration wins. Without it this case
    // would pass against an `insert_sheet_before` that installed nothing at all.
    harness.add_author("root { color: rgb(3, 3, 3) }");
    harness.frame();
    assert_eq!(color(&harness, harness.root), (3, 3, 3));
}
