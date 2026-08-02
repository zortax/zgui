//! The viewport the results a store is holding were produced for.
//!
//! A layout result is a function of two things: the styles and content the boxes carry, and the
//! viewport the root was measured against. Whether the first has moved is answered by the per-box
//! caches — a box that owes a layout is a box holding no cached answer. Nothing answers the second,
//! because the viewport is handed to each pass and kept nowhere, so a store cannot tell a pass into
//! the same viewport from a pass into a different one. This is what tells it.
//!
//! It is recorded by the store rather than by the caller for the same reason the caches are: a
//! caller that forgot to record would silently keep geometry laid out for the previous window size,
//! and the symptom is a document that never re-flows when the window is resized — which no
//! assertion about *what changed* can see, because nothing changed.

use taffy::Size;

use crate::tree::store::LayoutStore;

impl LayoutStore {
    /// The viewport the last root layout that ran to completion was asked for.
    ///
    /// `None` before any pass has completed, and after anything that makes the held results
    /// meaningless whatever the viewport is.
    pub fn laid_out_viewport(&self) -> Option<Size<f32>> {
        self.laid_out.map(|(width, height)| Size {
            width: f32::from_bits(width),
            height: f32::from_bits(height),
        })
    }

    /// Whether the results now held were produced for exactly this viewport.
    ///
    /// Compared by bits rather than by value, so that two viewports which are not the same number
    /// are never taken for one — including the case where one of them is a NaN that arrived from a
    /// degenerate surface, which compares unequal to itself and must therefore force a pass rather
    /// than skip one.
    pub fn laid_out_for(&self, viewport: Size<f32>) -> bool {
        self.laid_out == Some((viewport.width.to_bits(), viewport.height.to_bits()))
    }

    /// Records that a root layout into `viewport` ran to completion.
    pub(crate) fn record_root_layout(&mut self, viewport: Size<f32>) {
        self.laid_out = Some((viewport.width.to_bits(), viewport.height.to_bits()));
    }

    /// Forgets which viewport the held results belong to, so that the next pass runs whatever it is
    /// asked for.
    pub(crate) fn forget_root_layout(&mut self) {
        self.laid_out = None;
    }
}
