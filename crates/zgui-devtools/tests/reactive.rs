//! What the reactivity tab counts.
//!
//! The tab is about scopes rather than about the dependency graph, because scopes are what
//! `reactive_graph` lets a tool outside it ask about. The number worth asserting is therefore the
//! one that catches the bug: instances alive against instances ever built.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{frames_over, opened, run, text};

/// The tab counts the components that are mounted, and says where each was written.
#[test]
fn the_tab_counts_what_is_mounted() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Reactivity);
    run(&mut harness, 16);

    let shown = text(&harness);
    assert!(
        shown.contains("instances alive"),
        "the tab does not report what is alive: {shown:.600}"
    );
    // The page the harness mounts is a component, so it is one of them.
    assert!(
        shown.contains("Page"),
        "the mounted component is not counted: {shown:.600}"
    );
    assert!(
        shown.contains("mod.rs:"),
        "the counted component does not say where it was written: {shown:.600}"
    );
    // And the tab is honest about the half it cannot reach.
    assert!(
        shown.contains("what this cannot show"),
        "the tab does not say what it cannot answer: {shown:.600}"
    );
}

/// Counting what is alive does not keep the window awake.
#[test]
fn the_tab_leaves_a_still_document_idle() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Reactivity);
    run(&mut harness, 120);

    let frames = frames_over(&mut harness, 300);
    assert_eq!(
        frames, 0,
        "the reactivity tab woke the window {frames} times over 300 vsyncs"
    );
}
