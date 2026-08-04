//! A still document with the panel open draws nothing, on every tab.
//!
//! The property the whole design rests on, and the one the panel is most able to break. Two tabs
//! show numbers that move every frame *because the panel is showing them*: a counter delta includes
//! the panel's own re-render, and a stage duration is the time that re-render took. So the naive
//! discipline — compare before writing — cannot converge on those two, and a window with either of
//! them open asks for a frame every refresh interval for as long as it is open, on a page nobody is
//! touching. The tree tab is the same hazard by another route: its rows are elements of the very
//! document it samples, so a tree that included them would grow every time it was drawn.
//!
//! Three hundred vsyncs is five seconds at 60 Hz. A window that wakes even once in that time on a
//! document that has not changed is a window whose battery cost is the inspector's.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{frames_over, opened, run};

/// How long the panel is given to settle before the count starts.
///
/// Longer than the publication cadence, so that a tab which publishes on a cadence has published
/// at least once and gone quiet again — a window still counting down to its first publication
/// would look idle for the wrong reason.
const SETTLE: usize = 120;

/// No tab keeps the window awake on a document that is not moving.
#[test]
fn no_tab_draws_a_frame_on_a_still_document() {
    for tab in Tab::ALL {
        let tools = DevTools::new();
        let mut harness = opened(tools);
        tools.set_open(true);
        tools.show(tab);
        run(&mut harness, SETTLE);

        let frames = frames_over(&mut harness, 300);
        assert_eq!(
            frames,
            0,
            "the {} tab woke the window {frames} times over 300 vsyncs on a still document",
            tab.label()
        );
    }
}
