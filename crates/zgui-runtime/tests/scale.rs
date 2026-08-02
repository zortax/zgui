//! That a document fills the surface it is drawn into, at every device pixel ratio.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A root that is told to be exactly as large as whatever contains it.
const CSS: &str = "root { display: block; width: 100%; height: 100% }";

/// The extent of the root box's fragment, in device pixels.
fn root_extent(window: &zgui_runtime::Window) -> Size<DevicePx, Device> {
    let layout = window.layout().borrow();
    let root = layout.root().expect("the document has a root box");
    let fragment = *layout
        .fragments_of_box(root)
        .first()
        .expect("the root box produced a fragment");
    layout
        .fragment(fragment)
        .expect("the fragment is live")
        .border_box
        .size
}

/// Lays the fixture out into a surface of `size` device pixels at `scale`, and reports the root.
fn root_at(scale: f64, size: Size<DevicePx, Device>) -> Size<DevicePx, Device> {
    let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().class("root").into_view().build(cx))
    });
    // Delivering the event moves the surface as well, exactly as a window system does: what the
    // surface reports has already changed by the time the notification arrives.
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: scale,
        size,
    });
    harness.settle(16);
    let extent = root_extent(&harness.app().windows()[0]);
    harness.shut_down();
    extent
}

#[test]
fn the_document_fills_the_surface_at_a_fractional_scale() {
    // The defect: the layout pass is measured in device pixels — every absolute length in a style
    // has been multiplied by the scale before the tree sees it — and it was handed the surface's
    // extent in CSS pixels. At a scale of one the two are the same number, which is why every test
    // that ran at one passed, and why the black band down the right of a fractionally scaled
    // window was thirty per cent of the surface: one minus one over 1.2 squared.
    let surface = Size::new(DevicePx(1080.0), DevicePx(720.0));
    let extent = root_at(1.2, surface);
    assert_eq!(
        extent, surface,
        "a root of 100% by 100% covers the whole surface"
    );
}

#[test]
fn the_document_fills_the_surface_at_a_scale_of_one_and_a_quarter_too() {
    let surface = Size::new(DevicePx(1000.0), DevicePx(500.0));
    assert_eq!(root_at(1.25, surface), surface);
}

#[test]
fn the_scale_of_one_case_is_unchanged() {
    // The control: the case every other test in the suite runs, which the fix must not move.
    let surface = Size::new(DevicePx(400.0), DevicePx(300.0));
    assert_eq!(root_at(1.0, surface), surface);
}

/// A document whose only element is a button of a known size in a known place.
const BUTTON_CSS: &str = "root { display: block; width: 100%; height: 100% }
                          control { display: block; width: 40px; height: 20px;
                                    margin: 30px 0 0 50px }";

/// One pointer event at `at`, in CSS pixels, which is what a window system reports.
fn pointer(action: PointerAction, at: Point<CssPx, Css>) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

#[test]
fn a_click_lands_on_what_is_under_it_at_a_fractional_scale() {
    // The other half of the scale question, and the half a screenshot cannot answer. The layout is
    // measured in device pixels and a pointer arrives in CSS ones, so the two are related by the
    // scale in exactly one place — and at a scale of one, which is what every other test in the
    // suite runs at, a conversion that is missing and a conversion that is right look the same.
    //
    // The button is 40 by 20 CSS pixels at 50, 30. Its centre is therefore at 70, 40, whatever the
    // scale is: a pointer position is not a device pixel.
    let clicks = Rc::new(Cell::new(0_u32));
    let counted = Rc::clone(&clicks);
    let mut harness = support::app(BUTTON_CSS, move |cx: &mut BuildCx<'_>| {
        let counted = Rc::clone(&counted);
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control().on(zgui_view::events::CLICK, move |_| {
                        counted.set(counted.get() + 1);
                    }),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 1.25,
        size: Size::new(DevicePx(1000.0), DevicePx(500.0)),
    });
    harness.settle(16);

    let centre = Point::new(CssPx(70.0), CssPx(40.0));
    harness.deliver_to_first(pointer(PointerAction::Moved, centre));
    harness.deliver_to_first(pointer(PointerAction::Pressed, centre));
    harness.deliver_to_first(pointer(PointerAction::Released, centre));
    harness.settle(16);
    assert_eq!(
        clicks.get(),
        1,
        "a click at the button's own centre in CSS pixels reached it"
    );

    // The control, and the half that discriminates. The button covers 50 to 90 CSS pixels across,
    // which is 62.5 to 112.5 device pixels. A pointer at 100 CSS pixels is well clear of it — and
    // lands squarely inside it if whatever is doing the hit test has taken the CSS number for a
    // device one. At a scale of one the two readings coincide, which is why nothing caught it.
    let scaled = Point::new(CssPx(100.0), CssPx(45.0));
    harness.deliver_to_first(pointer(PointerAction::Moved, scaled));
    harness.deliver_to_first(pointer(PointerAction::Pressed, scaled));
    harness.deliver_to_first(pointer(PointerAction::Released, scaled));
    harness.settle(16);
    assert_eq!(
        clicks.get(),
        1,
        "a click outside the button in CSS pixels, and inside it in device pixels, reached it"
    );
    harness.shut_down();
}

