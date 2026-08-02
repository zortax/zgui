//! A frame that damages nothing must not reach the device at all.
//!
//! Not an optimisation: presenting an unchanged frame spends a swap-chain image, and the next
//! frame that *does* change something then waits for the display to hand one back. On the machine
//! this was written against that wait is a whole refresh interval, which turns a two-millisecond
//! interaction into a twenty-millisecond one — and the frames it happens to are the ones a person
//! is looking at, because they are the ones that follow an event closely enough to be a response
//! to it.
//!
//! An empty damage set is an ordinary event, not a mistake: a pointer press over something with no
//! pressed appearance, a key that moved no caret, a wake that turned out to concern another
//! window. Each of those runs the whole pipeline and arrives at the renderer with nothing to
//! redraw.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_render::{FrameOutcome, Renderer, SkipReason};
use zgui_scene::{Quad, Scene};

use support::{SIDE, opaque, plain_renderer, present};

/// A scene holding one rectangle of `colour`.
fn one_quad(colour: [u8; 3]) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene.paints.add(zgui_scene::Paint::Solid(opaque(
        colour[0], colour[1], colour[2],
    )));
    scene.push_quad(Quad::filled(
        Rect::new(
            Point::new(DevicePx(16.0), DevicePx(16.0)),
            Size::new(DevicePx(64.0), DevicePx(64.0)),
        ),
        paint,
    ));
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_frame_with_nothing_to_redraw_is_not_composed_and_not_presented() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let first = one_quad([200, 40, 40]);
    let before = present(&mut renderer, &first);

    // The same scene again with an empty damage set. Nothing has changed, so nothing may be
    // recorded, submitted or presented.
    let outcome = renderer.draw(&first, &DamageSet::new());
    assert_eq!(
        outcome,
        FrameOutcome::Skipped(SkipReason::Undamaged),
        "an empty damage set has to skip before any device work"
    );
    assert!(
        !outcome.wants_another_frame(),
        "nothing changed, so asking for another frame would ask for the same nothing for ever"
    );
    assert!(
        outcome.retires_damage(),
        "there was no damage to carry forward"
    );

    let after = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        before, after,
        "the surface has to still hold the frame before it"
    );
}

#[test]
fn a_scene_that_did_change_still_reaches_the_device_when_its_damage_says_so() {
    // The counterfactual for the test above: the skip must be decided by the damage set and by
    // nothing else, or it would swallow real frames whose scene happens to be cheap.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let before = present(&mut renderer, &one_quad([200, 40, 40]));

    let mut damage = DamageSet::new();
    damage.absorb(Rect::new(Point::new(0, 0), Size::new(SIDE, SIDE)));
    let outcome = renderer.draw(&one_quad([40, 200, 40]), &damage);
    assert!(
        outcome.stats().is_some(),
        "a damaged frame reaches the target, but this one reported {outcome:?}"
    );

    let after = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_ne!(before, after, "the frame changed the rectangle it damaged");
}
