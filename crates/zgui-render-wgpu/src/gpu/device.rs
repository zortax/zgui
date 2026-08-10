//! Opening a device, and what the one that opened can do.

use std::ffi::CStr;
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
/// One of these per graphics device, shared by every surface drawn on it — which is what
/// [`SharedGraphics`](crate::SharedGraphics) is for, and how an application's windows come to draw
/// on one device rather than one each. Everything the renderer allocates hangs off it, and
/// everything it holds dies with it, which is why device loss is a rebuild rather than a repair.
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
    /// The Vulkan device extensions enabled beyond the ones wgpu asks for.
    extensions: Vec<&'static CStr>,
    /// Whether the device has been reported lost.
    loss: Arc<DeviceLoss>,
}

impl Gpu {
    /// Opens a device on `adapter`, or says why it could not.
    ///
    /// A GL adapter is asked for the downlevel limit set rather than its own, because its own is
    /// routinely more than a device created from it will grant.
    ///
    /// # Vulkan device extensions
    ///
    /// `extensions` names Vulkan device extensions to enable on top of the ones wgpu asks for. A
    /// Vulkan image created with a DRM format modifier and exported as a dma-buf needs some of
    /// them, and a device extension can be enabled only while the device is created, so the list
    /// has to arrive here. The empty slice is the ordinary request and takes the ordinary path,
    /// unchanged in every respect.
    ///
    /// The list is all-or-nothing. Where the adapter is Vulkan and its physical device has every
    /// name, the device is created through wgpu's hal with all of them enabled. Everything else —
    /// a GL adapter, one missing name, a driver that refuses — opens the device the ordinary way
    /// and leaves [`Gpu::vulkan_extensions`] empty. A caller reads that method and does what it
    /// can do without them.
    pub fn open(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        extensions: &[&'static CStr],
    ) -> Result<Self, String> {
        let info = adapter.get_info();
        let limits = if info.backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
        } else {
            adapter.limits()
        };
        // Derived once and read by both paths. The hal path creates the device itself, so a
        // feature set or a limit set of its own would give a device capabilities the ordinary path
        // never grants, with nothing anywhere to report the difference.
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("zgui.device"),
            required_features: optional_features() & adapter.features(),
            required_limits: limits,
            ..Default::default()
        };
        let (device, queue, extensions) =
            match open_with_extensions(&adapter, extensions, &descriptor) {
                Some((device, queue)) => (device, queue, extensions.to_vec()),
                None => {
                    let (device, queue) =
                        futures::executor::block_on(adapter.request_device(&descriptor))
                            .map_err(|error| error.to_string())?;
                    (device, queue, Vec::new())
                }
            };

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
            extensions,
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

    /// Returns the Vulkan device extensions enabled on this device beyond the ones wgpu asks for.
    ///
    /// Empty on four occasions: where nothing asked for any, where the adapter is not a Vulkan one,
    /// where the physical device lacks a name that was asked for, and where the driver refused a
    /// device carrying them. So this is the answer to whether they were enabled, and it is an
    /// answer a caller has to read: an image created for an extension the device never enabled is
    /// refused much later, by a call that names something else entirely.
    pub fn vulkan_extensions(&self) -> &[&'static CStr] {
        &self.extensions
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

/// Opens a device with `extensions` enabled, or answers `None` where that cannot be done.
///
/// `None` is a report rather than a failure, and every one of its causes is ordinary. The fast
/// path is Vulkan-only, because a DRM format modifier and a dma-buf export are Vulkan things:
/// [`wgpu::Adapter::as_hal`] answers `None` on the GL backend, and the caller opens the device the
/// ordinary way instead.
///
/// # Why the names are checked first
///
/// `vkCreateDevice` answers `VK_ERROR_EXTENSION_NOT_PRESENT` for a name the physical device does
/// not have, and wgpu-hal turns that one result into a panic rather than an error. So a name is
/// found to be missing here, before the driver ever sees it, which is also what makes the list
/// all-or-nothing: a partly enabled set would leave a caller believing it had the rest.
fn open_with_extensions(
    adapter: &wgpu::Adapter,
    extensions: &[&'static CStr],
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Option<(wgpu::Device, wgpu::Queue)> {
    if extensions.is_empty() {
        return None;
    }
    // SAFETY: the guard is read from and dropped at the end of this function. Nothing reachable
    // through it is destroyed here, and the device made from it below outlives it, which is what
    // `as_hal` asks of a caller.
    let hal = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }?;
    let physical = hal.physical_device_capabilities();
    if let Some(missing) = extensions
        .iter()
        .find(|name| !physical.supports_extension(name))
    {
        tracing::warn!(
            adapter = %adapter::describe(&adapter.get_info()),
            extension = ?missing,
            "the physical device does not have a Vulkan device extension that was asked for"
        );
        return None;
    }

    // Adding names is the one change the callback is allowed to make, and `create_info` is
    // documented to have its extension list overwritten, so the names go in the vector.
    let callback: Box<wgpu::hal::vulkan::CreateDeviceCallback<'_>> = Box::new(move |args| {
        for &name in extensions {
            if !args.extensions.contains(&name) {
                args.extensions.push(name);
            }
        }
    });
    // SAFETY: the features, limits and memory hints are the ones `request_device` would have
    // passed, so this device is created with exactly what `descriptor` states. The callback only
    // appends extension names, every one of which was found on the physical device above; it
    // removes nothing, disables no feature, and touches no other field.
    let opened = unsafe {
        hal.open_with_callback(
            descriptor.required_features,
            &descriptor.required_limits,
            &descriptor.memory_hints,
            Some(callback),
        )
    };
    let opened = match opened {
        Ok(opened) => opened,
        Err(error) => {
            tracing::warn!(
                adapter = %adapter::describe(&adapter.get_info()),
                %error,
                "a device with the Vulkan device extensions asked for was refused"
            );
            return None;
        }
    };
    // SAFETY: `opened` was created from this adapter's own hal adapter, immediately above, and
    // `descriptor.required_features` is the exact feature set it was created with.
    match unsafe { adapter.create_device_from_hal(opened, descriptor) } {
        Ok((device, queue)) => Some((device, queue)),
        Err(error) => {
            tracing::warn!(
                adapter = %adapter::describe(&adapter.get_info()),
                %error,
                "a device created through the hal could not be adopted"
            );
            None
        }
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
