//! The frame loop: what a frame produced, and how the loop parked afterwards.

use std::time::Duration;

use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_profile::Counter;
use zgui_render::Renderer;
use zgui_scene::{PaintRef, Quad};
use zgui_testkit_scene::harness::park::ParkModel;
use zgui_testkit_scene::{Fixture, FrameCx, Harness};

/// A device rectangle in integer pixels.
fn ink(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

/// A fixture that draws one named card and damages it.
fn card_fixture() -> Fixture {
    Fixture::new(|cx: &mut FrameCx<'_>| {
        let bounds = ink(4, 4, 64, 24);
        let fill = PaintRef::solid(cx.scene().paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
        cx.scene().push_quad(Quad::filled(
            Rect::new(
                Point::new(DevicePx(4.0), DevicePx(4.0)),
                Size::new(DevicePx(64.0), DevicePx(24.0)),
            ),
            fill,
        ));
        cx.damage_rect(bounds);
        cx.record_subject("#card", bounds);
    })
}

#[test]
fn a_frame_reports_what_it_drew_and_what_it_damaged() {
    let mut harness = Harness::new(card_fixture());
    harness.frame();

    assert_eq!(harness.frames(), 1);
    assert_eq!(harness.ink_of("#card"), ink(4, 4, 64, 24));
    assert!(harness.query("#card").is_some());
    assert!(harness.query("#missing").is_none());
    assert!(
        harness
            .damage_rects()
            .iter()
            .any(|rect| rect.contains_rect(harness.ink_of("#card")))
    );
    assert!(
        harness
            .transcript()
            .as_str()
            .contains("quad order=1 bounds=rect(4, 4, 64, 24)")
    );
}

#[test]
#[should_panic(expected = "drew no subject called `#absent`")]
fn asking_for_a_subject_the_frame_did_not_draw_is_a_failure_and_not_an_empty_rectangle() {
    // An absent subject reported as an empty rectangle would make every containment assertion
    // written about it hold.
    let mut harness = Harness::new(card_fixture());
    harness.frame();
    let _ = harness.ink_of("#absent");
}

#[test]
fn an_idle_frame_emits_no_primitives_and_the_control_shows_the_counter_can_move() {
    let mut draw = true;
    let mut harness = Harness::new(Fixture::new(move |cx: &mut FrameCx<'_>| {
        if draw {
            let fill = PaintRef::solid(cx.scene().paints.solid(Color::BLACK));
            cx.scene().push_quad(Quad::filled(
                Rect::new(
                    Point::new(DevicePx(0.0), DevicePx(0.0)),
                    Size::new(DevicePx(8.0), DevicePx(8.0)),
                ),
                fill,
            ));
            draw = false;
        }
    }));

    let busy = harness.measure(|harness| harness.frame());
    let control = busy.control(Counter::PrimitivesEmitted);
    assert_eq!(control.value(), 1);

    let idle = harness.measure(|harness| harness.frame());
    idle.assert_zero(Counter::PrimitivesEmitted, &control);
}

#[test]
fn a_seven_hundred_millisecond_deadline_costs_one_resume_and_one_redraw() {
    let delay = Duration::from_millis(700);
    let mut armed = true;
    let mut harness = Harness::new(Fixture::new(move |cx: &mut FrameCx<'_>| {
        if armed {
            cx.wake_at(cx.now() + delay);
            armed = false;
        }
    }));

    harness.frame();
    assert_eq!(harness.parked_deadline(), Some(harness.now() + delay));

    harness.advance(Duration::from_millis(699));
    assert_eq!(harness.redraws_requested(), 0, "not reached yet");
    assert_eq!(harness.wakes(), 0, "parked, not spinning");

    harness.advance(Duration::from_millis(1));
    assert_eq!(
        harness.redraws_requested(),
        1,
        "the deadline itself asked for the frame"
    );
    assert_eq!(harness.resumes(), 1);
    assert_eq!(harness.wakes(), 1);

    harness.frame();
    assert!(
        harness.parked_deadline().is_none(),
        "an expired deadline is never re-installed"
    );
    assert_eq!(
        harness.frames_requested(),
        0,
        "nothing asked for a frame from inside one"
    );
    harness.assert_park_invariant();
}

#[test]
fn resumes_never_exceed_frames_plus_one_over_a_scripted_run() {
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        cx.wake_at(cx.now() + Duration::from_millis(16));
    }));
    for _ in 0..200 {
        harness.frame();
        harness.advance(Duration::from_millis(16));
    }
    assert_eq!(harness.resumes(), 200);
    assert!(harness.resumes() <= harness.frames() + 1);
    harness.assert_park_invariant();
}

