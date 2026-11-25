//! A paint stage that records through the interface only.

// Recorded through iceblink and never iceblink_core.
use iceblink::Transaction;

/// Records nothing.
pub fn record(_transaction: &mut Transaction, renderer: &dyn iceblink::Renderer) {
    let _ = renderer.as_any();
}
