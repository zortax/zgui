//! What a reader's position does when the window changes size or output under them.
//!
//! A scroll offset is clamped where it is written, and the numbers it is clamped against belong to
//! layout: the scrollport's extent and the content's. Both move without anything having scrolled —
//! a window resized, a document reflowed, a surface dragged onto a monitor with a different device
//! pixel ratio — so an offset that was legal when it was written can stop being legal without ever
//! being touched again.
//!
//! The failure is total rather than subtle, which is why it is asserted here rather than left to a
//! visual check. The fragment pass composes every descendant of a container against its offset, so
//! a container scrolled a screenful past the end of its own content has that whole subtree
//! translated clear of its scrollport: nothing intersects the clip, nothing is emitted, and the
//! window is blank while every counter in the frame reports a document laid out correctly.
//!
//! The other half is the one that must **not** happen. A window that grew taller, or that changed
//! only its width, has not moved the reader — and a resize that helpfully re-derived the offset as
//! a fraction of the new extent would move them on every drag of a window edge.

mod support;

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A page taller than any window this test gives it, inside the document's own scroll container.
const CSS: &str = "
root { display: block; width: 400px; overflow: scroll }
.row { display: block; width: 400px; height: 40px; background-color: #202020 }
";

/// The application under test.
type Window = zgui_platform_headless::Harness<zgui_runtime::Runtime>;

/// A window holding a hundred rows in a scrolling root.
fn listing() -> Window {
    support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut page = zgui_elements::column().class("root");
        for _ in 0..100 {
            page = page.child(zgui_elements::column().class("row"));
        }
        Box::new(page.into_view().build(cx))
    })
}

/// The window the harness is driving.
fn window(harness: &Window) -> &zgui_runtime::Window {
    harness.app().windows().first().expect("a window")
}

/// The scrolled container and its offset, its limit, and where the fragments compose against.
///
/// Read off the live window rather than remembered, because every one of the three is a different
/// answer and the defect this file guards moves them apart.
fn scrolled_container(harness: &Window) -> (f32, f32, f32) {
    let window = window(harness);
    let layout = window.layout().borrow();
    let scroll = window.scroll().borrow();
    for key in layout.keys() {
        let Some(region) = zgui_layout::scroll_region::region_of(&layout, key) else {
            continue;
        };
        let Some(element) = layout.node(key).source else {
            continue;
        };
        if region.limit().y.0 <= 0.0 {
            continue;
        }
        return (
            scroll.offset_of(element).y.0,
            region.limit().y.0,
            scroll.composed().of(element).y.0,
        );
    }
    panic!("the document has no container whose content overflows it");
}

/// Sends the window to `width` by `height` device pixels and lets the frames it asks for run.
fn size_to(harness: &mut Window, width: f32, height: f32) {
    harness.deliver_to_first(SurfaceEvent::Resized(Size::<DevicePx, Device>::new(
        DevicePx(width),
        DevicePx(height),
    )));
    harness.settle(32);
    harness.advance(std::time::Duration::from_millis(20));
    harness.settle(32);
}

/// Moves the surface to `scale` while holding its extent in CSS pixels fixed.
fn scale_to(harness: &mut Window, scale: f32, css_width: f32, css_height: f32) {
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: f64::from(scale),
        size: Size::new(DevicePx(css_width * scale), DevicePx(css_height * scale)),
    });
    harness.settle(32);
    harness.advance(std::time::Duration::from_millis(20));
    harness.settle(32);
}

