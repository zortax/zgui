//! The instrument every budget test is written against.
//!
//! A harness is a frame loop with a clock a test moves by hand, a renderer that records instead of
//! drawing, and exclusive use of the frame counters. It answers three kinds of question, and the
//! third is the one nothing else can:
//!
//! * **what a frame did** — the counters, through [`Harness::measure`] and its controls;
//! * **what a frame drew** — the transcript, the damage set and the named subjects' ink;
//! * **how the loop parked** — [`Harness::parked_deadline`], [`Harness::redraws_requested`],
//!   [`Harness::frames_requested`] and [`Harness::resumes`], which are what separate a loop waiting
//!   correctly from one spinning on an expired deadline while running no frames at all.
//!
//! # What a frame is, and what it is not yet
//!
//! One frame is: build the display list through the [`Pipeline`] seam, finish the scene against the
//! frame's damage, draw it into the capture renderer, then park. Restyle, layout, fragments and
//! accessibility plug into the same loop through that one seam, and nothing the harness itself owns
//! — the clock, the parking, the counter access, the fixture loader and the damage set — changes
//! when they do.
//!
//! **Every stage runs on the calling thread.** That is not an accident of the frame body being a
//! closure: a restyle traversal driven across a worker pool would make a count of the styles
//! it lowered a property of the runner's core count rather than of the design, and a budget written
//! against it would pass on one machine and fail on another. Whatever plugs in here is driven
//! sequentially for the same reason.

pub mod accessors;
pub mod frame;
pub mod park;
pub mod subject;

use std::time::Duration;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Size};
use zgui_platform::{Clock, VirtualClock};
use zgui_profile::counter;
use zgui_render::{FrameOutcome, Renderer, SkipReason};
use zgui_scene::Scene;

use crate::capture::CaptureRenderer;
use crate::counters::{Measurement, Recording};
use crate::fixture::Fixture;
use crate::harness::frame::{FrameCx, Pipeline, Requests};
use crate::harness::park::{Park, ParkModel};
use crate::harness::subject::Subjects;

/// A frame loop under a test's control.
///
/// ```
/// use std::time::Duration;
/// use zgui_color::Color;
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scene::{PaintRef, Quad};
/// use zgui_testkit_scene::{Fixture, Harness};
///
/// let mut harness = Harness::new(Fixture::new(|cx: &mut zgui_testkit_scene::FrameCx<'_>| {
///     let ink = Rect::new(Point::new(0, 0), Size::new(64, 24));
///     let fill = PaintRef::solid(cx.scene().paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
///     cx.scene().push_quad(Quad::filled(
///         Rect::new(
///             Point::new(DevicePx(0.0), DevicePx(0.0)),
///             Size::new(DevicePx(64.0), DevicePx(24.0)),
///         ),
///         fill,
///     ));
///     cx.damage_rect(ink);
///     cx.record_subject("#card", ink);
/// }));
///
/// harness.frame();
/// assert_eq!(harness.ink_of("#card").size.width, 64);
/// assert!(harness.damage_rects().iter().any(|rect| rect.contains_rect(harness.ink_of("#card"))));
/// assert!(harness.transcript().to_string().contains("quad order=1"));
/// ```
pub struct Harness {
    /// What builds each frame.
    pipeline: Box<dyn Pipeline>,
    /// The clock every phase reads.
    clock: VirtualClock,
    /// The display list, kept across frames because its side tables are.
    scene: Scene,
    /// The renderer, which records rather than drawing.
    renderer: CaptureRenderer,
    /// What the last frame produced.
    damage: DamageSet,
    /// What the last frame named.
    subjects: Subjects,
    /// How the loop is parked.
    park: Park,
    /// The surface extent frames are built for.
    viewport: Size<i32, Device>,
    /// How many frames have run.
    frames: u64,
    /// What [`Harness::frames`] read when the parking counts were last set back to zero.
    ///
    /// The park invariant is a ratio, so both of its terms have to be counted over the same window.
    /// Resetting the resumes while leaving the frames cumulative would let a run that had already
    /// performed a thousand frames spin a thousand times afterwards without the ratio noticing.
    frames_at_reset: u64,
    /// Whether the surface is hidden.
    occluded: bool,
    /// Exclusive use of the counters, released when the harness is dropped.
    _recording: Recording,
}

