//! A window's frame recorded into its compositor.

use zgui_bits::DamageSet;
use zgui_paint::CompositorSync;

use crate::window::Window;

impl Window {
    /// Opens the frame's transaction.
    ///
    /// The first call attaches a sync to the painter, so the walk records every fragment it
    /// visits. After [`Window::forget_compositor_sync`], the client is registered again with
    /// nothing and the whole surface is damaged first.
    pub(super) fn open_transaction(&mut self) {
        if self.resync.take() {
            self.compositor.restart();
            self.driven.forget();
            if let Some(sync) = self.painter.compositor() {
                sync.reset();
            }
            self.damage = DamageSet::full();
        }
        let transaction = self.compositor.transaction();
        if self.painter.compositor().is_none() {
            let mut sync = CompositorSync::new(self.compositor.output());
            // A shared parameter block that changes asks this window for the frame that draws it.
            let waker = std::sync::Arc::clone(&self.waker);
            sync.set_wake(std::sync::Arc::new(move || {
                zgui_reactive::FrameWaker::wake(&*waker)
            }));
            self.painter.attach_compositor(sync);
        }
        self.sync_scroll_drive();
        if let Some(sync) = self.painter.compositor() {
            sync.open(transaction);
        }
    }

    /// Runs the frames of the output that are due on the window's thread for the moment `at`, and
    /// takes what the driver answered since the last call.
    ///
    /// A driver with a thread of its own builds and draws on its own, and the answers are what it
    /// did so far. The outcome is the last frame drawn for the output, or that the window's work
    /// was handed over when none was drawn yet.
    pub(super) fn draw_output(&mut self, at: zgui_vocab::Timestamp) -> crate::compositor::Drawn {
        let interval = self.refresh_interval();
        self.hand_lost_steps();
        self.compositor.run_due(compositor_types::FrameClock {
            now_ns: nanos(at.since_origin()),
            present_ns: nanos(at.since_origin()),
            interval_ns: nanos(interval),
        });
        self.take_answers()
    }

    /// Returns the moment of the window's clock that `ns` on the driver's frame clock is.
    ///
    /// A stepped driver's frame clock is the window's own clock, which the window steps it with.
    pub(crate) fn frame_clock_moment(&self, ns: u64) -> web_time::Instant {
        if self.compositor.pacing() == compositor_api::DriverPacing::Stepped {
            return self.clock.origin() + web_time::Duration::from_nanos(ns);
        }
        let now = compositor_api::now_ns();
        let ahead = web_time::Duration::from_nanos(ns.saturating_sub(now));
        let behind = web_time::Duration::from_nanos(now.saturating_sub(ns));
        self.clock.now() + ahead - behind.min(self.clock.now() - self.clock.origin())
    }

    /// Returns when the frame started at `at` is predicted to be shown, which animations are
    /// sampled at.
    ///
    /// A stepped driver shows the frame at the clock the window steps it with, which is `at`. A
    /// driver that paces itself shows it at its next display time, and at `at` when it predicts
    /// none.
    pub(crate) fn shown_at(&self, at: zgui_vocab::Timestamp) -> zgui_vocab::Timestamp {
        if self.compositor.pacing() == compositor_api::DriverPacing::Stepped {
            return at;
        }
        let Some(display) = self.compositor.stats().next_display_ns else {
            return at;
        };
        at + web_time::Duration::from_nanos(display.saturating_sub(compositor_api::now_ns()))
    }

