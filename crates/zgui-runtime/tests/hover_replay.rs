//! A row whose background changed is never replayed from the painting it had before.
//!
//! A paint-only change moves nothing: the fragment keeps its rectangle and its content, and the
//! one thing that tells the painter to encode it again rather than replay last frame's recording
//! is the lowered style the fragment resolves to. That lowering is cached by the identity of the
//! cascade's property groups, which are addresses — and an address is only an identity while the
//! allocation behind it is alive. A row hovered, left, and hovered again allocates and frees its
//! background group every time, so the allocator hands the same block back; a cache that kept
//! only the numbers then answers the hovered style with the un-hovered lowering, and the row is
//! replayed without its highlight, with correct damage and nothing to report it.
//!
//! Two hundred hovers, because the reuse is the allocator's decision and not the frame's.

mod support;

use zgui_interned::ClassName;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, View};

const ROWS: usize = 20;

const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .row { display: block; height: 10px; background-color: rgb(200, 200, 200) }
                   .row.hov { background-color: rgb(10, 10, 10) }";

/// The red channel of every solid quad the window shows.
fn drawn_reds(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    support::drawing(&harness.app().windows()[0])
        .quads()
        .iter()
        .filter_map(|quad| quad.color("fill"))
        .map(|[red, ..]| (red * 255.0).round() as u8)
        .collect()
}

#[test]
fn a_hovered_row_is_always_painted_with_its_highlight() {
    let hover: RwSignal<Option<usize>> = RwSignal::new(Some(0));
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let mut rows = zgui_elements::column();
        for index in 0..ROWS {
            rows = rows.child(
                zgui_elements::column()
                    .class("row")
                    .class_toggle(ClassName::new("hov"), move || hover.get() == Some(index)),
            );
        }
        Box::new(rows.into_view().build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    for step in 1..=200 {
        // Far apart and close by in turn, so the rows freed and reallocated between two hovers
        // of the same row vary in number.
        let next = (step * 7) % ROWS;
        hover.set(Some(next));
        let frames = harness.settle(8);
        assert_eq!(frames, 1, "step {step}: one hover is one frame");
        let reds = drawn_reds(&harness);
        assert_eq!(
            reds.iter().filter(|red| **red == 10).count(),
            1,
            "step {step}: the row hovered was painted without its highlight: {reds:?}"
        );
    }
    harness.shut_down();
}
