//! The app wiring, which chooses the implementations.

/// Builds the compositor every window draws through.
pub fn compositor() -> Box<dyn compositor_api::Compositor> {
    Box::new(compositor_core::CompositorImpl::new())
}