    /// Takes what the driver answered since the last call and acts on it.
    ///
    /// Drawn frames feed the counters and the window's first show, a rejected commit makes the
    /// next frame record everything again, and the compositor's scrolling is taken for the next
    /// frame to adopt.
    pub(crate) fn take_answers(&mut self) -> crate::compositor::Drawn {
        let mut answers = self.compositor.answers();
        if answers.rejected.is_some() {
            self.restart_after_rejection();
        }
        for report in &answers.committed {
            crate::window::counters::committed(report);
        }
        let drawn = crate::compositor::Drawn::of(&answers);
        if !answers.drawn.is_empty() {
            self.steps_built();
            crate::window::counters::held(&self.compositor);
        }
        for (counters, outcome) in &answers.drawn {
            crate::window::counters::built(counters);
            self.renderer_totals.drawn(outcome, &self.compositor);
            self.show_first_frame(*outcome);
            if let Some(containment) = &self.containment {
                containment.drawn(outcome.retires_damage());
            }
        }
        if let Some(containment) = &self.containment {
            containment.assert_contained();
        }
        if answers.shown {
            self.show_first_frame(compositor_types::FrameOutcome::Presented(
                compositor_types::FrameStats::default(),
            ));
        }
        if let Some((starved, _occluded)) = answers.presenter {
            self.starved = starved;
        }
        if let Some(error) = &answers.failed {
            tracing::error!(target: "zgui::compositor", ?error, "the window's renderer failed");
        }
        if self.check_damage {
            crate::window::counters::check_declared(&answers.compositor);
        }
        self.take_scroll_events(&mut answers);
        self.arm_driver();
        drawn
    }

    /// Makes the driver's next frame a deadline of the window, unless the driver paces itself.
    fn arm_driver(&mut self) {
        let due = self.compositor.next_due();
        let paced = self.compositor.pacing() == compositor_api::DriverPacing::SelfPaced;
        let at = due
            .filter(|_| !paced)
            .map(|due| self.frame_clock_moment(due));
        self.driven.arm(at);
    }

    /// Records everything again from the next frame on, after the driver rejected a commit.
    fn restart_after_rejection(&mut self) {
        self.compositor.restart();
        if let Some(sync) = self.painter.compositor() {
            sync.reset();
        }
        self.driven.forget();
        self.damage = DamageSet::full();
        self.request_frame();
    }

    /// Makes the next frame drop everything this window recorded into its compositor and record
    /// the whole surface again from nothing.
    ///
    /// No frame is asked for.
    pub fn forget_compositor_sync(&self) {
        self.resync.set(true);
    }

    /// Commits the transaction the embed host wrote into.
    ///
    /// A rejection is answered later, as a rejected frame is.
    pub(super) fn submit_embed_transaction(&mut self, transaction: compositor_types::Transaction) {
        self.compositor.commit(transaction);
    }

    /// Commits the frame's transaction.
    ///
    /// A rejected transaction is answered later and leaves the compositor with nothing of this
    /// window, so the frame after the answer records the whole surface again.
    ///
    /// A transaction that writes no node, slot or parameter block declares the damage the frame's
    /// paint walk drew, which holds every pixel the walk's recordings and offsets change. With
    /// invariants checked, a declaration the compositor's verification finds short is a defect.
    pub(super) fn submit_transaction(&mut self, mut transaction: compositor_types::Transaction) {
        let report = self.painter.compositor().map(|sync| sync.report());
        // Table writes, such as a transform or a colour an animation moves, change pixels the
        // walk's damage does not hold.
        let declares = report.is_some_and(|report| {
            report.table_writes == 0 && report.params_written == 0 && report.released == 0
        });
        if declares {
            transaction.declare_damage(
                self.compositor.output(),
                crate::window::declared::declared(&self.painted),
            );
        }
        if let Some(report) = &report {
            crate::window::counters::synced(report);
            tracing::debug!(target: "zgui::compositor", ?report, "transaction committed");
        }
        if self.check_damage {
            // Property, paint and offset writes, and the compositor's own scrolling, redraw
            // pixels the walk never visited.
            let fit = report.is_some_and(|report| {
                report.table_writes == 0 && report.params_written == 0 && report.offsets == 0
            }) && !self.compositor_scrolling();
            let compositor = &self.compositor;
            self.containment
                .get_or_insert_with(|| crate::window::containment::Containment::observe(compositor))
                .committed(&self.painted, fit);
        }
        self.compositor.commit(transaction);
    }
}

/// Returns `duration` in whole nanoseconds, saturating.
fn nanos(duration: web_time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