/// Every ratio a desktop actually presents a window at: integral, fractional and doubled.
const RATIOS: [f64; 5] = [1.0, 1.2, 1.25, 1.5, 2.0];

/// A surface of `logical` CSS pixels at `scale`, in device pixels, as a window system reports it.
fn surface_at(scale: f64, logical: (f32, f32)) -> Size<DevicePx, Device> {
    Size::new(
        DevicePx((logical.0 * scale as f32).round()),
        DevicePx((logical.1 * scale as f32).round()),
    )
}

#[test]
fn the_last_pixel_of_the_surface_is_covered_by_the_document() {
    // The assertion the earlier one could not make. A root fragment whose *extent* equals the
    // surface says nothing about where that fragment sits, and a document laid out into the right
    // number of pixels in the wrong place leaves the same black band as one laid out into too few.
    // This asks the question a person looking at the window asks: is the bottom-right pixel inside
    // something the document drew?
    for ratio in RATIOS {
        let surface = surface_at(ratio, (900.0, 600.0));
        let mut harness = support::app(CSS, |cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().class("root").into_view().build(cx))
        });
        harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: ratio,
            size: surface,
        });
        harness.settle(16);

        let window = &harness.app().windows()[0];
        let layout = window.layout().borrow();
        let root = layout.root().expect("the document has a root box");
        let fragment = *layout
            .fragments_of_box(root)
            .first()
            .expect("the root box produced a fragment");
        let box_ = layout
            .fragment(fragment)
            .expect("the fragment is live")
            .border_box;
        let corner = (surface.width.0 - 1.0, surface.height.0 - 1.0);
        assert!(
            box_.origin.x.0 <= corner.0
                && box_.origin.y.0 <= corner.1
                && box_.origin.x.0 + box_.size.width.0 > corner.0
                && box_.origin.y.0 + box_.size.height.0 > corner.1,
            "at a ratio of {ratio} the pixel at {corner:?} of a {surface:?} surface is outside \
             everything the document laid out, which on screen is a black band: {box_:?}"
        );
        drop(layout);
        harness.shut_down();
    }
}

/// A document whose one element has a size and a position no ratio may change.
const FIXED_CSS: &str = "root { display: block; width: 100%; height: 100% }
                         .pin { display: block; width: 40px; height: 20px;
                                margin: 30px 0 0 50px }";

/// The pinned element's border box, in device pixels, after the window has moved to `ratio`.
fn pinned_box(ratio: f64) -> zgui_geom::Rect<DevicePx, Device> {
    let mut harness = support::app(FIXED_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::control().class("pin"))
                .into_view()
                .build(cx),
        )
    });
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: ratio,
        size: surface_at(ratio, (900.0, 600.0)),
    });
    harness.settle(16);
    let box_ = class_box(&harness.app().windows()[0], "pin");
    harness.shut_down();
    box_
}

/// The border box of the first fragment of the first box generated by an element in `class`.
///
/// # Panics
///
/// Panics when no element carries the class: a rectangle that was never found is not a rectangle
/// that agreed with anything.
fn class_box(window: &zgui_runtime::Window, class: &str) -> zgui_geom::Rect<DevicePx, Device> {
    let layout = window.layout().borrow();
    let document = window.document().borrow();
    for key in layout.keys() {
        let Some(source) = layout.get(key).and_then(|node| node.source) else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|name| name.as_ref() == class)
        {
            continue;
        }
        let Some(fragment) = layout.fragments_of_box(key).first().copied() else {
            continue;
        };
        if let Some(fragment) = layout.fragment(fragment) {
            return fragment.border_box;
        }
    }
    panic!("no element carrying {class:?} produced a fragment");
}

