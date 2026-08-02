//! What an animated transform costs, and whether everything it moves actually moves with it.
//!
//! A transform is the one animated property that is neither paint nor layout. Nothing about it is
//! inherited, no size is computed from it and no box is rebuilt for it — and yet it moves the
//! rectangle the element covers, the rectangle a click is answered over, and the position of
//! everything drawn inside it. So the tier that serves it is only correct if all three follow, and
//! each of the three fails silently on its own: stale ink is a trail of pixels left behind, a stale
//! hit entry is a control that answers where it used to be, and a descendant left behind is a label
//! that slides out of the button it is written on.
//!
//! Every case drives the real loop, over frames the loop asked for itself.

mod support;

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::{COUNTERS_ENABLED, counter};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// Held for the whole of any case that reads a counter, which is all of them.
static COUNTERS: Mutex<()> = Mutex::new(());

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// A bar that slides across a track, with a label written on it.
///
/// The bar declares a transform of its own, so the animation moves one that is already there —
/// which is what keeps the answers the shared style gives about the element, above all whether it
/// establishes a stacking context, the same throughout.
const SLIDE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .track { display: block; position: relative;
                                  width: 400px; height: 40px }
                         .bar { display: block; width: 100px; height: 40px;
                                background-color: rgb(20, 120, 220);
                                transform: translateX(0px);
                                animation: slide 1000ms linear infinite }
                         .pip { display: block; width: 10px; height: 10px;
                                background-color: rgb(250, 250, 250) }
                         @keyframes slide {
                             from { transform: translateX(0px) }
                             to { transform: translateX(300px) }
                         }";

/// A bar that slides once and stops, holding nothing.
///
/// Without a fill mode the element goes back to the transform its own style asks for the moment the
/// animation is over, which is the one thing an override written outside the cascade cannot do by
/// expiring: something has to compose the box again.
const ONCE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                        .track { display: block; position: relative;
                                 width: 400px; height: 40px }
                        .bar { display: block; width: 100px; height: 40px;
                               background-color: rgb(20, 120, 220);
                               transform: translateX(0px);
                               animation: slide 100ms linear }
                        .pip { display: block; width: 10px; height: 10px;
                               background-color: rgb(250, 250, 250) }
                        @keyframes slide {
                            from { transform: translateX(300px) }
                            to { transform: translateX(300px) }
                        }";

/// The same bar, with no transform of its own for the animation to move.
///
/// This one may *not* take the placement tier on the frame its animation begins: an element that
/// acquires a transform acquires a stacking context and a containing block with it, and both are
/// read from the shared style.
const APPEARS_CSS: &str = "root { display: block; width: 400px; height: 300px }
                           .track { display: block; position: relative;
                                    width: 400px; height: 40px }
                           .bar { display: block; width: 100px; height: 40px;
                                  background-color: rgb(20, 120, 220);
                                  animation: slide 1000ms linear infinite }
                           @keyframes slide {
                               from { transform: translateX(0px) }
                               to { transform: translateX(300px) }
                           }";

/// A track holding one sliding bar with a pip inside it.
fn sliding_bar(css: &'static str) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app(css, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column().class("root").child(
            zgui_elements::column().class("track").child(
                zgui_elements::column()
                    .class("bar")
                    .child(zgui_elements::column().class("pip")),
            ),
        );
        Box::new(view.into_view().build(cx))
    })
}

/// A pointer event at a point in the window, in CSS pixels.
fn pointer_at(action: PointerAction, x: f32, y: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// The device-space ink rectangle of the first fragment of every element carrying `class`.
fn inks(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
) -> Vec<(i32, i32, i32, i32)> {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for index in 0..document.store().slot_count() {
        let index = zgui_dom::NodeIndex::new(index as u32);
        if document.store().try_core(index).is_none() {
            continue;
        }
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| &**held == class)
        {
            continue;
        }
        let key = document.store().key_of(index);
        for box_key in layout.boxes_of(key) {
            let Some(&frag) = layout.fragments_of_box(*box_key).first() else {
                continue;
            };
            let Some(fragment) = layout.fragment(frag) else {
                continue;
            };
            let ink = fragment.ink;
            found.push((
                ink.origin.x.0 as i32,
                ink.origin.y.0 as i32,
                ink.size.width.0 as i32,
                ink.size.height.0 as i32,
            ));
        }
    }
    found
}

