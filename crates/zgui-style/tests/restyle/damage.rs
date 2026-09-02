//! What the engine every damage level produces makes the rest of the frame owe.

use crate::support::Harness;
use style::selector_parser::RestyleDamage;
use zgui_bits::Dirty;
use zgui_vocab::UiState;

#[test]
fn an_insertion_gets_its_layout_damage_from_the_initial_style_branch() {
    let mut harness = Harness::new();
    harness.add_author("box { color: rgb(1, 1, 1) }");
    harness.frame();
    harness.retire_all();

    let added = harness.append(harness.root, "box");
    let pass = harness.frame();

    let record = pass
        .records
        .iter()
        .find(|record| record.index == added)
        .expect("the traversal styled the new element");
    assert!(record.initial, "it had never been styled before");
    assert!(
        pass.restyled_nodes().is_empty(),
        "a first-time cascade is not a restyle, however much work it did"
    );
    assert_eq!(
        record.damage,
        RestyleDamage::empty(),
        "the engine accumulates no damage for a first-time cascade, which is the whole reason the \
         branch exists"
    );
    assert!(
        harness
            .owed(added)
            .contains(Dirty::RELAYOUT | Dirty::REBUILD_BOX),
        "without the branch a mounted subtree is never laid out and never appears"
    );
}

#[test]
fn a_border_colour_change_repaints_through_the_key_comparison() {
    let mut harness = Harness::new();
    let button = harness.append(harness.root, "control");
    harness.set_classes(button, &["btn"]);
    harness.add_author(
        ".btn { border-top-color: rgb(1, 0, 0) }\n\
         .btn:hover { border-top-color: rgb(2, 0, 0) }",
    );
    harness.frame();
    harness.retire_all();

    harness.set_state(button, UiState::HOVER, true);
    let pass = harness.frame();

    let record = pass
        .records
        .iter()
        .find(|record| record.index == button)
        .expect("the element restyled");
    assert!(
        harness.owed(button).contains(Dirty::REPAINT),
        "the key comparison is what decides a repaint, and it fires on the border group's identity"
    );
    // A recorded fact about this build rather than a design decision: `border-top-color` carries
    // no damage annotation of its own, and the generated predicate for the widest level compares
    // every property of the border group anyway — so the engine reports a *relayout* for a hover
    // that moves one colour. The key comparison is what makes the repaint correct either way, and
    // a release that narrowed this would change the line below rather than the behaviour above.
    assert!(
        record.damage.contains(RestyleDamage::RELAYOUT),
        "this build over-fires here; the day it stops, this line is the record of it: {:?}",
        record.damage
    );
}

#[test]
fn every_damage_level_the_engine_can_produce_maps_to_at_least_one_obligation() {
    /// One property, the value it starts at, and the value it moves to.
    const CASES: &[(&str, &str, &str)] = &[
        // relayout
        ("width", "10px", "20px"),
        // recalculate overflow — from an element that is *already* transformed, because gaining a
        // first transform is a relayout and would pass against a translation that never tests the
        // middle arms
        ("transform", "translateX(1px)", "translateX(2px)"),
        // rebuild stacking context
        ("z-index", "1", "2"),
        // repaint
        ("opacity", "1", "0.5"),
        // no damage at all: the key comparison is the whole predicate here
        ("border-top-color", "rgb(1, 0, 0)", "rgb(2, 0, 0)"),
    ];

    let mut levels = Vec::new();
    for (property, before, after) in CASES {
        let mut harness = Harness::new();
        let node = harness.append(harness.root, "box");
        harness.add_author(&format!(
            "box {{ position: relative; transform: translateX(1px); {property}: {before} }}"
        ));
        harness.frame();
        harness.retire_all();

        harness.replace(
            0,
            &format!(
                "box {{ position: relative; transform: translateX(1px); {property}: {after} }}"
            ),
        );
        let pass = harness.frame();

        let record = pass
            .records
            .iter()
            .find(|record| record.index == node)
            .expect("the element restyled");
        levels.push(record.damage);
        assert!(
            !harness.owed(node).is_empty(),
            "`{property}` moved and produced the engine damage {:?}, which has to become at least \
             one obligation",
            record.damage
        );
    }

    // The vacuity guard: if the fixture stopped producing the distinct levels, every assertion
    // above would still hold while testing one arm four times.
    let distinct: std::collections::BTreeSet<u16> =
        levels.iter().map(|damage| damage.bits()).collect();
    assert!(
        distinct.len() >= 3,
        "the fixture has to reach several damage levels, not one: {levels:?}"
    );
    assert!(
        levels
            .iter()
            .any(|damage| !damage.contains(RestyleDamage::RELAYOUT)),
        "and not all of them are the widest one, or the lattice below the top arm is untested: \
         {levels:?}"
    );
}

#[test]
fn a_transform_change_recalculates_overflow_without_relaying_anything_out() {
    let mut harness = Harness::new();
    let node = harness.append(harness.root, "box");
    harness.add_author("box { transform: translateX(1px) }");
    harness.frame();
    harness.retire_all();

    harness.replace(0, "box { transform: translateX(2px) }");
    harness.frame();

    let owed = harness.owed(node);
    assert!(
        owed.contains(Dirty::REFRAGMENT | Dirty::REHIT | Dirty::REPAINT),
        "the middle arm of the lattice, which a flat sequence of tests would never reach: {owed:?}"
    );
    assert!(
        !owed.contains(Dirty::RELAYOUT),
        "nothing moved in layout, so re-running it would be pure waste"
    );
}

