//! Which device already has a path renderer.

use std::sync::{Arc, Weak};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};

use zgui_render_wgpu::Gpu;

use crate::device::{Shared, SharedRenderer};

/// Every device that currently has one, weakly.
///
/// Weakly, because a renderer must not be what keeps a device alive: the entry has to disappear when
/// the last window on that device does, or a lost device would be held by its own cache for the rest
/// of the process.
type Entries = Vec<(Weak<Gpu>, Weak<SharedRenderer>)>;

/// The native registry, shared by the threads that can own a GPU.
#[cfg(not(target_arch = "wasm32"))]
fn with_registry<T>(visit: impl FnOnce(&mut Entries) -> T) -> T {
    static REGISTRY: OnceLock<Mutex<Entries>> = OnceLock::new();
    let registry = REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut held = registry.lock().unwrap_or_else(|held| held.into_inner());
    visit(&mut held)
}

/// Browser GPU objects stay on the thread that owns their JavaScript handles.
#[cfg(target_arch = "wasm32")]
fn with_registry<T>(visit: impl FnOnce(&mut Entries) -> T) -> T {
    thread_local! {
        static REGISTRY: std::cell::RefCell<Entries> = const { std::cell::RefCell::new(Vec::new()) };
    }
    REGISTRY.with(|registry| visit(&mut registry.borrow_mut()))
}

/// The path renderer for `gpu`, building one only if that device has none yet.
///
/// Two windows on one device share one renderer, and two devices never do. That is not a
/// micro-optimisation: the renderer's fixed buffers are measured in hundreds of megabytes, so one
/// per window is a device's whole video-memory budget spent on copies of one thing.
///
/// # Errors
///
/// The renderer's own construction error, when the device cannot run what it needs.
pub fn for_gpu(gpu: &Arc<Gpu>) -> Result<Shared, vello::Error> {
    with_registry(|held| {
        held.retain(|(device, renderer)| device.strong_count() > 0 && renderer.strong_count() > 0);
        if let Some(existing) = held.iter().find_map(|(device, renderer)| {
            device
                .upgrade()
                .filter(|held| Arc::ptr_eq(held, gpu))
                .and_then(|_| renderer.upgrade())
        }) {
            return Ok(existing);
        }
        let built: Shared = Arc::new(SharedRenderer::new(gpu)?);
        held.push((Arc::downgrade(gpu), Arc::downgrade(&built)));
        Ok(built)
    })
}