/// Whether any element carrying `class` is currently hovered.
fn hovered(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>, class: &str) -> bool {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    for index in 0..document.store().slot_count() {
        let index = zgui_dom::NodeIndex::new(index as u32);
        let Some(core) = document.store().try_core(index) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| &**held == class)
        {
            continue;
        }
        if core.ui_state().contains(zgui_vocab::UiState::HOVER) {
            return true;
        }
    }
    false
}

#[test]
fn a_sliding_bar_is_re_placed_and_never_re_cascaded() {
    let _guard = measuring();
    let mut harness = sliding_bar(SLIDE_CSS);
    harness.settle(8);
    // The animation is created by the first cascade and starts on the tick that follows it, and
    // that first tick is the one that may still cascade — the element's own style has to carry the
    // transform before the placement tier will accept it.
    for _ in 0..3 {
        harness.advance(FRAME);
        harness.pump();
    }

    let before = inks(&harness, "bar");
    harness.reset_counts();
    counter::reset();
    harness.advance(FRAME);
    assert_eq!(harness.pump(), 1, "the reached deadline produced one frame");
    let frame = counter::snapshot();
    let after = inks(&harness, "bar");

    assert_eq!(before.len(), 1, "the bar has one fragment");
    assert_ne!(
        before, after,
        "the bar took a tier and its ink never moved: the animation is not on the screen"
    );

    if COUNTERS_ENABLED {
        assert_eq!(
            frame.elements_restyled, 0,
            "a transform that moved nothing but where the box is drawn ran the cascade"
        );
        assert_eq!(
            frame.tier_c_placements, 1,
            "the sliding bar did not take the placement tier"
        );
        assert_eq!(
            frame.boxes_rebuilt, 0,
            "a transform rebuilt a box, which is the tier's whole point"
        );
    }
    assert!(
        harness.parked_deadline().is_some(),
        "the loop parked with no deadline: the animation would never tick again"
    );
    harness.assert_park_invariant();
}

#[test]
fn what_is_drawn_inside_a_sliding_bar_slides_with_it() {
    // The descendant half. A transform is composed down the tree as a matrix, so a box inside the
    // animated one is at a different place on the device with no style of its own having moved —
    // and a tier that recomposed only the animated element would leave the label behind while the
    // button slid out from under it.
    let _guard = measuring();
    let mut harness = sliding_bar(SLIDE_CSS);
    harness.settle(8);
    for _ in 0..3 {
        harness.advance(FRAME);
        harness.pump();
    }

    let bar_before = inks(&harness, "bar");
    let pip_before = inks(&harness, "pip");
    harness.advance(FRAME);
    assert_eq!(harness.pump(), 1);
    let bar_after = inks(&harness, "bar");
    let pip_after = inks(&harness, "pip");

    assert_eq!(pip_before.len(), 1, "the pip has one fragment");
    let moved = |from: &[(i32, i32, i32, i32)], to: &[(i32, i32, i32, i32)]| to[0].0 - from[0].0;
    assert_ne!(moved(&bar_before, &bar_after), 0, "the bar did not move");
    assert_eq!(
        moved(&pip_before, &pip_after),
        moved(&bar_before, &bar_after),
        "the pip did not move with the bar it is drawn inside"
    );
}

#[test]
fn a_click_lands_on_a_sliding_bar_where_it_now_is() {
    // The hit half, and the one that fails most quietly: the index answers over a rectangle it was
    // given, so a tier that moved the fragment and not the entry leaves every control answering
    // where it was when its animation started.
    let _guard = measuring();
    let mut harness = sliding_bar(SLIDE_CSS);
    harness.settle(8);

    // A point the bar starts to the left of and slides onto. The bar is a hundred wide at the
    // origin and travels three hundred, so this is outside it at the start and inside it later.
    let probe = (250.0, 20.0);
    harness.deliver_to_first(pointer_at(PointerAction::Moved, probe.0, probe.1));
    harness.settle(4);
    assert!(
        !hovered(&harness, "bar"),
        "the probe is inside the bar before it has slid there"
    );

    // Far enough into the animation that the bar has passed the probe. The pointer does not move
    // again: what puts it inside the bar is the bar arriving under it.
    let mut arrived = false;
    for _ in 0..40 {
        harness.advance(FRAME);
        harness.pump();
        if hovered(&harness, "bar") {
            arrived = true;
            break;
        }
    }
    assert!(
        arrived,
        "the bar slid under a stationary pointer and the hit index never noticed"
    );
}

