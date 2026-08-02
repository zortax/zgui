//! What a frame is handed, and what it may ask for.

use std::time::Instant;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Rect, Size};
use zgui_scene::Scene;

use crate::harness::subject::Subjects;

/// What one frame asked the loop for after it had run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Requests {
    /// The earliest moment anything asked to be woken at.
    pub deadline: Option<Instant>,
    /// Whether anything asked for another frame from inside this one.
    pub another_frame: bool,
}

impl Requests {
    /// Merges a deadline in, keeping the earlier of the two.
    ///
    /// Several things want a deadline in one frame — an animation, a timer, a delayed tooltip — and
    /// the loop parks on one. Keeping the earliest is the whole of the merge.
    pub fn wake_at(&mut self, deadline: Instant) {
        self.deadline = Some(match self.deadline {
            Some(held) => held.min(deadline),
            None => deadline,
        });
    }
}

/// Everything one frame of the pipeline is given.
///
/// A frame reads the clock through this rather than calling the system, builds into the scene it is
/// handed, absorbs what it changed into the damage set, names what it drew, and says what would
/// make it want another frame.
pub struct FrameCx<'a> {
    /// The present moment, fixed for the whole frame.
    now: Instant,
    /// The surface extent this frame is being built for.
    viewport: Size<i32, Device>,
    /// The display list being built.
    scene: &'a mut Scene,
    /// What must be redrawn.
    damage: &'a mut DamageSet,
    /// What the frame drew, by name.
    subjects: &'a mut Subjects,
    /// What the frame is asking the loop for.
    requests: &'a mut Requests,
}

impl<'a> FrameCx<'a> {
    /// Builds the context for one frame.
    pub(crate) fn new(
        now: Instant,
        viewport: Size<i32, Device>,
        scene: &'a mut Scene,
        damage: &'a mut DamageSet,
        subjects: &'a mut Subjects,
        requests: &'a mut Requests,
    ) -> Self {
        Self {
            now,
            viewport,
            scene,
            damage,
            subjects,
            requests,
        }
    }

    /// The present moment.
    pub fn now(&self) -> Instant {
        self.now
    }

    /// The surface extent this frame is for.
    pub fn viewport(&self) -> Size<i32, Device> {
        self.viewport
    }

    /// The display list being built.
    pub fn scene(&mut self) -> &mut Scene {
        self.scene
    }

    /// What must be redrawn.
    pub fn damage(&mut self) -> &mut DamageSet {
        self.damage
    }

    /// Adds `rect` to what must be redrawn.
    pub fn damage_rect(&mut self, rect: Rect<i32, Device>) {
        self.damage.absorb(rect);
    }

    /// Records that this frame drew `name`, covering `ink`.
    pub fn record_subject(&mut self, name: &str, ink: Rect<i32, Device>) {
        self.subjects.record(name, ink);
    }

    /// Asks the loop to park no later than `deadline`.
    ///
    /// This is what an animation and a timer both do, and it is the only input to the parked
    /// deadline: a frame that wants to run again *later* says so here, and a frame that wants to run
    /// again *now* calls [`FrameCx::request_another_frame`].
    pub fn wake_at(&mut self, deadline: Instant) {
        self.requests.wake_at(deadline);
    }

    /// Asks for one more frame, from inside this one.
    ///
    /// Every in-frame requester — a mutation, an input dispatch, a wake from another thread — sets
    /// this flag rather than asking the surface directly, and the frame's last phase converts the
    /// flag into exactly one redraw request. Four requesters in one frame therefore cost one frame,
    /// not four.
    pub fn request_another_frame(&mut self) {
        self.requests.another_frame = true;
    }
}

/// The part of the frame the harness does not own.
///
/// This is the seam the frame loop is assembled through: the harness owns the clock, the parking,
/// the damage set, the display list and the counters, and whatever builds a frame from a document
/// plugs in here. A test supplies a closure; a real engine supplies itself.
pub trait Pipeline {
    /// Builds one frame.
    fn build_frame(&mut self, cx: &mut FrameCx<'_>);
}

impl<F: FnMut(&mut FrameCx<'_>)> Pipeline for F {
    fn build_frame(&mut self, cx: &mut FrameCx<'_>) {
        self(cx);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::Requests;

    #[test]
    fn merging_deadlines_keeps_the_earliest() {
        let now = Instant::now();
        let mut requests = Requests::default();
        requests.wake_at(now + Duration::from_millis(700));
        requests.wake_at(now + Duration::from_millis(16));
        requests.wake_at(now + Duration::from_millis(300));
        assert_eq!(requests.deadline, Some(now + Duration::from_millis(16)));
    }
}
