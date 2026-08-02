//! The bars a scrolling document draws, and what a press on one does.
//!
//! Every assertion here is made over a real window: the fragments come out of the tree the frame
//! composed, and the rectangles they are checked against come out of the *scene* — what was
//! actually pushed to the display list — rather than out of the arithmetic that produced them. A
//! bar that is computed correctly and emitted by nothing looks exactly like the fault this is
//! written against, which was a fragment kind with two consumers and no producer at all.

mod support;

use zgui_geom::{Css, CssPx, Point};
use zgui_layout::fragment::ScrollbarPart;
use zgui_layout::{Axis, FragmentKind};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A page filling the window, whose content is twice as tall as it is.
///
/// `overflow-y: scroll` rather than `auto`, so the container is a scroll container whatever its
/// content measures to and the fixture cannot quietly stop testing scrolling; and only on the one
/// axis, so the numbers below are about one bar rather than about two and a corner.
const CSS: &str = "
root { display: block; width: 100%; height: 100%; overflow-x: hidden; overflow-y: scroll }
.filler { display: block; width: 100%; height: 600px; background-color: #202020 }
.scrim { position: fixed; left: 0; top: 0; right: 0; bottom: 0; background-color: #000080 }
";

/// The same page holding content that fits, so the gutter is reserved and nothing can move.
const SHORT_CSS: &str = "
root { display: block; width: 100%; height: 100%; overflow-x: hidden; overflow-y: scroll }
.filler { display: block; width: 100%; height: 100px; background-color: #202020 }
";

/// How wide a gutter is, in device pixels, which is what layout reserved for the bar.
const GUTTER: f32 = 15.0;

/// A window holding one scrolling page, optionally covered by a fixed scrim.
fn page(css: &'static str, scrim: bool) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
        let mut root = zgui_elements::column()
            .class("root")
            .child(zgui_elements::column().class("filler"));
        if scrim {
            root = root.child(zgui_elements::column().class("scrim"));
        }
        Box::new(root.into_view().build(cx))
    })
}

/// The window the harness is driving.
fn window(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> &zgui_runtime::Window {
    harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window")
}

/// Every scrollbar fragment in the document, as its part and its rectangle in device pixels.
fn bars(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Vec<(Axis, ScrollbarPart, [f32; 4])> {
    let window = window(harness);
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            let FragmentKind::Scrollbar { axis, part } = fragment.kind else {
                continue;
            };
            let box_ = fragment.border_box;
            found.push((
                axis,
                part,
                [
                    box_.origin.x.0,
                    box_.origin.y.0,
                    box_.size.width.0,
                    box_.size.height.0,
                ],
            ));
        }
    }
    found
}

/// The rectangle of the one bar of `part`, or a panic naming what there was instead.
fn bar(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    part: ScrollbarPart,
) -> [f32; 4] {
    let found = bars(harness);
    let mut matched = found
        .iter()
        .filter(|(axis, held, _)| *axis == Axis::Vertical && *held == part);
    let first = *matched
        .next()
        .unwrap_or_else(|| panic!("no vertical {part:?}; the document has {found:?}"));
    assert!(matched.next().is_none(), "more than one vertical {part:?}");
    first.2
}

/// Whether the scene holds a quad covering exactly `rect`.
///
/// The display list rather than the fragment tree, because a fragment that nothing emits for is the
/// whole of the fault this file exists for.
fn painted(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    rect: [f32; 4],
) -> bool {
    window(harness).scene().primitives.quads.iter().any(|quad| {
        quad.bounds
            .iter()
            .zip(rect.iter())
            .all(|(drawn, wanted)| (drawn - wanted).abs() < 0.01)
    })
}

/// Where the page is scrolled to, in device pixels down.
fn offset(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let window = window(harness);
    let element = {
        let layout = window.layout().borrow();
        let root = layout.root().expect("a root box");
        layout
            .node(root)
            .source
            .expect("the root came from an element")
    };
    window.scroll().borrow().offset_of(element).y.0
}

/// Where a point in the transformed scroller's own space is drawn on the device.
///
/// The transform written out as arithmetic, which is what lets an assertion state where a pointer
/// must go rather than record where one happened to work.
type Onto = fn(f32, f32) -> (f32, f32);

/// How wide and tall the transformed scroller is in its own space.
const PANEL: f32 = 100.0;

/// How tall its content is, in the same space.
const CONTENT: f32 = 200.0;

/// A page holding one scroller of `PANEL` square under `transform`, with `CONTENT` inside it.
///
/// The transform is anchored at the scroller's own origin, so that the map from its space to the
/// device is the transform itself and a test can state where a point *must* land rather than
/// discover it.
fn warped(transform: &str) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let css = format!(
        "root {{ display: block; width: 100%; height: 100% }}
         .panel {{ display: block; width: {PANEL}px; height: {PANEL}px; overflow-x: hidden;
                   overflow-y: scroll; transform: {transform}; transform-origin: 0 0 }}
         .tall {{ display: block; width: 100%; height: {CONTENT}px }}"
    );
    support::app_with_text(&css, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("panel")
                        .child(zgui_elements::column().class("tall")),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// The element that scrolls: the one whose box drew a vertical bar.
fn scroller(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> zgui_dom::NodeKey {
    let window = window(harness);
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let drew_a_bar = layout
            .fragments_of_box(key)
            .iter()
            .filter_map(|frag| layout.fragment(*frag))
            .any(|fragment| matches!(fragment.kind, FragmentKind::Scrollbar { .. }));
        if drew_a_bar && let Some(source) = layout.node(key).source {
            return source;
        }
    }
    panic!("no element in the document drew a scrollbar");
}

/// Where the scrolling element is scrolled to, in its own pixels down.
fn scrolled(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let element = scroller(harness);
    window(harness).scroll().borrow().offset_of(element).y.0
}

/// Delivers one pointer action at `(x, y)` in CSS pixels and settles the frames it asks for.
fn pointer(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    action: PointerAction,
    at: (f32, f32),
) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::<CssPx, Css>::new(CssPx(at.0), CssPx(at.1))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
}

#[test]
fn a_scrolling_page_draws_a_track_and_a_thumb_into_the_gutter_it_reserved() {
    let mut harness = page(CSS, false);
    harness.settle(8);

    let track = bar(&harness, ScrollbarPart::Track);
    assert_eq!(
        track,
        [400.0 - GUTTER, 0.0, GUTTER, 300.0],
        "the track fills the strip the content was moved out of"
    );

    // Six hundred of content in a three-hundred port: half of it is visible, so the thumb is half
    // the track, and it starts at the top because nothing has scrolled.
    let thumb = bar(&harness, ScrollbarPart::Thumb);
    assert_eq!(thumb, [400.0 - GUTTER, 0.0, GUTTER, 150.0]);

    assert!(
        painted(&harness, track),
        "the track reached the display list"
    );
    assert!(painted(&harness, thumb), "and so did the thumb");
}

#[test]
fn a_reserved_gutter_with_nothing_to_scroll_has_a_track_and_no_thumb() {
    let mut harness = page(SHORT_CSS, false);
    harness.settle(8);

    let found = bars(&harness);
    assert_eq!(
        found.len(),
        1,
        "one piece of bar and no more, and it is {found:?}"
    );
    assert_eq!(found[0].1, ScrollbarPart::Track);
    assert!(
        painted(&harness, found[0].2),
        "the strip is filled even though nothing can move, because it is space the content will \
         never cover"
    );
}

#[test]
fn a_fixed_scrim_and_the_gutter_together_cover_the_whole_window() {
    // The reported fault: a backdrop leaves an uncovered strip down the right. A fixed box covers
    // the viewport by definition, and the viewport is the window less its gutters — so the strip is
    // only a hole while the gutter is empty, and the test is that the two rectangles now tile the
    // window between them.
    let mut harness = page(CSS, true);
    harness.settle(8);

    let track = bar(&harness, ScrollbarPart::Track);
    let scrim = {
        let window = window(&harness);
        let layout = window.layout().borrow();
        let mut widest: Option<[f32; 4]> = None;
        for key in layout.keys() {
            let Some(node) = layout.get(key) else {
                continue;
            };
            if node.style.get_box().position != zgui_css::values::size::PositionValue::Fixed {
                continue;
            }
            let Some(frag) = layout.fragments_of_box(key).first().copied() else {
                continue;
            };
            let Some(fragment) = layout.fragment(frag) else {
                continue;
            };
            let box_ = fragment.border_box;
            widest = Some([
                box_.origin.x.0,
                box_.origin.y.0,
                box_.size.width.0,
                box_.size.height.0,
            ]);
        }
        widest.expect("the fixed scrim was laid out")
    };

    assert_eq!(
        scrim,
        [0.0, 0.0, 400.0 - GUTTER, 300.0],
        "the scrim covers the viewport, which is the window less its gutter"
    );
    assert_eq!(
        scrim[0] + scrim[2],
        track[0],
        "and the track begins exactly where it ends, with no strip between them"
    );
    assert_eq!(track[0] + track[2], 400.0, "reaching the window's edge");
    assert!(painted(&harness, track));
}

#[test]
fn a_press_on_the_thumb_keeps_its_grab_and_drags_the_content_proportionally() {
    let mut harness = page(CSS, false);
    harness.settle(8);
    assert_eq!(offset(&harness), 0.0);

    // Forty pixels down a thumb that runs from zero to a hundred and fifty. A press that moved
    // anything is the fault reported — "clicking on them makes them jump down a bit" — so the
    // first assertion is that the press alone does nothing at all.
    pointer(&mut harness, PointerAction::Moved, (392.0, 40.0));
    pointer(&mut harness, PointerAction::Pressed, (392.0, 40.0));
    assert_eq!(
        offset(&harness),
        0.0,
        "pressing the thumb must not move the content: the grab is what the drag keeps"
    );

    // Down thirty. The thumb's travel is 300 - 150 = 150 and the content's is 600 - 300 = 300, so
    // the content moves twice as far as the pointer did.
    pointer(&mut harness, PointerAction::Moved, (392.0, 70.0));
    assert!(
        (offset(&harness) - 60.0).abs() < 0.51,
        "thirty pixels of thumb is sixty of content; the offset is {}",
        offset(&harness)
    );
    assert_eq!(
        bar(&harness, ScrollbarPart::Thumb)[1],
        30.0,
        "and the thumb's near edge is under the pointer's grab, where it was taken"
    );

    // Past the end, which the content cannot follow.
    pointer(&mut harness, PointerAction::Moved, (392.0, 900.0));
    assert_eq!(
        offset(&harness),
        300.0,
        "clamped to what the content allows"
    );

    // And let go: the pointer no longer drives it.
    pointer(&mut harness, PointerAction::Released, (392.0, 900.0));
    pointer(&mut harness, PointerAction::Moved, (392.0, 20.0));
    assert_eq!(
        offset(&harness),
        300.0,
        "a move after the release is not part of the drag"
    );
}

#[test]
fn a_press_on_the_track_pages_towards_it() {
    let mut harness = page(CSS, false);
    harness.settle(8);

    // Below the thumb, which runs from zero to a hundred and fifty: one screenful on.
    pointer(&mut harness, PointerAction::Moved, (392.0, 250.0));
    pointer(&mut harness, PointerAction::Pressed, (392.0, 250.0));
    assert_eq!(
        offset(&harness),
        300.0,
        "a screenful is three hundred, and that is also the end"
    );
    pointer(&mut harness, PointerAction::Released, (392.0, 250.0));

    // Above the thumb, which now runs from a hundred and fifty to three hundred: one screenful back.
    pointer(&mut harness, PointerAction::Moved, (392.0, 40.0));
    pointer(&mut harness, PointerAction::Pressed, (392.0, 40.0));
    assert_eq!(offset(&harness), 0.0);
}

#[test]
fn a_thumb_is_dragged_in_its_own_space_however_its_scroller_is_transformed() {
    // Why this needs a transform to say anything at all: a fragment keeps its rectangle in its own
    // space, and a pointer arrives in device pixels. Every other test here has an identity chain,
    // where those are the same numbers — so a drag that subtracts one from the other is arithmetic
    // on equal quantities and cannot be seen to be wrong. Put the scroller under a scale and the two
    // part company: the grab is a difference of two spaces, the thumb jumps by it on the first move,
    // and everything after that tracks at the device's rate rather than the bar's.
    //
    // Both maps below are the transform written out, not a run recorded. Anchored at the scroller's
    // own origin, a point (x, y) of its space is drawn wherever the transform sends it.
    let cases: [(&str, Onto); 2] = [
        ("translate(100px, 20px) scale(2)", |x, y| {
            (100.0 + 2.0 * x, 20.0 + 2.0 * y)
        }),
        // A quarter turn, which also crosses the axes: the bar runs down the scroller's own y and
        // across the device's x, so a drag that projected the pointer onto the device's y would not
        // move at all here however far the pointer went.
        ("translate(200px, 40px) rotate(90deg)", |x, y| {
            (200.0 - y, 40.0 + x)
        }),
    ];

    // What the bar is, in the scroller's own space, from the two lengths and the gutter.
    let middle = PANEL - GUTTER / 2.0;
    let thumb = PANEL * (PANEL / CONTENT);
    let free = PANEL - thumb;
    let limit = CONTENT - PANEL;
    // Grabbed part-way down the thumb and dragged twenty of the scroller's pixels further on, so
    // the thumb's near edge ends at twenty and the grab is what has to survive the move.
    let (grabbed, moved) = (20.0, 40.0);
    let edge = moved - grabbed;
    let expected = edge / free * limit;

    for (transform, device) in cases {
        let mut harness = warped(transform);
        harness.settle(8);

        assert_eq!(
            bar(&harness, ScrollbarPart::Thumb),
            [PANEL - GUTTER, 0.0, GUTTER, thumb],
            "{transform}: the thumb is measured in the scroller's own space, where the transform \
             has not touched it"
        );

        let press = device(middle, grabbed);
        pointer(&mut harness, PointerAction::Moved, press);
        pointer(&mut harness, PointerAction::Pressed, press);
        assert_eq!(
            scrolled(&harness),
            0.0,
            "{transform}: the press itself moves nothing"
        );

        pointer(&mut harness, PointerAction::Moved, device(middle, moved));
        assert!(
            (scrolled(&harness) - expected).abs() < 0.51,
            "{transform}: twenty of the scroller's pixels is {expected} of content, not {}",
            scrolled(&harness)
        );
        assert_eq!(
            bar(&harness, ScrollbarPart::Thumb)[1],
            edge,
            "{transform}: and the thumb's near edge is under the grab it was taken by"
        );

        pointer(&mut harness, PointerAction::Released, device(middle, moved));
    }
}

#[test]
fn the_bar_follows_a_resize() {
    let mut harness = page(CSS, false);
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::Resized(zgui_geom::Size::new(
        zgui_geom::DevicePx(500.0),
        zgui_geom::DevicePx(400.0),
    )));
    harness.settle(8);

    let track = bar(&harness, ScrollbarPart::Track);
    assert_eq!(
        track,
        [500.0 - GUTTER, 0.0, GUTTER, 400.0],
        "the track moved with the window's right edge and grew with its height"
    );
    assert!(painted(&harness, track));
}

#[test]
fn at_a_fractional_scale_the_track_begins_exactly_where_the_content_ends() {
    // The seam this is written against: the content box and the track are both derived from the
    // gutter, and two roundings of one quantity part company at a scale that is not a whole number.
    // They are derived from each other's edge instead, so there is one number and no seam.
    let mut harness = page(CSS, false);
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 1.25,
        size: zgui_geom::Size::new(zgui_geom::DevicePx(501.0), zgui_geom::DevicePx(401.0)),
    });
    harness.settle(8);

    let track = bar(&harness, ScrollbarPart::Track);
    let content = {
        let window = window(&harness);
        let layout = window.layout().borrow();
        let root = layout.root().expect("a root box");
        let frag = layout.fragments_of_box(root)[0];
        layout.fragment(frag).expect("a fragment").content_box
    };
    assert_eq!(
        track[0],
        content.origin.x.0 + content.size.width.0,
        "the track begins on the content box's own right edge"
    );
    assert_eq!(track[0] + track[2], 501.0, "and reaches the surface's edge");
    assert!(painted(&harness, track));
}
