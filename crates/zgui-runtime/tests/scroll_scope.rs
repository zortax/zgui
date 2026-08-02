//! What a wheel notch is allowed to rebuild while something is watching the page move.
//!
//! # The defect this is written against
//!
//! An observed border box is reported in the window's own pixels, so *anything scrolling* moves it.
//! A surface anchored to something inside a scroller therefore re-places itself on every frame of
//! every scroll, which is correct and cheap — a new `left` and a new `top` — unless the damage that
//! change carries says the boxes below it have to be made again. It did: any layout-affecting
//! change answered with the whole damage set, the construct-descendants bit in it propagated to the
//! document root, and the stage that asked *the root* whether anything owed a rebuild replaced
//! every box in the document. One wheel notch, on a real page, cost two thirds of a second.
//!
//! Nothing about that is visible in a damage rectangle: the damage was right the whole time. So it
//! is asserted here on the counters, against controls taken from runs in which the same counters
//! moved, and paired with the two things a cheap frame could satisfy by doing nothing at all — the
//! page has to have scrolled, and the anchored surface has to have followed it.
//!
//! It is its own test target because the counters are one process-wide block.

mod support;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::Counter;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_testkit_scene::counters::Recording;
use zgui_view::{AnyView, BuildCx, IntoView, NodeRef, ShowProps, View};
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A scroll container with rows in it, and a surface floating over the page.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.page { display: block; position: relative }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.anchor { display: block; width: 40px; height: 20px }
.surface { display: block; position: absolute; width: 60px; height: 30px;
           background-color: #404040 }
.leaf { display: block; width: 10px; height: 10px }
";

/// How many rows the scroller holds, which is many more than fit in it.
const ROWS: usize = 60;

/// A window holding a scroller, an anchor inside it, and a surface placed against that anchor.
///
/// The surface is the shape of every floating surface there is: it watches the anchor's box, it
/// writes its own `left` and `top` from what it is told, and it holds children of its own so that a
/// rebuild of it would be visible as more than one box. `extra` mounts a further child inside it,
/// which is the local structural change the second case exercises.
fn page(extra: RwSignal<bool>) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let anchor = NodeRef::new();
        let watched = anchor.observe_border_box();
        let mut port = zgui_elements::column().class("port");
        for index in 0..ROWS {
            let mut row = zgui_elements::column().class("row");
            if index == 4 {
                row = row.child(zgui_elements::column().class("anchor").node_ref(anchor));
            }
            port = port.child(row);
        }
        let surface = zgui_elements::column()
            .class("surface")
            .style_property("left", move || {
                watched.get().map(|box_| format!("{}px", box_.origin.x.0))
            })
            .style_property("top", move || {
                watched.get().map(|box_| format!("{}px", box_.origin.y.0))
            })
            .child(zgui_elements::column().class("leaf"))
            .child(
                ShowProps::builder()
                    .when(move || extra.get())
                    .children(|| AnyView::new(zgui_elements::column().class("leaf")))
                    .build()
                    .render(),
            );
        Box::new(
            zgui_elements::column()
                .class("page")
                .child(port)
                .child(surface)
                .into_view()
                .build(cx),
        )
    })
}

/// Turns the wheel over the scroller and lets the whole glide arrive.
fn wheel(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, lines: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..24 {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(4);
}

/// The top edge of the first fragment of the element carrying `class`, in device pixels.
///
/// # Panics
///
/// Panics when nothing in the document carries the class, because every caller is about to assert
/// something about where it is.
fn top_of(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>, class: &str) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    let mut found: Option<f32> = None;
    for key in layout.keys() {
        let Some(source) = layout.node(key).source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| held.0.as_ref() == class)
        {
            continue;
        }
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            found = Some(found.map_or(fragment.border_box.origin.y.0, |held: f32| {
                held.min(fragment.border_box.origin.y.0)
            }));
        }
    }
    found.unwrap_or_else(|| panic!("nothing in the document carries `{class}`"))
}

/// How many boxes the document holds altogether.
fn box_count(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> u64 {
    let window = harness.app().windows().first().expect("a window");
    u64::from(window.layout().borrow().len())
}

#[test]
fn a_wheel_notch_under_a_watching_surface_rebuilds_no_box_at_all() {
    let mut recording = Recording::begin();
    let extra = RwSignal::new(false);
    let mut harness = page(extra);

    // The control: opening the window builds every box there is, which is what makes the zero below
    // mean "this scroll did not" rather than "nothing here ever counts a box".
    let built = recording.measure(|| {
        harness.settle(16);
    });
    let boxes = built.control(Counter::BoxesRebuilt);

    let page_before = top_of(&harness, "row");
    let surface_before = top_of(&harness, "surface");
    let scrolled = recording.measure(|| {
        wheel(&mut harness, 3.0);
    });
    let page_after = top_of(&harness, "row");
    let surface_after = top_of(&harness, "surface");

    // Two positive controls. A scroll that moved nothing satisfies the budget, and so does one
    // whose watcher was never told — and the second is exactly what "stop watching" gets wrong.
    assert!(
        page_after < page_before - 1.0,
        "the page did not scroll at all: {page_before} to {page_after}"
    );
    assert!(
        (surface_after - surface_before).abs() > 1.0,
        "the anchor moved and the surface watching it stayed at {surface_before}, so nothing \
         re-placed itself and the budget below is measuring an idle frame"
    );

    scrolled.assert_zero(Counter::BoxesRebuilt, &boxes);
}

#[test]
fn a_local_structural_change_rebuilds_its_own_subtree_and_not_the_page() {
    let mut recording = Recording::begin();
    let extra = RwSignal::new(false);
    let mut harness = page(extra);
    harness.settle(16);

    let total = box_count(&harness);
    assert!(
        total > ROWS as u64,
        "the fixture holds {total} boxes, which is too few for the proportion below to mean \
         anything"
    );

    // One element mounted inside the surface. What that owes is the surface's own boxes; what it
    // used to cost was every box on the page.
    let mounted = recording.measure(|| {
        extra.set(true);
        harness.settle(8);
    });

    let rebuilt = mounted.get(Counter::BoxesRebuilt);
    assert!(
        rebuilt > 0,
        "nothing was built at all, so the child never appeared and this measures an idle frame"
    );
    assert!(
        rebuilt <= 8,
        "mounting one element into a surface holding two rebuilt {rebuilt} boxes of {total}, so \
         the change was serviced by rebuilding the document rather than the subtree"
    );
    assert!(
        box_count(&harness) > total,
        "the document did not gain the box the change was about"
    );
}
