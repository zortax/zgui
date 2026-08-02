//! Where the geometry a view observes says a box is, when something above it has been moved.
//!
//! A fragment keeps its rectangle in its own untransformed space, because that is the space its
//! clip, its corner radii and its children are all expressed in. What a *view* asks for is the
//! other thing: a surface placed against a trigger, a control measuring a gesture against itself
//! and a drag all compare a rectangle with the screen, and the screen is where the transform put
//! it.
//!
//! The failure this exists to catch has no wrong pixel anywhere in it. Every box is laid out
//! correctly, painted correctly and answers a pointer correctly; only the number handed back to a
//! view is in the wrong space, and the consequence is one component away — a menu opened from
//! inside a centred dialog appears half a panel from the control that opened it, because the panel
//! is centred by a transform and the trigger reported a place the transform had already moved it
//! from.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_view::{BuildCx, IntoView, NodeRef, View};

/// Where the box is laid out, and how far its own transform then moves it.
///
/// Both offsets are asymmetric and neither is a multiple of the other, so a reading that has lost
/// the transform, doubled it, or applied it to the wrong axis all say different things.
const AT: (f32, f32) = (10.0, 30.0);
/// How far the transform moves it.
const BY: (f32, f32) = (60.0, 20.0);
/// How big it is.
const SIZE: (f32, f32) = (100.0, 50.0);

/// A box at [`AT`], moved by [`BY`].
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block }
                   .box { width: 100px; height: 50px; margin-left: 10px; margin-top: 30px }
                   .moved { transform: translate(60px, 20px) }";

/// Opens a window holding one box of `class` and reports the border box it observed.
///
/// # Panics
///
/// Panics when nothing was ever delivered, because a fixture reading `None` would agree with every
/// answer there is.
fn observed(class: &'static str) -> Rect<DevicePx, Device> {
    let seen: Rc<Cell<Option<Rect<DevicePx, Device>>>> = Rc::new(Cell::new(None));
    let recorded = Rc::clone(&seen);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        let recorded = Rc::clone(&recorded);
        // Observation starts once the handle names a node, which is what a surface positioned
        // against its anchor does. Held for the life of the window: a dropped effect stops.
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if handle.get().is_none() {
                return;
            }
            let box_of = handle.observe_border_box();
            let recorded = Rc::clone(&recorded);
            core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                use zgui_reactive::prelude::Get;
                recorded.set(box_of.get());
            }));
        }));
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("box")
                        .class(class)
                        .node_ref(handle),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    let delivered = seen.get().expect("a box was delivered to the view");
    app.shut_down();
    delivered
}

#[test]
fn an_observed_box_is_where_it_is_laid_out_when_nothing_moves_it() {
    // The control. Without it the assertion below holds just as well for a reading that adds the
    // transform to a rectangle that already had it, and for one that reports the whole window.
    let plain = observed("still");
    assert_eq!(
        (plain.origin.x.0, plain.origin.y.0),
        AT,
        "an untransformed box is observed where layout put it"
    );
    assert_eq!((plain.size.width.0, plain.size.height.0), SIZE);
}

#[test]
fn an_observed_box_under_a_transform_is_where_the_transform_puts_it() {
    let moved = observed("moved");
    assert_eq!(
        (moved.origin.x.0, moved.origin.y.0),
        (AT.0 + BY.0, AT.1 + BY.1),
        "the box is drawn {BY:?} from where it was laid out, and that is where a view placing \
         something against it has to be told it is"
    );
    assert_eq!(
        (moved.size.width.0, moved.size.height.0),
        SIZE,
        "a translation moves a box without resizing it"
    );
}

/// How far the animation carries the box, and how long it takes.
///
/// A linear slide over a whole second, sampled while it is under way: the reading is then a
/// different number on every frame, which is the only way a reading that has stopped following can
/// be told from one that never followed.
const SLIDE: f32 = 240.0;
/// How long that slide lasts.
const SLIDE_MILLIS: u64 = 1_000;

/// The same box, sliding [`SLIDE`] pixels across the window for [`SLIDE_MILLIS`].
const SLIDING_CSS: &str = "root { display: block; width: 400px; height: 300px }
                           column { display: block }
                           .box { width: 100px; height: 50px; margin-left: 10px;
                                  margin-top: 30px }
                           .slides { transform: translateX(0px);
                                     animation: slide 1000ms linear }
                           @keyframes slide {
                               from { transform: translateX(0px) }
                               to { transform: translateX(240px) }
                           }";

#[test]
fn an_observed_box_follows_its_own_transform_frame_after_frame_while_it_animates() {
    // The case a per-frame reading exists for. An animated transform composes a *new* matrix on
    // every frame, so what the box is observed at is only right if the matrices a view is answered
    // from are as current as the fragments they belong to — on the twentieth frame of the animation
    // exactly as much as on the first.
    let seen: Rc<Cell<Option<Rect<DevicePx, Device>>>> = Rc::new(Cell::new(None));
    let recorded = Rc::clone(&seen);
    let mut app = support::app(SLIDING_CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        let recorded = Rc::clone(&recorded);
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if handle.get().is_none() {
                return;
            }
            let box_of = handle.observe_border_box();
            let recorded = Rc::clone(&recorded);
            core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                use zgui_reactive::prelude::Get;
                recorded.set(box_of.get());
            }));
        }));
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("box")
                        .class("slides")
                        .node_ref(handle),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    // Sampled every sixteen milliseconds through the slide, against where a linear ease says the
    // box is at that moment. The slack is one frame's worth of travel: the reading belongs to the
    // frame that was composed, and which side of the sample that frame's clock fell on is the
    // harness's business.
    let step = core::time::Duration::from_millis(16);
    let slack = SLIDE * 16.0 / SLIDE_MILLIS as f32 + 1.0;
    let mut elapsed = 0_u64;
    let mut sampled = 0;
    while elapsed + 16 < SLIDE_MILLIS {
        app.advance(step);
        app.pump();
        elapsed += 16;
        let Some(observed) = seen.get() else {
            continue;
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "an elapsed count of milliseconds under a thousand is exact in an f32"
        )]
        let fraction = elapsed as f32 / SLIDE_MILLIS as f32;
        let expected = AT.0 + SLIDE * fraction;
        assert!(
            (observed.origin.x.0 - expected).abs() <= slack,
            "at {elapsed}ms the box is drawn at x={expected} but was observed at \
             x={}; a view placing something against it would be that far out",
            observed.origin.x.0
        );
        assert_eq!(
            observed.origin.y.0, AT.1,
            "the slide is along one axis, so the other must not move"
        );
        sampled += 1;
    }
    app.shut_down();
    assert!(
        sampled > 20,
        "only {sampled} frames of the slide were sampled, which is too few to say the reading kept \
         following it"
    );
}
