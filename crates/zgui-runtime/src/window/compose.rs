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
        self.tell_refresh();
        self.hand_lost_steps();
        self.compositor.run_due(compositor_types::FrameClock {
            now_ns: nanos(at.since_origin()),
            present_ns: nanos(at.since_origin()),
            interval_ns: nanos(interval),
        });
        self.take_answers()
    }

    /// Tells the driver the refresh rate the platform reports for the window's output, when it
    /// changed.
    fn tell_refresh(&mut self) {
        let rate = self
            .surface
            .refresh_rate_millihertz()
            .filter(|rate| *rate > 0);
        if self.told_rate.replace(rate) != Some(rate) {
            self.compositor.set_refresh(refresh_ns(rate));
        }
    }

    /// Reports to the driver when a frame last reached the display, if the platform said so since
    /// the last report.
    fn report_presented(&mut self) {
        let Some(at) = self
            .surface
            .presentation_timing()
            .and_then(|timing| timing.last_presented)
        else {
            return;
        };
        if self.reported_presented.replace(at) == Some(at) {
            return;
        }
        self.compositor.report_presented(self.frame_clock_ns(at));
    }

    /// Returns where the moment `at` of the window's clock falls on the driver's frame clock.
    fn frame_clock_ns(&self, at: web_time::Instant) -> u64 {
        if self.compositor.pacing() == compositor_api::DriverPacing::Stepped {
            let since = at.saturating_duration_since(self.clock.origin());
            return u64::try_from(since.as_nanos()).unwrap_or(u64::MAX);
        }
        let ago = self.clock.now().saturating_duration_since(at);
        compositor_api::now_ns().saturating_sub(u64::try_from(ago.as_nanos()).unwrap_or(u64::MAX))
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
    /// driver that paces itself shows it when the frame a commit made now goes into is shown, and
    /// at `at` when it predicts none.
    pub(crate) fn shown_at(&self, at: zgui_vocab::Timestamp) -> zgui_vocab::Timestamp {
        if self.compositor.pacing() == compositor_api::DriverPacing::Stepped {
            return at;
        }
        let Some((_, display)) = self.compositor.commit_frame(self.refresh_interval()) else {
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
        self.report_presented();
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
        if let Some(error) = answers.failed.take() {
            if self.compositor.shown() {
                tracing::error!(target: "zgui::compositor", ?error, "the window's renderer failed");
            } else {
                self.unopened = Some(error);
            }
        }
        if self.check_damage {
            crate::window::counters::check_declared(&answers.compositor);
        }
        self.take_scroll_events(&mut answers);
        self.arm_driver();
        drawn
    }

    /// Waits for the output's renderer once, and then applies what the window reads of the device:
    /// the largest image it decodes and the coverage its text is drawn with.
    pub(crate) fn await_renderer(&mut self) {
        if self.renderer_known {
            return;
        }
        self.compositor.await_renderer();
        self.renderer_known = true;
        let Some(status) = self.compositor.status() else {
            return;
        };
        // A photo the device can hold is decoded whole rather than clamped to the smallest
        // supported device's limit.
        self.images
            .set_max_dimension(status.caps.max_texture_size.saturating_sub(2).max(1));
        self.settle_text_coverage();
    }

    /// Takes why the window's renderer could not be built, if the window never showed a frame.
    pub(crate) const fn take_unopened(&mut self) -> Option<compositor_types::GpuUnavailable> {
        self.unopened.take()
    }

    /// Takes the driver's answers that woke the window between its frames.
    ///
    /// The answers ask for the frames they need, and a change of the presenter's state asks for
    /// one too.
    pub(crate) fn take_woken_answers(&mut self) {
        let starved = self.starved;
        self.take_answers();
        if self.starved != starved {
            self.request_frame();
        }
    }

    /// Asks for a frame that runs the driver's frames, if the driver's newest snapshot still has
    /// one due by `now`.
    ///
    /// Otherwise the driver's next frame becomes the window's deadline again.
    pub(crate) fn compositor_due(&mut self, now: web_time::Instant) {
        self.arm_driver();
        if self.driven.due().is_some_and(|due| due <= now) {
            self.request_compositor_frame();
        }
    }

    /// Makes the driver's next frame a deadline of the window, unless the driver paces itself.
    pub(crate) fn arm_driver(&mut self) {
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
    /// A transaction that writes no node, slot or parameter block, and changes no chunk that draws
    /// glyphs, declares the damage the frame's paint walk drew, which holds every pixel the walk's
    /// recordings and offsets change. With invariants checked, a declaration the compositor's
    /// verification finds short is a defect.
    pub(super) fn submit_transaction(&mut self, mut transaction: compositor_types::Transaction) {
        let report = self.painter.compositor().map(|sync| sync.report());
        // Table writes, such as a transform or a colour an animation moves, change pixels the
        // walk's damage does not hold, and so do hinted glyphs, whose bitmaps only the
        // compositor's rasteriser places.
        let declares = report.is_some_and(|report| {
            report.table_writes == 0
                && report.params_written == 0
                && report.released == 0
                && report.glyphs_changed.is_none()
        });
        if declares {
            transaction.declare_damage(
                self.compositor.output(),
                crate::window::declared::declared(&self.painted, &self.recorded_ink),
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
            let mut visited = crate::window::declared::visited(&self.painted, &self.recorded_ink);
            if let Some(embolden) = report.and_then(|report| report.glyphs_changed) {
                visited = crate::window::declared::grown(&visited, glyph_allowance(embolden));
            }
            self.containment
                .get_or_insert_with(|| crate::window::containment::Containment::observe(compositor))
                .committed(&visited, fit);
        }
        self.compositor.commit(transaction);
    }
}

/// Returns how far, in device pixels, the bitmaps of glyph runs emboldened by at most `embolden`
/// device pixels can reach past the ink the walk measures for them.
fn glyph_allowance(embolden: f32) -> i32 {
    // ink's sweep of the reference rasteriser: hinting moves a bitmap's edge by up to 3 rows,
    // emboldening adds its width on each side, and a stroke's spike at a sharp corner one more.
    3 + embolden.max(0.0).ceil() as i32 + 1
}

/// Returns the refresh interval in nanoseconds of a rate in thousandths of a hertz.
pub(super) fn refresh_ns(millihertz: Option<u32>) -> Option<u64> {
    millihertz.map(|rate| 1_000_000_000_000 / u64::from(rate))
}

/// Returns `duration` in whole nanoseconds, saturating.
fn nanos(duration: web_time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
