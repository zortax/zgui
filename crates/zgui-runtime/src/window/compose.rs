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
        if let Some(sync) = self.painter.compositor() {
            sync.open(transaction);
        }
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
    pub(super) fn submit_transaction(&mut self, transaction: compositor_types::Transaction) {
        let report = self.painter.compositor().map(|sync| sync.report());
        match self.compositor.commit(transaction) {
            Ok(committed) => {
                if let Some(report) = &report {
                    crate::window::counters::committed(&committed, report);
                }
                self.committed = Some(committed.counters);
            }
            Err(_) => {
                if let Some(sync) = self.painter.compositor() {
                    sync.reset();
                }
                self.damage = DamageSet::full();
                self.request_frame();
            }
        }
        if let Some(report) = report {
            tracing::debug!(target: "zgui::compositor", ?report, "transaction committed");
            if report.omitted_primitives > 0 && !self.compositor_omissions_noted {
                self.compositor_omissions_noted = true;
                tracing::warn!(
                    target: "zgui::compositor",
                    primitives = report.omitted_primitives,
                    "content of kinds the compositor path does not record yet is not drawn"
                );
            }
        }
    }
}
