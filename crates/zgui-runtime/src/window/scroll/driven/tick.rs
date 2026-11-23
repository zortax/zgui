//! Frames that only present what the compositor's scrolling moved.

use compositor_api::WatchCause;
use compositor_types::{CompositorEvent, FrameOutcome, ScrollPhase, SkipReason};
use web_time::{Duration, Instant};
use zgui_bits::DamageSet;
use zgui_platform::Clock;
use zgui_profile::{Counter, counter};

use crate::compositor::Answers;
use crate::window::Window;
use crate::window::frame::FrameReport;
use crate::window::scroll::driven::Shown;

impl Window {
    /// Asks for a frame that presents the compositor's scrolling, unless a frame was asked for.
    pub(crate) fn request_compositor_frame(&self) {
        if !self.awaiting_frame.get() && !self.driven.tick_owed.replace(true) {
            zgui_profile::latency::mark("req.tick");
            self.surface.request_redraw();
        }
    }

    /// Reports whether the frame asked for only presents the compositor's scrolling.
    ///
    /// Any other work owed, such as input, a damaged region or a reconfiguration, makes the frame
    /// a whole one.
    pub(crate) fn only_compositor_owed(&self) -> bool {
        self.driven.tick_owed.get()
            && !self.awaiting_frame.get()
            && self.queued.is_empty()
            && !self.reconfigure
            && self.damage.is_empty()
    }

    /// Runs the compositor's frames that are due and takes what the driver answered, without
    /// running the window's pipeline.
    pub(crate) fn tick_compositor(&mut self, clock: &dyn Clock) -> FrameReport {
        let started = Instant::now();
        self.driven.tick_owed.set(false);
        self.driven.due = None;
        counter::bump(Counter::CompositorTicks);
        let outcome = if self.occluded || self.starved {
            FrameOutcome::Skipped(SkipReason::Occluded)
        } else {
            self.draw_output(clock.timestamp()).outcome
        };
        let report = FrameReport {
            elapsed: started.elapsed(),
            paint_elapsed: Duration::ZERO,
            outcome,
            painted: DamageSet::new(),
            needs_another_frame: false,
            restyled: 0,
            timers_fired: 0,
            animated: 0,
        };
        self.last_report = Some(report);
        if let Some(probe) = self.probe.clone() {
            probe.frame_ended(self);
        }
        report
    }

    /// Keeps the newest offset of every node the compositor's scrolling moved, acts on the
    /// gestures it routed otherwise than predicted, and asks for the frames the watches that fired
    /// owe. Reports whether any node moved.
    ///
    /// The window is asked for a frame when a watch fires, see
    /// [`Window::watch_motion`]: a node comes to rest, is about to show content the window has
    /// not recorded, or leaves the offsets its rows hold. A move that changes what a `:hover` rule
    /// matches under the pointer asks for one too.
    pub(crate) fn take_scroll_events(&mut self, answers: &mut Answers) -> bool {
        let mut scrolled = false;
        for event in answers.compositor.drain(..) {
            let CompositorEvent::Scroll {
                node,
                offset,
                serial,
                phase,
                ..
            } = event
            else {
                continue;
            };
            let settled = phase == ScrollPhase::Settled;
            scrolled = true;
            self.driven.shown.insert(
                node,
                Shown {
                    offset,
                    serial,
                    settled,
                },
            );
            // Queued events of a node coalesce, so the step is the motion since the offset the
            // window last saw, which is one driver frame on a driver that runs on this thread.
            let active = self.driven.active.entry(node).or_default();
            if !settled {
                active.step = active
                    .last
                    .map_or([0.0; 2], |last| [offset[0] - last[0], offset[1] - last[1]]);
            }
            active.last = Some(offset);
        }
        for (gesture, routing) in std::mem::take(&mut answers.rerouted) {
            self.rerouted(gesture, routing);
        }
        let wake = !answers.watched.is_empty();
        for &(node, cause, _, step) in &answers.watched {
            if cause == WatchCause::Settled {
                self.driven
                    .shown
                    .entry(node)
                    .and_modify(|shown| shown.settled = true);
            } else if let Some(active) = self.driven.active.get_mut(&node) {
                active.step = step;
            }
        }
        if wake {
            self.request_frame();
        }
        if scrolled {
            self.wake_for_hover();
        }
        scrolled
    }
}
