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

    /// Builds the output's frame for the moment `at` and draws it.
    ///
    /// The counters of a built frame are bridged, and with invariants checked a frame that follows
    /// this window's commit, and that the compositor scrolled nothing in, is held to the damage
    /// its paint walk drew. The scroll events the build reported are taken for the next frame to
    /// adopt.
    pub(super) fn draw_output(&mut self, at: zgui_vocab::Timestamp) -> crate::compositor::Drawn {
        let interval = self.refresh_interval();
        self.pass_gestures();
        let drawn = self
            .compositor
            .draw(&mut *self.renderer, self.subpixel_order, at, interval);
        // The compositor's own scrolling moves pixels the window's walk did not visit.
        let scrolled = self.take_scroll_events(&drawn);
        if let Some(built) = self.compositor.take_built() {
            crate::window::counters::built(&built);
            crate::window::counters::held(&**self.compositor.compositor());
            self.renderer_totals.drawn(&drawn.outcome, &*self.renderer);
            if self.check_damage
                && !scrolled
                && let Some(committed) = &self.committed
            {
                crate::window::containment::check(
                    self.compositor.frame(),
                    committed,
                    self.retired,
                    &self.painted,
                );
            }
            self.retired = drawn.outcome.retires_damage();
        }
        drawn
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
    /// A rejected transaction leaves the compositor with nothing of this window, as a rejected
    /// frame does.
    pub(super) fn submit_embed_transaction(&mut self, transaction: compositor_types::Transaction) {
        if self.compositor.commit(transaction).is_err() {
            if let Some(sync) = self.painter.compositor() {
                sync.reset();
            }
            self.damage = DamageSet::full();
            self.request_frame();
        }
    }

    /// Commits the frame's transaction.
    ///
    /// A rejected transaction leaves the compositor with nothing of this window, so the sync
    /// forgets what it recorded and the next frame records the whole surface again.
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
        let violations = self.compositor.compositor().counters().damage_violations;
        match self.compositor.commit(transaction) {
            Ok(committed) => {
                if self.check_damage {
                    let after = self.compositor.compositor().counters().damage_violations;
                    assert_eq!(
                        after, violations,
                        "the frame's paint damage misses pixels its transaction changes"
                    );
                }
                if let Some(report) = &report {
                    crate::window::counters::committed(&committed, report);
                }
                self.committed = Some(committed.counters);
            }
            Err(_) => {
                if let Some(sync) = self.painter.compositor() {
                    sync.reset();
                }
                self.driven.forget();
                self.damage = DamageSet::full();
                self.request_frame();
            }
        }
        if let Some(report) = report {
            tracing::debug!(target: "zgui::compositor", ?report, "transaction committed");
        }
    }
}
