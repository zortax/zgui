//! Frames that only present what the compositor's scrolling moved.

use compositor_types::{CompositorEvent, FrameOutcome, ScrollPhase, SkipReason};
use web_time::{Duration, Instant};
use zgui_bits::DamageSet;
use zgui_platform::Clock;
use zgui_profile::{Counter, counter};

use crate::compositor::Drawn;
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

    /// Builds and presents the compositor's next frame without running the window's pipeline.
    pub(crate) fn tick_compositor(&mut self, clock: &dyn Clock) -> FrameReport {
        let started = Instant::now();
        let now = clock.now();
        self.driven.tick_owed.set(false);
        self.driven.due = None;
        counter::bump(Counter::CompositorTicks);
        let outcome = if self.occluded || self.starved {
            FrameOutcome::Skipped(SkipReason::Occluded)
        } else {
            self.draw_output(clock.timestamp()).outcome
        };
        let cost = clock.now().saturating_duration_since(now);
        let interval = self.refresh_interval();
        self.present
            .observed(self.renderer.acquire_block(), cost, interval);
        self.settle_retry(outcome, now);
        self.show_first_frame(outcome);
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

    /// Takes the compositor's events, keeps the newest offset of every node, and asks for the
    /// frames that `drawn` and those offsets owe. Reports whether any node moved.
    ///
    /// The window is asked for a frame when a node comes to rest, or when a node is about to show
    /// content the window has not recorded: its offset, or the offset two more steps of its
    /// motion reach. While the compositor moves anything, each built frame asks for the next one,
    /// until one finds nothing to build.
    pub(crate) fn take_scroll_events(&mut self, drawn: &Drawn) -> bool {
        let mut events = std::mem::take(&mut self.driven.events);
        self.compositor.take_events(&mut events);
        let mut wake = false;
        let mut scrolled = false;
        for event in events.drain(..) {
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
            let active = self.driven.active.entry(node).or_default();
            let step = active
                .last
                .map_or([0.0; 2], |last| [offset[0] - last[0], offset[1] - last[1]]);
            active.step = step;
            active.last = Some(offset);
            let ahead = [offset[0] + 2.0 * step[0], offset[1] + 2.0 * step[1]];
            wake |= settled || !active.covers(offset) || !active.covers(ahead);
        }
        self.driven.events = events;
        if wake {
            self.request_frame();
        }
        // A frame that found nothing to build and no change to come ends the motion.
        self.driven.moving &= drawn.built || drawn.wake_ns.is_some();
        let interval = self.refresh_interval();
        self.driven.due = if self.driven.moving && drawn.built {
            Some(self.clock.now() + interval)
        } else {
            drawn
                .wake_ns
                .map(|wake| self.clock.origin() + Duration::from_nanos(wake))
        };
        scrolled
    }
}