#[test]
#[should_panic(expected = "expired deadlines against")]
fn the_park_invariant_fails_on_a_loop_whose_wake_edge_is_missing() {
    // The positive control for the invariant asserted on every frame above. With the edge missing,
    // the expired deadline stays installed and is reported reached on every turn: the loop looks
    // idle, ignores its own timers and burns a core, and only this ratio can see it.
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        cx.wake_at(cx.now());
    }))
    .with_park_model(ParkModel::MissingWakeEdge);

    harness.frame();
    for _ in 0..999 {
        harness.advance(Duration::from_millis(1));
    }
}

#[test]
fn four_in_frame_requesters_cost_exactly_one_redraw() {
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        for _ in 0..4 {
            cx.request_another_frame();
        }
    }));
    harness.frame();
    assert_eq!(harness.frames_requested(), 1);
    assert_eq!(harness.redraws_requested(), 1);
}

#[test]
fn an_occluded_surface_is_never_asked_for_another_frame() {
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        cx.request_another_frame();
    }));

    // The control: while visible, the same frame body does ask for another frame.
    assert!(!harness.is_occluded(), "the control runs visible");
    harness.frame();
    assert_eq!(harness.redraws_requested(), 1);

    harness.reset_counters();
    harness.set_occluded(true);
    assert!(
        harness.is_occluded(),
        "and the measured run is the same harness, hidden"
    );
    harness.frame();
    assert_eq!(
        harness.redraws_requested(),
        0,
        "honouring an in-frame request on a hidden surface is the full-rate spin"
    );
}

#[test]
fn a_resize_reconfigures_the_surface_the_frames_are_built_for() {
    let mut harness = Harness::new(card_fixture());
    harness.frame();
    assert!(harness.transcript().as_str().contains("viewport=800x600"));

    harness.resize(Size::new(320, 200));
    harness.frame();
    assert!(harness.transcript().as_str().contains("viewport=320x200"));
    assert_eq!(
        harness.renderer().target().map(|target| target.size),
        Some(Size::new(320, 200))
    );
}

#[test]
fn the_clock_only_moves_when_the_test_moves_it() {
    let mut harness = Harness::new(card_fixture());
    let start = harness.now();
    harness.frame();
    harness.frame();
    assert_eq!(harness.now(), start);

    harness.advance(Duration::from_millis(5));
    assert_eq!(harness.now(), start + Duration::from_millis(5));
}

#[test]
#[should_panic(expected = "expired deadlines against 0 frames")]
fn a_spin_after_a_counter_reset_is_still_a_spin() {
    // The invariant is a ratio, and a ratio needs both terms taken over the same window. With the
    // frames cumulative and the resumes reset, a run that had already done a thousand frames could
    // spin a thousand times afterwards and pay for it with work it did before the reset.
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        cx.wake_at(cx.now());
    }))
    .with_park_model(ParkModel::MissingWakeEdge);

    for _ in 0..8 {
        harness.frame();
    }
    harness.reset_counters();
    harness.advance(Duration::from_millis(1));
    harness.advance(Duration::from_millis(1));
}

#[test]
fn a_reset_does_not_make_a_correct_park_fail() {
    // The control for the test above: the tightened window must fail a spin without failing the
    // ordinary case of one deadline delivered before the frame it asks for has run.
    let mut harness = Harness::new(Fixture::new(|cx: &mut FrameCx<'_>| {
        cx.wake_at(cx.now() + Duration::from_millis(16));
    }));
    harness.frame();
    harness.reset_counters();
    harness.advance(Duration::from_millis(16));
    assert_eq!(harness.resumes(), 1);
    harness.frame();
    harness.assert_park_invariant();
}

#[test]
#[should_panic(expected = "left")]
fn a_bound_on_a_renderer_specific_counter_fails_through_the_snapshot_too() {
    // Refusing `draw_calls` by name closes only the door a test knocks on. Read off the snapshot
    // there is no name to refuse, and the zero a capture renderer leaves would satisfy every bound
    // a budget could write — for ever, while measuring nothing.
    let mut harness = Harness::new(card_fixture());
    harness.frame();
    assert!(
        harness.counters().draw_calls < 64,
        "left a renderer-specific counter assertable"
    );
}

#[test]
fn the_snapshot_poisons_only_what_no_capture_renderer_can_move() {
    // The control: the poisoning is narrow. Everything a backend-neutral stage produces still reads
    // its real value, or the accessor would be useless rather than merely safe.
    let mut harness = Harness::new(card_fixture());
    harness.frame();
    let counters = harness.counters();
    assert_eq!(counters.primitives_emitted, 1);
    for counter in zgui_testkit_scene::counters::RENDERER_SPECIFIC {
        assert_eq!(
            counters.get(counter),
            zgui_testkit_scene::counters::POISON,
            "{}",
            counter.name()
        );
    }
}