#[test]
fn a_known_elements_device_rect_scales_with_the_ratio() {
    // Forty by twenty CSS pixels at fifty, thirty, and nothing about it is content-sized — so
    // where it lands is arithmetic rather than a recorded number.
    //
    // The expectation is written as *rounded edges* rather than a rounded size, because that is
    // the rule the framework snaps by: each absolute edge goes to a whole device pixel and a size
    // is the difference between two of them. At a ratio of 1.25 the element spans 62.5 to 112.5
    // device pixels, which is 63 to 113 and therefore fifty pixels wide — not `40 × 1.25`. A test
    // asserting the multiplied size would be asserting that the framework does not snap, which is
    // a different and wrong claim.
    for ratio in RATIOS {
        let box_ = pinned_box(ratio);
        let edge = |css: f64| (css * ratio).round() as f32;
        assert_eq!(
            (
                box_.origin.x.0,
                box_.origin.y.0,
                box_.size.width.0,
                box_.size.height.0
            ),
            (
                edge(50.0),
                edge(30.0),
                edge(90.0) - edge(50.0),
                edge(50.0) - edge(30.0)
            ),
            "at a ratio of {ratio} the element is not between the device pixels its CSS edges \
             land on"
        );
    }
}

/// A document whose one element is sized by the text inside it and by nothing else.
const CONTENT_CSS: &str = "root { display: flex; width: 100%; height: 100% }
                           .measured { padding: 4px 6px }";

/// The measured element's border box after the window has been through `ratios` in order.
fn measured_after(ratios: &[f64]) -> zgui_geom::Rect<DevicePx, Device> {
    let mut harness = support::app_with_text(CONTENT_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::row()
                .class("root")
                .child(zgui_elements::text().class("measured").child("abcdefgh"))
                .into_view()
                .build(cx),
        )
    });
    for ratio in ratios {
        harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: *ratio,
            size: surface_at(*ratio, (400.0, 300.0)),
        });
        harness.settle(16);
    }
    let box_ = class_box(&harness.app().windows()[0], "measured");
    harness.shut_down();
    box_
}

#[test]
fn a_ratio_change_relays_out_what_its_own_content_sized() {
    // The defect a window that never changes ratio cannot have. Everything the layout algorithms
    // are handed is in device pixels, and the per-box cache is keyed by the *question* asked — a
    // run mode, an available space, a known size. A min-content or max-content probe carries no
    // size at all, so the same box asks a byte-identical question at every ratio and is answered
    // from a slot computed at the previous one.
    //
    // The symptom is a document that half rescales: `padding: 4px 6px` is converted afresh and
    // doubles, the text inside it keeps the width it had at the old ratio, and the element ends up
    // one-times text inside two-times padding. Everything with an explicit size looks perfect,
    // which is why an assertion on a `width: 40px` box cannot see it.
    for ratio in RATIOS {
        let fresh = measured_after(&[ratio]);
        for previous in RATIOS.iter().filter(|other| **other != ratio) {
            let moved = measured_after(&[*previous, ratio]);
            assert_eq!(
                (moved.size.width.0, moved.size.height.0),
                (fresh.size.width.0, fresh.size.height.0),
                "a window that reached a ratio of {ratio} from {previous} laid its content-sized \
                 element out differently from one that opened at {ratio}"
            );
        }
    }
}

#[test]
fn a_content_sized_elements_device_extent_scales_with_the_ratio() {
    // The other half, and the one that says the re-layout produced the *right* answer rather than
    // merely a repeatable one. The fixed face advances every cluster half the font size, so eight
    // clusters at the initial size are four times it, and the padding is six CSS pixels a side.
    let one = measured_after(&[1.0]);
    for ratio in RATIOS {
        let box_ = measured_after(&[ratio]);
        let expected = (one.size.width.0 as f64 * ratio).round() as f32;
        assert!(
            (box_.size.width.0 - expected).abs() <= 1.0,
            "at a ratio of {ratio} an element {} device pixels wide at a ratio of one is {} wide, \
             and {expected} is what {ratio} times its content and padding comes to",
            one.size.width.0,
            box_.size.width.0
        );
    }
}
