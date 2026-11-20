//! A subtree hidden with `display: none` while its animations and transitions run.
//!
//! An application caches a page and hides it by a class on an ancestor. The page's descendants
//! lose their styles when it is hidden, and their animations end with them. A frame that shows the
//! page again styles them from their rules, and their keyframe animations start again.

use zgui_dom::NodeIndex;
use zgui_style::Lifecycle;

use crate::support::{Harness, animation_frame, width};

/// A host that shows one of two pages, a field with a width transition, and a bar with an
/// infinite width animation. Both move a box, so both take the cascading tier.
const SHEET: &str = "root { display: block }
                     .host { display: block }
                     .page { display: block }
                     .host.second .first, .host.first .second { display: none }
                     .field { display: block; width: 100px; transition: width 400ms linear }
                     .field.wide { width: 300px }
                     .bar { display: block; width: 10px; animation: grow 400ms linear infinite }
                     @keyframes grow { from { width: 10px } to { width: 50px } }";

/// How long one frame is, in seconds.
const FRAME: f64 = 0.05;

/// The parts of the document the cases read.
struct Pages {
    /// The element whose class picks the shown page.
    host: NodeIndex,
    /// The page the cases hide and show.
    first: NodeIndex,
    /// The element between the first page and the field.
    wrap: NodeIndex,
    /// The field under the first page, with its transition running.
    field: NodeIndex,
    /// The bar under the first page, with its animation running.
    bar: NodeIndex,
}

/// root > host > (first > (wrap > field, bar), second), with the first page shown and the field's
/// transition and the bar's animation running.
fn document() -> (Harness, Pages) {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let root = harness.root;
    let host = harness.append(root, "box");
    harness.set_classes(host, &["host", "first"]);
    let first = harness.append(host, "box");
    harness.set_classes(first, &["page", "first"]);
    let wrap = harness.append(first, "box");
    let field = harness.append(wrap, "box");
    harness.set_classes(field, &["field"]);
    let bar = harness.append(first, "box");
    harness.set_classes(bar, &["bar"]);
    let second = harness.append(host, "box");
    harness.set_classes(second, &["page", "second"]);
    harness.frame();
    harness.retire_all();
    harness.set_classes(field, &["field", "wide"]);
    animation_frame(&mut harness, 0.0);
    animation_frame(&mut harness, FRAME);
    (
        harness,
        Pages {
            host,
            first,
            wrap,
            field,
            bar,
        },
    )
}

/// Shows the page `class` names.
fn show(harness: &mut Harness, pages: &Pages, class: &str) {
    harness.set_classes(pages.host, &["host", class]);
}

#[test]
fn a_page_hidden_and_shown_while_its_animations_run_restyles_its_subtree() {
    let (mut harness, pages) = document();
    let mut now = FRAME;
    for _ in 0..3 {
        show(&mut harness, &pages, "second");
        for _ in 0..3 {
            now += FRAME;
            animation_frame(&mut harness, now);
        }
        assert!(
            harness.document.node(pages.field).primary_style().is_none(),
            "a hidden page holds no styles below it"
        );
        show(&mut harness, &pages, "first");
        now += FRAME;
        animation_frame(&mut harness, now);
        assert!(harness.document.node(pages.first).primary_style().is_some());
        assert_eq!(
            width(&harness, pages.field),
            300.0,
            "the field is styled from its rules, without a transition"
        );
        let bar = width(&harness, pages.bar);
        assert!(
            (10.0..50.0).contains(&bar),
            "the bar's animation runs on the shown page: {bar}"
        );
        now += FRAME;
        animation_frame(&mut harness, now);
    }
}

#[test]
fn a_page_hidden_in_the_frame_its_animations_ask_for_a_cascade_is_shown_again() {
    let (mut harness, pages) = document();
    let mut now = FRAME;
    for _ in 0..3 {
        // The tick marks the bar, and the restyle that services the mark hides the page.
        show(&mut harness, &pages, "second");
        now += FRAME;
        animation_frame(&mut harness, now);
        // Shown again in the next frame, whose tick finds the bar's animation cancelled.
        show(&mut harness, &pages, "first");
        now += FRAME;
        animation_frame(&mut harness, now);
        let bar = width(&harness, pages.bar);
        assert!(
            (10.0..50.0).contains(&bar),
            "the bar's animation runs on the shown page: {bar}"
        );
    }
}

#[test]
fn hiding_a_page_cancels_the_animations_below_it() {
    let (mut harness, pages) = document();
    show(&mut harness, &pages, "second");
    animation_frame(&mut harness, 2.0 * FRAME);
    let report = harness
        .engine
        .animation_tick(&harness.document, zgui_style::AnimationTime(3.0 * FRAME));
    let field = harness.document.store().key_of(pages.field);
    let bar = harness.document.store().key_of(pages.bar);
    let cancelled = |key| {
        report
            .edges
            .iter()
            .any(|edge| edge.node == key && edge.lifecycle == Lifecycle::Cancelled)
    };
    assert!(cancelled(field), "the field's transition is cancelled");
    assert!(cancelled(bar), "the bar's animation is cancelled");
    assert!(
        report
            .elements
            .iter()
            .all(|element| element.index != pages.field && element.index != pages.bar),
        "nothing below the hidden page animates"
    );
}

#[test]
fn a_marked_field_put_back_under_a_hidden_page_is_styled_when_the_page_is_shown() {
    let (mut harness, pages) = document();
    // The tick marks the field, and the field leaves the document before the restyle.
    let report = harness
        .engine
        .animation_tick(&harness.document, zgui_style::AnimationTime(2.0 * FRAME));
    assert!(
        report
            .elements
            .iter()
            .any(|element| element.index == pages.field)
    );
    harness
        .engine
        .mark_animation_restyle(&harness.document, pages.field);
    harness.edit(|edit| edit.remove(pages.field));
    harness.frame();
    harness.retire_all();
    assert_eq!(harness.engine.animation_waiting(), 1);

    // The page is hidden, and the field goes back under it while it is hidden.
    show(&mut harness, &pages, "second");
    harness.frame();
    harness.retire_all();
    harness.edit(|edit| edit.insert_before(pages.wrap, pages.field, None));
    harness.frame();
    harness.retire_all();
    assert_eq!(
        harness.engine.animation_waiting(),
        0,
        "a field under a hidden page waits for nothing"
    );

    show(&mut harness, &pages, "first");
    harness.frame();
    harness.retire_all();
    let field = width(&harness, pages.field);
    assert!(
        (100.0..=300.0).contains(&field),
        "the field is styled on the shown page: {field}"
    );
}
