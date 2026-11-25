//! The vello path renderer the windows on one device share.

use std::sync::{Arc, Mutex, Weak};

use iceblink_vello::{Choice, SharedRenderer, VelloOptions, VelloRaster};
use iceblink_wgpu::{Gpu, VectorFactory};

/// Returns the factory of the path rasteriser a device can run, built with `options`.
///
/// Rasterisers on one device share the vello path renderer that `paths` keeps for it.
pub(crate) fn vectors(options: VelloOptions, paths: PathRenderers) -> VectorFactory {
    Box::new(
        move |gpu: &Arc<Gpu>, size| match paths.shared(gpu, &options) {
            Some(shared) => Box::new(VelloRaster::sharing(shared, gpu.queue())),
            None => {
                iceblink_vello::for_device(gpu.adapter(), gpu.device(), gpu.queue(), size, &options)
            }
        },
    )
}

/// The vello path renderer of the last device a window drew paths on.
///
/// A second window on the same device shares the renderer and its fixed buffers. The renderer is
/// dropped with the last rasteriser that uses it.
#[derive(Clone, Default)]
pub(crate) struct PathRenderers {
    held: Arc<Mutex<Option<Held>>>,
}

/// A path renderer and the device it was built on, both held weakly.
struct Held {
    device: Weak<Gpu>,
    renderer: Weak<SharedRenderer>,
}

impl PathRenderers {
    /// Returns the path renderer of `gpu`, and builds it with `options` when the device has none.
    ///
    /// Returns `None` when the device cannot run vello or the renderer fails to build.
    fn shared(&self, gpu: &Arc<Gpu>, options: &VelloOptions) -> Option<Arc<SharedRenderer>> {
        if iceblink_vello::chosen(gpu.adapter()) != Choice::Compute {
            return None;
        }
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // A replacement device after a loss is another `Gpu`, so it gets a renderer of its own.
        if let Some(entry) = held.as_ref()
            && entry
                .device
                .upgrade()
                .is_some_and(|device| Arc::ptr_eq(&device, gpu))
            && let Some(shared) = entry.renderer.upgrade()
        {
            return Some(shared);
        }
        let options = VelloOptions {
            adapter: options.adapter.clone().or(Some(gpu.adapter().get_info())),
            cache_directory: options.cache_directory.clone(),
        };
        let shared = SharedRenderer::new(gpu.device(), &options).ok()?;
        *held = Some(Held {
            device: Arc::downgrade(gpu),
            renderer: Arc::downgrade(&shared),
        });
        Some(shared)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iceblink_vello::{Choice, VelloOptions};
    use iceblink_wgpu::SharedGraphics;

    use super::{PathRenderers, vectors};

    #[test]
    fn two_windows_on_one_device_share_one_path_renderer() {
        let graphics = SharedGraphics::new();
        let Ok(gpu) = graphics.open_gpu() else {
            eprintln!("skipped: no graphics device");
            return;
        };
        if iceblink_vello::chosen(gpu.adapter()) != Choice::Compute {
            eprintln!("skipped: the device cannot run vello");
            return;
        }
        let paths = PathRenderers::default();
        let first_window = vectors(VelloOptions::default(), paths.clone());
        let second_window = vectors(VelloOptions::default(), paths.clone());

        let rasters = [
            first_window(&gpu, [256, 256]),
            second_window(&gpu, [256, 256]),
        ];

        let held = paths
            .held
            .lock()
            .expect("no test panicked holding the lock");
        let shared = held
            .as_ref()
            .and_then(|entry| entry.renderer.upgrade())
            .expect("the windows' path renderer is alive");
        assert_eq!(
            Arc::strong_count(&shared),
            rasters.len() + 1,
            "both windows draw through the one path renderer"
        );
    }
}