#[test]
fn a_bar_whose_only_transform_is_the_animations_own_is_placed_too() {
    // The commonest shape in a component library, and the one the gallery's progress bar has: the
    // element declares no transform and its keyframes are the only thing that gives it one.
    //
    // It reaches the same tier, and the reason is worth stating because it is not obvious. A
    // running animation contributes declarations to the *cascade*, at its own origin — so the very
    // first cascade that starts the animation already resolves the element's own style to the value
    // at time zero, and every style the tick ever compares against carries a transform. The element
    // is therefore never seen crossing the line the placement path refuses to cross, and the
    // refusal is left guarding the case where the two do disagree.
    let _guard = measuring();
    let mut harness = sliding_bar(APPEARS_CSS);
    harness.settle(8);
    for _ in 0..3 {
        harness.advance(FRAME);
        harness.pump();
    }

    let before = inks(&harness, "bar");
    harness.reset_counts();
    counter::reset();
    harness.advance(FRAME);
    assert_eq!(harness.pump(), 1);
    let frame = counter::snapshot();

    assert_ne!(before, inks(&harness, "bar"), "the bar never moved");
    if COUNTERS_ENABLED {
        assert_eq!(
            frame.tier_c_placements, 1,
            "the bar never reached the placement tier"
        );
        assert_eq!(frame.elements_restyled, 0, "it went on cascading");
        assert_eq!(frame.boxes_rebuilt, 0, "it went on rebuilding boxes");
    }
}

#[test]
fn a_bar_goes_back_where_its_style_puts_it_when_the_animation_ends() {
    // The retirement, and the defect it closes is permanent rather than transient: the placement an
    // animation wrote is read while the box is composed, and the style the box would otherwise be
    // composed from never moved. So nothing in a later frame would ever ask for the box to be
    // composed again, and it would keep the position the animation's last frame put it in for the
    // rest of the document's life.
    let _guard = measuring();
    let mut harness = sliding_bar(ONCE_CSS);
    harness.settle(8);
    for _ in 0..3 {
        harness.advance(FRAME);
        harness.pump();
    }
    let moved = inks(&harness, "bar");

    // Well past the hundred milliseconds the animation runs for.
    for _ in 0..16 {
        harness.advance(FRAME);
        harness.pump();
    }
    let settled = inks(&harness, "bar");

    let track = inks(&harness, "track");
    assert_ne!(
        moved, settled,
        "the bar stayed where the animation's last frame left it"
    );
    assert_eq!(
        settled[0].0, track[0].0,
        "the bar did not come back to the place its own style puts it: {settled:?}"
    );
    assert!(
        harness.parked_deadline().is_none(),
        "the loop kept a deadline for an animation that had ended"
    );
    harness.assert_park_invariant();
}

/// The coordinate system the first box of the first element carrying `class` establishes, and the
/// matrix it resolves to in the frame that was just drawn.
fn space_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    class: &str,
) -> Option<(zgui_scene::SpatialId, zgui_geom::Matrix4)> {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    let spatial = &window.scene().spatial;
    for index in 0..document.store().slot_count() {
        let index = zgui_dom::NodeIndex::new(index as u32);
        if document.store().try_core(index).is_none() {
            continue;
        }
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| &**held == class)
        {
            continue;
        }
        let key = document.store().key_of(index);
        for box_key in layout.boxes_of(key) {
            let owner = zgui_scene::PropertyOwner::of(*box_key);
            if let Some(id) = spatial.of(owner) {
                return Some((id, spatial.resolve(id)?));
            }
        }
    }
    None
}

/// Whether anything in the frame that was drawn names `space`'s slot.
fn drawn_under(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    space: zgui_scene::SpatialId,
) -> bool {
    let window = harness
        .app()
        .windows()
        .first()
        .expect("the application opened a window");
    window
        .scene()
        .primitives
        .quads
        .iter()
        .any(|quad| quad.transform == space.index())
}

