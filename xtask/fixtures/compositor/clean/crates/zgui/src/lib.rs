//! The app wiring, which chooses the implementations.

/// Builds the compositor every window draws through.
pub fn compositor() -> Box<dyn iceblink::Compositor> {
    Box::new(iceblink_core::CompositorImpl::new())
}
