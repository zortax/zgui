//! A window's frame recorded into its compositor.

use zgui_bits::DamageSet;
use zgui_paint::CompositorSync;

use crate::compositor::CompositorRenderer;
use crate::window::Window;

impl Window {
    /// Opens the frame's transaction, when the window draws through a compositor.
    ///
    /// The first call attaches a sync to the painter, so the walk records every fragment it
    /// visits.
    pub(super) fn open_transaction(&mut self) {
        let Some(bridge) = bridge(&mut *self.renderer) else {
            return;
        };
        let (output, transaction) = (bridge.output(), bridge.transaction());
        if self.painter.compositor().is_none() {
            self.painter.attach_compositor(CompositorSync::new(output));
        }
        if let Some(sync) = self.painter.compositor() {
            sync.open(transaction);
        }
    }

    /// Commits the frame's transaction.
    ///
    /// A rejected transaction leaves the compositor with nothing of this window, so the sync
    /// forgets what it recorded and the next frame redraws the whole surface.
    pub(super) fn submit_transaction(&mut self, transaction: compositor_types::Transaction) {
        let report = self.painter.compositor().map(|sync| sync.report());
        let Some(bridge) = bridge(&mut *self.renderer) else {
            return;
        };
        bridge.submit(transaction);
        if bridge.take_rejected() {
            if let Some(sync) = self.painter.compositor() {
                sync.reset();
            }
            self.damage = DamageSet::full();
            self.request_frame();
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

/// Returns the window's renderer as a compositor bridge, if it is one.
fn bridge(renderer: &mut dyn zgui_render::Renderer) -> Option<&mut CompositorRenderer> {
    renderer
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<CompositorRenderer>())
}