#[test]
fn a_pointer_events_change_rewrites_the_hit_entry_without_touching_the_boxes() {
    let mut harness = Harness::new();
    let node = harness.append(harness.root, "box");
    harness.add_author("box { pointer-events: none }");
    harness.frame();
    harness.retire_all();

    harness.replace(0, "box { pointer-events: auto }");
    harness.frame();

    let owed = harness.owed(node);
    assert!(
        owed.contains(Dirty::REHIT | Dirty::REPAINT),
        "what a hit test answers over the box moved, so its entry is written again: {owed:?}"
    );
    assert!(
        !owed.intersects(Dirty::RELAYOUT | Dirty::REBUILD_BOX | Dirty::RESHAPE),
        "nothing drawn or laid out moved; a property the classifier does not name is taken at \
         the widest cost, which rebuilds every box for a hover: {owed:?}"
    );
}

#[test]
fn a_z_index_change_restacks_without_relaying_anything_out() {
    let mut harness = Harness::new();
    let node = harness.append(harness.root, "box");
    harness.add_author("box { position: relative; z-index: 1 }");
    harness.frame();
    harness.retire_all();

    harness.replace(0, "box { position: relative; z-index: 2 }");
    harness.frame();

    let owed = harness.owed(node);
    assert!(
        owed.contains(Dirty::RESTACK | Dirty::REHIT | Dirty::REPAINT),
        "{owed:?}"
    );
    assert!(!owed.contains(Dirty::RELAYOUT));
}

#[test]
fn a_generated_content_colour_change_repaints_and_rebuilds_the_box() {
    let mut harness = Harness::new();
    let item = harness.append(harness.root, "box");
    harness.add_author("box::before { content: \"x\"; color: rgb(1, 0, 0) }");
    harness.frame();
    let styled = harness
        .document
        .node(item)
        .pseudo_style(&style::selector_parser::PseudoElement::Before);
    assert!(
        styled.is_some(),
        "the fixture has to actually generate content, or the case below tests nothing"
    );
    harness.retire_all();

    harness.replace(0, "box::before { content: \"x\"; color: rgb(2, 0, 0) }");
    harness.frame();

    let owed = harness.owed(item);
    assert!(
        owed.contains(Dirty::REPAINT),
        "generated content has no node of its own, so its identity has to be part of the \
         originating element's key: {owed:?}"
    );
    assert!(
        owed.contains(Dirty::REBUILD_BOX),
        "a generated-content style is cloned into the box that carries it: {owed:?}"
    );
}

#[test]
fn a_pseudo_that_starts_existing_reshapes_and_rebreaks_the_box_that_carries_it() {
    let mut harness = Harness::new();
    let item = harness.append(harness.root, "box");
    harness.append_text(item, "hello");
    harness.add_author("box { width: 10px }\nbox::before { color: rgb(1, 0, 0) }");
    harness.frame();

    // One layout-affecting change first, so that the element already has recorded text keys. A
    // case without it passes on an element the text-key comparison has never seen, which reports a
    // re-shape for any reason at all.
    harness.replace(
        0,
        "box { width: 20px }\nbox::before { color: rgb(1, 0, 0) }",
    );
    harness.frame();
    harness.retire_all();

    harness.replace(
        0,
        "box { width: 20px }\nbox::before { content: \"x\"; color: rgb(1, 0, 0) }",
    );
    let pass = harness.frame();

    let record = pass
        .records
        .iter()
        .find(|record| record.index == item)
        .expect("the element restyled");
    assert_eq!(
        record.damage,
        RestyleDamage::reconstruct(),
        "a pseudo-element beginning to exist is the one thing that sets every bit of the damage \
         word, and without it this case tests an ordinary relayout"
    );
    let owed = harness.owed(item);
    assert!(
        owed.contains(Dirty::RELAYOUT | Dirty::REBUILD_BOX),
        "{owed:?}"
    );
    assert!(
        owed.contains(Dirty::RESHAPE | Dirty::REBREAK),
        "the widest damage the engine can report has to reach the text obligations unnarrowed: \
         generated content is text this element's own style says nothing about, so no comparison \
         of that style can see it. Narrowed, the content appears unshaped or one frame late: \
         {owed:?}"
    );
}

#[test]
fn a_custom_property_change_repaints_the_element_that_declares_it() {
    let mut harness = Harness::new();
    let node = harness.append(harness.root, "vector");
    harness.add_author("vector { --zgui-fill: rgb(1, 0, 0) }");
    harness.frame();
    harness.retire_all();

    harness.replace(0, "vector { --zgui-fill: rgb(2, 0, 0) }");
    harness.frame();

    assert!(
        harness.owed(node).contains(Dirty::REPAINT),
        "vector paint is resolved out of the custom-property maps, so a theme that changes only a \
         custom property must still produce damage"
    );
}
