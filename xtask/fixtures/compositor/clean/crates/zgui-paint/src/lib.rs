//! A paint stage that records through the interface only.

// Recorded through compositor_types and never compositor_core.
use compositor_types::Transaction;
// A module of the contract crate, which shares its name with a compositor crate.
use compositor_types::compositor_color::Color;

/// Records nothing.
pub fn record(_transaction: &mut Transaction, renderer: &dyn compositor_api::Renderer) {
    let _ = renderer.as_any();
}