#[test]
fn spatial_ids_are_stable_across_an_animation() {
    // What the representation is *for*, said in the one place it can be observed end to end.
    //
    // A coordinate system interned by its matrix is named by its value, so an element moving a
    // pixel a frame is handed a different name on every frame of its movement: a hundred and twenty
    // names for one element's one coordinate system, none of which anything can cache output under.
    // Naming it after the box instead makes a tick a *write*, and the assertion below is that
    // sentence with nothing else in it — the name at the start of the slide is the name at the end,
    // while the matrix underneath it has been somewhere else on nearly every frame in between.
    //
    // The two negative halves matter as much as the positive one. The tree must not have grown, or
    // the names are being minted and merely happen to compare equal at the two ends; and the matrix
    // must actually have moved, or the fixture is asserting stability over an animation that never
    // ran.
    let _guard = measuring();
    let mut harness = sliding_bar(SLIDE_CSS);
    harness.settle(8);
    for _ in 0..3 {
        harness.advance(FRAME);
        harness.pump();
    }

    let (first, from) = space_of(&harness, "bar").expect("the bar establishes one of its own");
    assert!(
        drawn_under(&harness, first),
        "nothing in the drawn frame is carried by the bar's coordinate system",
    );
    let nodes = |harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        harness
            .app()
            .windows()
            .first()
            .expect("the application opened a window")
            .scene()
            .spatial
            .len()
    };
    let before = nodes(&harness);

    let mut places = std::collections::BTreeSet::new();
    let mut latest = first;
    for frame in 1..=120 {
        harness.advance(FRAME);
        harness.pump();
        let (name, matrix) = space_of(&harness, "bar").expect("the bar is still in the document");
        assert_eq!(
            name, first,
            "the bar's coordinate system was renamed on frame {frame}",
        );
        places.insert(format!("{:?}", matrix.columns));
        latest = name;
    }

    assert_eq!(latest, first);
    assert!(
        places.len() > 2,
        "the bar's matrix took {} distinct values over a hundred and twenty frames, so this \
         asserted stability over an animation that was not running",
        places.len(),
    );
    assert_ne!(
        space_of(&harness, "bar").expect("still there").1,
        from,
        "the bar ended where it started, so nothing was under test",
    );
    assert_eq!(
        nodes(&harness),
        before,
        "the document grew a coordinate system per frame of the animation",
    );
    assert!(
        drawn_under(&harness, first),
        "the bar's own name stopped being what its primitives are carried by",
    );
}

#[test]
fn a_settled_transform_animation_writes_one_property_and_rebuilds_nothing() {
    // What the tier is *for*, measured rather than described. A transform moves where a box is
    // drawn and nothing else, and where a box is drawn is one matrix in one node the box already
    // owns — so once the display list knows the region the movement covers, a frame of the
    // animation costs that write and the union of where the ink was and where it is. Nothing is
    // styled, no box is rebuilt, no fragment is composed again and nothing is painted afresh.
    //
    // "Once it knows the region" is why this measures the second pass rather than the first. Draw
    // order is assigned before anything moves, so a box that will be moved by a write has to be
    // ordered against everywhere it will go; nothing upstream states that, so it is learnt from the
    // first pass — which composes, exactly as a transform always did.
    let _guard = measuring();
    let mut harness = sliding_bar(SLIDE_CSS);
    harness.settle(8);
    // Two full cycles of the thousand-millisecond animation, at a frame apiece.
    for _ in 0..120 {
        harness.advance(FRAME);
        harness.pump();
    }

    let before = space_of(&harness, "bar")
        .expect("the bar establishes one of its own")
        .1;
    harness.reset_counts();
    counter::reset();
    for _ in 0..120 {
        harness.advance(FRAME);
        harness.pump();
    }
    let frame = counter::snapshot();
    let after = space_of(&harness, "bar")
        .expect("the bar is still in the document")
        .1;

    assert_ne!(
        before, after,
        "the bar's matrix never moved, so this measured an animation that was not running"
    );
    if COUNTERS_ENABLED {
        assert!(
            frame.place_writes_without_reemit > 0,
            "not one frame of the animation was served by a write"
        );
        assert_eq!(
            frame.place_writes_with_reemit, 0,
            "the animation was still being composed after a whole cycle of learning where it goes"
        );
        assert_eq!(
            frame.order_band_escapes, 0,
            "the bar left the region it declared"
        );
        assert_eq!(frame.boxes_rebuilt, 0, "a written transform rebuilt a box");
        assert_eq!(
            frame.fragments_rebuilt, 0,
            "a written transform composed a fragment again"
        );
        assert_eq!(
            frame.elements_restyled, 0,
            "a written transform ran the cascade"
        );
        assert_eq!(
            frame.repaints, 0,
            "a written transform re-encoded what it had already drawn"
        );
    }
}
