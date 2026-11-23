//! A paint stage that reaches past the interface.

use compositor_types::Transaction;

/// Reads the wgpu renderer behind the interface.
pub fn record(_transaction: &mut Transaction, renderer: &mut dyn compositor_api::Renderer) {
    // PLANTED VIOLATION: a downcast to a concrete renderer.
    let _ = renderer.as_any_mut().and_then(|any| any.downcast_mut::<WgpuRenderer>());
    // PLANTED VIOLATION: a path into an implementation crate.
    let _ = compositor_core::CompositorImpl::new();
    // PLANTED VIOLATION: a path into a compositor crate that no list names.
    let _ = compositor_wgsl::effect(compositor_types::ShaderMode::PAINT, "");
}
