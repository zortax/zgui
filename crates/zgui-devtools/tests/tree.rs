//! What the tree tab shows, and what it must not show.
//!
//! Two shapes of one document: the components somebody wrote, and every node those components
//! produced. The assertions worth having are about the boundaries between them — that a component
//! is named and located, that the nodes it made are under it, and above all that the panel's own
//! rows are in neither, because the tree's rows are elements of the document the tree is read from.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab, TreeMode};

use support::{boxes, frames_over, opened, run, text};

/// How long the panel is given to settle before anything is counted.
const SETTLE: usize = 120;

/// An open inspector showing the tree tab in `mode`.
fn showing(mode: TreeMode) -> (DevTools, zgui_platform_headless::Harness<zgui::runtime::Runtime>) {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Elements);
    tools.set_tree_mode(mode);
    run(&mut harness, 16);
    (tools, harness)
}

/// The components tab names the components the test page is built from, and says where they live.
#[test]
fn the_components_mode_names_each_component_and_where_it_was_written() {
    let (_tools, harness) = showing(TreeMode::Components);
    let shown = text(&harness);

    assert!(
        shown.contains("Page"),
        "the page's own component is not in the tree: {shown}"
    );
    // The declaration site, which is the answer a component tree is worth having for: the support
    // module is where `Page` is written.
    assert!(
        shown.contains("mod.rs:"),
        "no component says which file and line it came from: {shown}"
    );
}

/// The full mode shows the nodes, and still says which component made them.
#[test]
fn the_full_mode_shows_the_nodes_with_the_boundaries_still_among_them() {
    let (_tools, harness) = showing(TreeMode::Full);
    let shown = text(&harness);

    assert!(
        shown.contains(".page"),
        "the page's own element is not in the full tree: {shown}"
    );
    assert!(
        shown.contains(".target"),
        "the target box is not in the full tree: {shown}"
    );
    assert!(
        shown.contains("Page"),
        "the full tree dropped the component boundaries: {shown}"
    );
}

/// The tree does not list the panel that lists it.
///
/// The runaway this is bounded against. The tree's rows are elements, elements are nodes, and nodes
/// are what the tree is read from — so a tree that included the panel would grow by its own size
/// every time it was drawn and never converge.
#[test]
fn the_tree_does_not_list_the_rows_that_list_it() {
    for mode in [TreeMode::Components, TreeMode::Full] {
        let (_tools, harness) = showing(mode);
        let shown = text(&harness);
        assert!(
            !shown.contains("zgui-devtools__tree-row"),
            "the {} tree lists its own rows: {shown}",
            mode.label()
        );
        assert!(
            !shown.contains("zgui-devtools__bar"),
            "the {} tree lists the panel's own chrome: {shown}",
            mode.label()
        );
    }
}

/// Drawing the tree does not grow the document it is a tree of.
#[test]
fn drawing_the_tree_does_not_feed_it() {
    for mode in [TreeMode::Components, TreeMode::Full] {
        let (_tools, mut harness) = showing(mode);
        run(&mut harness, SETTLE);
        let before = boxes(&harness);

        let frames = frames_over(&mut harness, 300);
        let after = boxes(&harness);

        assert_eq!(
            before,
            after,
            "the {} tree grew the document from {before} boxes to {after} while drawing itself",
            mode.label()
        );
        assert_eq!(
            frames,
            0,
            "the {} tree woke the window {frames} times on a still document",
            mode.label()
        );
    }
}

/// A component folds away what it built, in either tree.
///
/// A component's content are the siblings between its markers rather than children of one, so a
/// tree that asked the document what is under a component row was told "nothing" and drew no
/// chevron — which left every component in the full tree permanently unfoldable, and the primitive
/// nodes beside them folding perfectly.
#[test]
fn a_component_folds_away_what_it_built_in_either_tree() {
    for mode in [TreeMode::Components, TreeMode::Full] {
        let (tools, mut harness) = showing(mode);
        let page = support::component_named(&harness, "Page")
            .unwrap_or_else(|| panic!("the {} tree has a row for `Page`", mode.label()));

        assert!(
            text(&harness).contains("Page"),
            "the {} tree does not name the component to begin with",
            mode.label()
        );
        tools.set_expanded(page, false);
        run(&mut harness, 16);

        let folded = text(&harness);
        assert!(
            folded.contains("Page"),
            "folding the {} component away took its own row with it",
            mode.label()
        );
        assert!(
            !folded.contains(".target"),
            "folding the `Page` component in the {} tree left what it built on screen: {folded}",
            mode.label()
        );
    }
}

/// Picking in the application opens the path to the row for it.
#[test]
fn picking_opens_the_path_to_what_was_picked() {
    let (tools, mut harness) = showing(TreeMode::Full);

    // Whatever the target box is, picked from outside the tree.
    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    tools.pick(target);
    run(&mut harness, 16);

    assert_eq!(
        tools.picked(),
        Some(target),
        "picking a node did not record it"
    );
    assert!(
        text(&harness).contains(".target"),
        "the picked node has no row in the tree"
    );
}

/// Selecting opens the detail pane under the tree, and nothing selected leaves it shut.
#[test]
fn selecting_opens_the_detail_pane_under_the_tree() {
    let (tools, mut harness) = showing(TreeMode::Full);
    assert!(
        !text(&harness).contains("box model"),
        "the detail pane is open with nothing picked"
    );

    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    tools.pick(target);
    run(&mut harness, 16);

    let shown = text(&harness);
    assert!(
        shown.contains("box model"),
        "picking left the detail pane shut: {shown}"
    );
    assert!(
        shown.contains("Components") && shown.contains("All nodes"),
        "the detail pane replaced the tree instead of splitting with it: {shown}"
    );
}