impl Harness {
    /// A harness over `fixture`, with nothing run yet.
    ///
    /// It takes exclusive use of the frame counters for its whole life, so two harnesses cannot
    /// measure at once and a second one on the same thread is a panic rather than a deadlock.
    pub fn new(fixture: Fixture) -> Self {
        let recording = Recording::begin();
        let mut renderer = CaptureRenderer::new();
        renderer.configure(zgui_render::RenderTarget::new(
            fixture.viewport,
            zgui_geom::Scale::new(1.0),
        ));
        Self {
            pipeline: fixture.pipeline,
            clock: VirtualClock::new(),
            scene: Scene::new(),
            renderer,
            damage: DamageSet::new(),
            subjects: Subjects::new(),
            park: Park::new(ParkModel::default()),
            viewport: fixture.viewport,
            frames: 0,
            frames_at_reset: 0,
            occluded: false,
            _recording: recording,
        }
    }

    /// The same harness modelling a different reading of the park.
    ///
    /// Only a test of the park itself needs this. The defective reading exists so that
    /// [`Harness::assert_park_invariant`] can be shown to fail.
    pub fn with_park_model(mut self, model: ParkModel) -> Self {
        self.park = Park::new(model);
        self
    }

    /// Runs one frame.
    ///
    /// The sequence is the frame loop's, as far as the crates that exist allow: build the display
    /// list, finish it against this frame's damage, draw it, then park. The last step is the one
    /// with a rule that is easy to lose — every in-frame request for another frame becomes exactly
    /// one redraw request, and an occluded surface gets none at all.
    pub fn frame(&mut self) {
        self.frames += 1;
        let now = self.clock.now();

        self.scene.begin_frame(self.viewport);
        self.damage = DamageSet::for_frame();
        self.subjects.clear();
        let mut requests = Requests::default();

        {
            let mut cx = FrameCx::new(
                now,
                self.viewport,
                &mut self.scene,
                &mut self.damage,
                &mut self.subjects,
                &mut requests,
            );
            self.pipeline.build_frame(&mut cx);
        }

        self.scene.finish(&self.damage);
        let outcome = if self.occluded {
            // A hidden surface is not drawn to at all, and the loop must not ask for another frame
            // on its behalf: honouring an in-frame request there is precisely the full-rate spin an
            // invisible window must not perform.
            FrameOutcome::Skipped(SkipReason::Occluded)
        } else {
            self.renderer.draw(&self.scene, &self.damage)
        };

        if requests.another_frame && outcome != FrameOutcome::Skipped(SkipReason::Occluded) {
            self.park.request_another_frame();
        }
        self.park.install(requests.deadline, now);
        self.assert_park_invariant();
    }

    /// Moves the clock, taking the deadline-expiry edge if that crosses the parked deadline.
    ///
    /// This routes through the same edge the real loop takes rather than only moving the clock, so
    /// a test can tell "the deadline woke us" from "the test called [`Harness::frame`]" — which is
    /// the distinction the whole parking design turns on.
    pub fn advance(&mut self, by: Duration) {
        self.clock.advance(by);
        self.park.expire(self.clock.now());
        self.assert_park_invariant();
    }

    /// Asserts that the loop is parking rather than spinning.
    ///
    /// Run after every frame and every advance, so a regression is a failure at the point it
    /// happens rather than a profiler finding weeks later.
    ///
    /// Both terms are counted since [`Harness::reset_counters`], because a ratio whose numerator is
    /// reset and whose denominator is not stops being a ratio: the frames of everything that came
    /// before would pay for a spin that came after.
    ///
    /// # Panics
    ///
    /// Panics when more deadlines have been reported reached than frames have run, plus one.
    pub fn assert_park_invariant(&self) {
        self.park
            .assert_invariant(self.frames - self.frames_at_reset);
    }

    /// Sets every counter back to zero, and forgets the loop's own counts.
    ///
    /// This also starts the park invariant's window again, so what it measures afterwards is what
    /// happened afterwards.
    pub fn reset_counters(&mut self) {
        counter::reset();
        self.park.reset_counts();
        self.frames_at_reset = self.frames;
    }

    /// Resets the counters, runs `exercise`, and reports what it moved.
    pub fn measure(&mut self, exercise: impl FnOnce(&mut Self)) -> Measurement {
        self.reset_counters();
        exercise(self);
        Measurement::new(counter::snapshot())
    }
}
