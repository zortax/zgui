//! Opening a device, and what the one that opened can do.

use std::sync::Arc;

use zgui_render::RenderCapabilities;

use crate::gpu::adapter;
use crate::gpu::loss::DeviceLoss;

/// The optional features asked for when they are offered, and done without when they are not.
///
/// Dual-source blending is what per-channel text antialiasing needs, and it is optional on real
/// drivers. Nothing here is required: a missing feature changes which primitives are emitted, and
/// a device that refuses to open because it lacks one would be a device that renders nothing at
/// all rather than one that renders slightly worse text.
fn optional_features() -> wgpu::Features {
    wgpu::Features::DUAL_SOURCE_BLENDING | wgpu::Features::PIPELINE_CACHE
}

/// A device, its queue, the adapter they came from and the instance behind all three.
///
/// One of these per graphics device, shared by every surface drawn on it. Everything the renderer
/// allocates hangs off it, and everything it holds dies with it — which is why device loss is a
/// rebuild rather than a repair.
#[derive(Debug)]
pub struct Gpu {
    /// The instance every surface is created from.
    instance: wgpu::Instance,
    /// The adapter the device came from, kept for its info and its capabilities.
    adapter: wgpu::Adapter,
    /// The device.
    device: wgpu::Device,
    /// The queue.
    queue: wgpu::Queue,
    /// What this device can do.
    capabilities: RenderCapabilities,
    /// Whether the device has been reported lost.
    loss: Arc<DeviceLoss>,
}

impl Gpu {
    /// Opens a device on `adapter`, or says why it could not.
    ///
    /// A GL adapter is asked for the downlevel limit set rather than its own, because its own is
    /// routinely more than a device created from it will grant.
    pub fn open(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Result<Self, String> {
        let info = adapter.get_info();
        let limits = if info.backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
        } else {
            adapter.limits()
        };
        let (device, queue) =
            futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("zgui.device"),
                required_features: optional_features() & adapter.features(),
                required_limits: limits,
                ..Default::default()
            }))
            .map_err(|error| error.to_string())?;

        let loss = Arc::new(DeviceLoss::new());
        let watcher = Arc::clone(&loss);
        device.set_device_lost_callback(move |reason, message| watcher.report(reason, &message));

        let capabilities = probe(&adapter, &device);
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            capabilities,
            loss,
        })
    }

    /// The instance surfaces are created from.
    ///
    /// A surface has to come from the same instance the device did, so the crate that owns the
    /// native window handle needs this rather than one of its own.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// The adapter the device came from.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// The device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// What this device can do.
    pub fn capabilities(&self) -> RenderCapabilities {
        self.capabilities
    }

    /// Whether the device has been reported lost.
    pub fn loss(&self) -> &Arc<DeviceLoss> {
        &self.loss
    }

    /// A one-line description of the adapter, for a startup line or a rejection list.
    pub fn describe(&self) -> String {
        adapter::describe(&self.adapter.get_info())
    }

    /// Blocks until everything submitted has finished.
    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

/// What a device created from `adapter` turned out to be able to do.
///
/// Read off the device rather than the adapter wherever the two can disagree, because an adapter's
/// reported capabilities are a promise about a device that has not been created yet.
fn probe(adapter: &wgpu::Adapter, device: &wgpu::Device) -> RenderCapabilities {
    let downlevel = adapter.get_downlevel_capabilities();
    let features = device.features();
    let limits = device.limits();
    RenderCapabilities {
        subpixel_text: features.contains(wgpu::Features::DUAL_SOURCE_BLENDING),
        vector_compute: downlevel
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            && adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm)
                .allowed_usages
                .contains(wgpu::TextureUsages::STORAGE_BINDING),
        mutable_texture_formats: downlevel
            .flags
            .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS),
        max_texture_size: limits.max_texture_dimension_2d as i32,
    }
}