/// Picking a box the components tree has no row for selects the component that built it.
///
/// The components view has a row per boundary and none per element, so a pointer pick would
/// otherwise select something invisible — which reads as the click having missed.
#[test]
fn picking_a_box_selects_the_component_that_built_it() {
    let (tools, mut harness) = showing(TreeMode::Components);
    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    let page = support::component_named(&harness, "Page").expect("`Page` is a component");

    tools.pick(target);
    run(&mut harness, 16);

    assert_eq!(
        tools.picked_component(),
        Some(page),
        "picking a box inside `Page` did not resolve to `Page`"
    );
    // And the detail pane still describes the box that was picked, not the component.
    assert!(
        text(&harness).contains("box model"),
        "the detail pane did not open for the picked box"
    );
}

/// A row folded away is unfolded again by picking something inside it.
///
/// The reason the tab follows the picker at all. Without it, picking in the application while the
/// branch it lives on happens to be closed selects a row that is not on screen — which reads as the
/// pick having done nothing.
#[test]
fn picking_something_inside_a_closed_row_opens_it_again() {
    let (tools, mut harness) = showing(TreeMode::Full);
    let page = support::node_with_class(&harness, "page").expect("the page is in the document");
    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");

    // Fold the page away, which takes the target with it.
    tools.set_expanded(page, false);
    run(&mut harness, 16);
    assert!(
        !text(&harness).contains(".target"),
        "closing the page left what is inside it on screen, so this proves nothing"
    );

    tools.pick(target);
    run(&mut harness, 16);
    assert!(
        text(&harness).contains(".target"),
        "picking a node inside a closed row left it closed"
    );
}

/// The rule between the tree and the detail pane can be dragged.
#[test]
fn dragging_the_rule_resizes_the_detail_pane() {
    let (tools, mut harness) = showing(TreeMode::Full);
    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    tools.pick(target);
    run(&mut harness, 16);

    let before = support::box_of(&harness, "zgui-devtools__split-detail")
        .size
        .height
        .0;
    let scale = harness.app().windows()[0].scale().get();
    let rule = support::centre(support::box_of(&harness, "zgui-devtools__split-rule"), scale);

    // Upwards is taller: the detail is below the rule.
    harness.deliver_to_first(support::pressed(rule));
    harness.settle(16);
    harness.deliver_to_first(support::moved(zgui::geom::Point::new(
        rule.x,
        zgui::geom::CssPx(rule.y.0 - 60.0),
    )));
    harness.settle(16);
    harness.deliver_to_first(support::released(zgui::geom::Point::new(
        rule.x,
        zgui::geom::CssPx(rule.y.0 - 60.0),
    )));
    run(&mut harness, 8);

    let after = support::box_of(&harness, "zgui-devtools__split-detail")
        .size
        .height
        .0;
    assert!(
        after > before + 40.0,
        "a 60px drag up took the detail pane from {before}px to {after}px"
    );

    // And the tree kept the rest rather than being pushed out of the tab.
    let tree = support::box_of(&harness, "zgui-devtools__split-tree")
        .size
        .height
        .0;
    assert!(tree > 0.0, "the tree was squeezed to nothing");
}

/// Neither half of the tab scrolls the other.
///
/// The tab used to be a scroller wrapping a scroller: two gutters down the right-hand edge, and a
/// page that kept moving after the inner one had hit its end.
#[test]
fn the_two_halves_scroll_independently() {
    let (tools, mut harness) = showing(TreeMode::Full);
    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    tools.pick(target);
    run(&mut harness, 16);

    let tab = support::box_of(&harness, "zgui-devtools__tabs");
    let tree = support::box_of(&harness, "zgui-devtools__split-tree");
    let detail = support::box_of(&harness, "zgui-devtools__split-detail");

    // Both halves fit inside the tab: nothing overflows it, so nothing scrolls it.
    assert!(
        tree.size.height.0 + detail.size.height.0 <= tab.size.height.0 + 8.0,
        "the two halves are {}px and {}px inside a {}px tab, so the tab itself scrolls",
        tree.size.height.0,
        detail.size.height.0,
        tab.size.height.0
    );
}

/// A tree with more in it than fits scrolls rather than squashing its rows.
///
/// A scrolling column is still a flex column, and a flex item's default is to shrink rather than
/// overflow — so the tree squeezed its own rows until the text was unreadable instead of scrolling
/// past them, and squeezed them further every time the detail pane grew.
#[test]
fn a_tree_too_tall_for_its_pane_scrolls_rather_than_squashing() {
    let tools = DevTools::new();
    // Far more rows than a 500px window can hold.
    let mut harness = support::sized(
        tools,
        zgui::geom::Size::new(zgui::geom::DevicePx(1000.0), zgui::geom::DevicePx(500.0)),
        40,
    );
    tools.set_open(true);
    tools.show(Tab::Elements);
    tools.set_tree_mode(TreeMode::Full);
    run(&mut harness, 16);

    let target =
        support::node_with_class(&harness, "target").expect("the target box is in the document");
    tools.pick(target);
    run(&mut harness, 16);

    let pane = support::box_of(&harness, "zgui-devtools__split-tree")
        .size
        .height
        .0;
    let row = support::box_of(&harness, "zgui-devtools__tree-row")
        .size
        .height
        .0;
    assert!(
        row >= 10.0,
        "the tree squashed its rows to {row}px inside a {pane}px pane rather than scrolling"
    );
}
