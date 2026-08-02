//! Reading a harness: what the last frame produced, and how the loop is parked.
//!
//! Everything here is a read of state the frame loop already keeps. It is a module of its own so
//! that [`mod@crate::harness`] stays the frame sequence and nothing else — the sequence is the part
//! with rules in it, and it should not be read past twenty accessors to find.

use std::time::Instant;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Rect, Size};
use zgui_platform::Clock;
use zgui_profile::{Counter, Counters, counter};
use zgui_render::Renderer;
use zgui_scene::Scene;

use crate::capture::CaptureRenderer;
use crate::harness::Harness;
use crate::harness::subject::Subject;
use crate::transcript::Transcript;

impl Harness {
    /// The present moment.
    pub fn now(&self) -> Instant {
        self.clock.now()
    }

    /// The moment the loop is parked until, if it is parked on a deadline at all.
    pub fn parked_deadline(&self) -> Option<Instant> {
        self.park.deadline()
    }

    /// How many redraw requests have reached the surface.
    pub fn redraws_requested(&self) -> u64 {
        self.park.redraws_requested()
    }

    /// How many of those came from a frame owing another frame.
    pub fn frames_requested(&self) -> u64 {
        self.park.frames_requested()
    }

    /// How many times a parked deadline has been reported reached.
    pub fn resumes(&self) -> u64 {
        self.park.resumes()
    }

    /// How many frames have run.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// How many times the loop was woken from a wait.
    pub fn wakes(&self) -> u64 {
        counter::get(Counter::Wakes)
    }

    /// Whether the surface is hidden.
    pub fn is_occluded(&self) -> bool {
        self.occluded
    }

    /// Hides or reveals the surface.
    pub fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
    }

    /// Every counter's value since the last reset, with the renderer-specific ones poisoned.
    ///
    /// This is the read without a control. Prefer [`Harness::measure`], whose assertions cannot be
    /// written without a control that proves the counter can move at all.
    ///
    /// A counter only a graphics backend increments reads
    /// [`POISON`](crate::counters::meaning::POISON) here rather than the zero it holds, because
    /// nothing here drew: a bound on one would otherwise hold for ever while measuring nothing.
    pub fn counters(&self) -> Counters {
        crate::counters::meaning::poisoned(counter::snapshot())
    }

    /// What the last frame reported as needing redrawing.
    ///
    /// Read from the damage set the pipeline produced, never from the renderer: a capture renderer
    /// redraws nothing, so asking it what it redrew would answer nothing every time.
    pub fn damage_rects(&self) -> &[Rect<i32, Device>] {
        self.damage.rects()
    }

    /// The whole damage set the last frame produced.
    pub fn damage(&self) -> &DamageSet {
        &self.damage
    }

    /// The subject the last frame drew under `name`.
    pub fn query(&self, name: &str) -> Option<&Subject> {
        self.subjects.get(name)
    }

    /// Where `name`'s ink landed in the last frame.
    ///
    /// # Panics
    ///
    /// Panics when the last frame drew no such subject, naming what it did draw. An absent subject
    /// silently reported as an empty rectangle would make every containment assertion about it
    /// hold.
    pub fn ink_of(&self, name: &str) -> Rect<i32, Device> {
        match self.subjects.get(name) {
            Some(subject) => subject.ink,
            None => panic!(
                "the last frame drew no subject called `{name}`; it drew: {:?}",
                self.subjects
                    .all()
                    .iter()
                    .map(|subject| subject.name.as_str())
                    .collect::<Vec<_>>()
            ),
        }
    }

    /// Every subject the last frame drew.
    pub fn subjects(&self) -> &[Subject] {
        self.subjects.all()
    }

    /// The last frame's transcript.
    ///
    /// # Panics
    ///
    /// Panics before the first frame has run.
    pub fn transcript(&self) -> &Transcript {
        self.renderer
            .transcript()
            .expect("no frame has been drawn yet; call frame() first")
    }

    /// The display list the last frame built.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The renderer, for a test that wants to ask it something directly.
    pub fn renderer(&self) -> &CaptureRenderer {
        &self.renderer
    }

    /// The clock, as the platform contract sees it.
    pub fn clock(&self) -> &dyn Clock {
        &self.clock
    }

    /// The surface extent frames are built for.
    pub fn viewport(&self) -> Size<i32, Device> {
        self.viewport
    }

    /// Points the harness at a surface of a different extent.
    ///
    /// The renderer is reconfigured, exactly as a real one is on a resize.
    pub fn resize(&mut self, viewport: Size<i32, Device>) {
        self.viewport = viewport;
        self.renderer.configure(zgui_render::RenderTarget::new(
            viewport,
            zgui_geom::Scale::new(1.0),
        ));
    }
}