/// A window scrolled to the very bottom of its content in a short viewport.
fn at_the_bottom() -> Window {
    let mut harness = listing();
    size_to(&mut harness, 400.0, 200.0);
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(100.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: 400.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..90 {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(8);
    let (at, limit, _) = scrolled_container(&harness);
    assert!(
        at > 0.0 && (at - limit).abs() < 1.0,
        "the wheel left the container at {at} of {limit}, which is not the end of the document"
    );
    harness
}

#[test]
fn a_window_that_grows_taller_under_a_reader_at_the_end_clamps_to_the_new_end() {
    // The whole document off the top of the window. The content did not move and there is now more
    // room than it fills, so the position that *was* the end is past it — and the offset is
    // clamped against an extent that is only recomputed by the layout pass this resize runs.
    let mut harness = at_the_bottom();
    size_to(&mut harness, 400.0, 1000.0);

    let (at, limit, composed) = scrolled_container(&harness);
    assert!(
        at <= limit + 0.5,
        "the container is at {at} with a limit of {limit}, so its subtree is composed {} pixels \
         above its own scrollport",
        at - limit
    );
    assert!(
        composed <= limit + 0.5,
        "the fragments are composed against {composed} with a limit of {limit}"
    );
}

#[test]
fn a_window_that_grows_taller_under_a_reader_in_the_middle_does_not_move_them() {
    // The other obligation, and it pulls the other way. The offset is still one the content allows,
    // so the line being read stays exactly where it is and only the amount of document around it
    // changes. Nothing here is a fraction of anything.
    let mut harness = at_the_bottom();
    size_to(&mut harness, 400.0, 200.0);
    let window = window(&harness);
    let element = {
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .find(|key| {
                zgui_layout::scroll_region::region_of(&layout, *key)
                    .is_some_and(|region| region.limit().y.0 > 0.0)
            })
            .and_then(|key| layout.node(key).source)
            .expect("a scrolled container")
    };
    // Well inside the document, so that growing the window cannot make this offset illegal.
    window
        .scroll()
        .borrow_mut()
        .scroll_to(
            &window.layout().borrow(),
            element,
            Point::new(DevicePx(0.0), DevicePx(600.0)),
            zgui_scroll::Behavior::Instant,
        )
        .expect("the container scrolls");
    harness.settle(8);
    let (before, _, _) = scrolled_container(&harness);

    size_to(&mut harness, 400.0, 1000.0);
    let (after, _, _) = scrolled_container(&harness);
    assert_eq!(
        after, before,
        "a window that only gained room beneath the reader moved them from {before} to {after}"
    );
}

#[test]
fn a_window_that_changes_only_its_width_does_not_move_the_reader() {
    let mut harness = at_the_bottom();
    size_to(&mut harness, 400.0, 200.0);
    let (before, _, _) = scrolled_container(&harness);

    size_to(&mut harness, 360.0, 200.0);
    let (after, _, _) = scrolled_container(&harness);
    assert_eq!(
        after, before,
        "a width-only resize moved the reader from {before} to {after}"
    );
}

#[test]
fn a_surface_that_doubles_its_ratio_keeps_the_reader_at_the_same_place_in_the_document() {
    // An offset is a number of *device* pixels. Carried across a change of ratio unchanged it
    // stands for a different place: a reader nine tenths of the way down a page arrives just under
    // halfway. Asserted as a fraction of the limit, which is the one thing both ratios agree on.
    let mut harness = listing();
    size_to(&mut harness, 400.0, 200.0);
    let window = window(&harness);
    let element = {
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .find(|key| {
                zgui_layout::scroll_region::region_of(&layout, *key)
                    .is_some_and(|region| region.limit().y.0 > 0.0)
            })
            .and_then(|key| layout.node(key).source)
            .expect("a scrolled container")
    };
    window
        .scroll()
        .borrow_mut()
        .scroll_to(
            &window.layout().borrow(),
            element,
            Point::new(DevicePx(0.0), DevicePx(2_000.0)),
            zgui_scroll::Behavior::Instant,
        )
        .expect("the container scrolls");
    harness.settle(8);
    let (before, limit_before, _) = scrolled_container(&harness);
    let where_they_were = before / limit_before;

    scale_to(&mut harness, 2.0, 400.0, 200.0);
    let (after, limit_after, _) = scrolled_container(&harness);
    assert!(
        limit_after > limit_before * 1.5,
        "the document was not re-measured at the new ratio: {limit_before} then {limit_after}"
    );
    let where_they_are = after / limit_after;
    assert!(
        (where_they_are - where_they_were).abs() < 0.02,
        "the reader was {:.1}% down the document and is now {:.1}% down it",
        where_they_were * 100.0,
        where_they_are * 100.0
    );
}

#[test]
fn a_surface_that_halves_its_ratio_leaves_the_reader_inside_the_document() {
    // The other direction, where the defect is the same one the resize clamp exists for: at the
    // bottom of a document on a two-times output, the number is one the one-times surface has no
    // content for at all.
    let mut harness = listing();
    size_to(&mut harness, 800.0, 400.0);
    scale_to(&mut harness, 2.0, 400.0, 200.0);
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(100.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: 400.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..90 {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(8);
    let (at, limit, _) = scrolled_container(&harness);
    assert!(
        (at - limit).abs() < 1.0,
        "the wheel left the container at {at} of {limit} at the higher ratio"
    );

    scale_to(&mut harness, 1.0, 400.0, 200.0);
    let (after, limit_after, composed) = scrolled_container(&harness);
    assert!(
        after <= limit_after + 0.5,
        "back at one times the container is at {after} with a limit of {limit_after}"
    );
    assert!(
        composed <= limit_after + 0.5,
        "the fragments are composed against {composed} with a limit of {limit_after}"
    );
    assert!(
        after > limit_after * 0.5,
        "the reader was at the end of the document and is now {after} of {limit_after}"
    );
}
