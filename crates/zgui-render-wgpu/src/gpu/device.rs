//! Opening a device, and what the one that opened can do.

use std::ffi::CStr;
use std::sync::Arc;

use zgui_render::RenderCapabilities;

use crate::gpu::adapter;
use crate::gpu::extensions;
use crate::gpu::loss::DeviceLoss;

/// The optional features asked for when they are offered, and done without when they are not.
///
/// Dual-source blending is what per-channel text antialiasing needs, and it is optional on real
/// drivers. Nothing here is required: a missing feature changes which primitives are emitted, and
/// a device that refuses to open because it lacks one would be a device that renders nothing at
/// all rather than one that renders slightly worse text.
///
/// Timestamp queries are asked for only when [pass timing](crate::gpu::timing) was asked for, so
/// the default device keeps its default feature set.
fn optional_features() -> wgpu::Features {
    let features = wgpu::Features::DUAL_SOURCE_BLENDING | wgpu::Features::PIPELINE_CACHE;
    if crate::gpu::timing::requested() {
        features | wgpu::Features::TIMESTAMP_QUERY
    } else {
        features
    }
}

/// The smallest memory block the allocator reserves on a device.
const MIN_MEMORY_BLOCK: u64 = 8 << 20;

/// The largest memory block the allocator reserves on a device.
const MAX_MEMORY_BLOCK: u64 = 256 << 20;

/// How the device's allocator sizes its memory blocks.
///
/// wgpu's default reserves a 128 MiB device block and a 64 MiB host block for the first
/// allocations, and a small window uses less than 8 MiB of them. On an integrated GPU, all of that
/// is system memory. Here the first blocks are 8 MiB and 4 MiB, and each new block is twice the
/// size of the last up to 256 MiB, so a large document gets the default's block size after five
/// more allocations. An allocation larger than the current block gets a block of its own, as with
/// the default.
fn memory_hints() -> wgpu::MemoryHints {
    wgpu::MemoryHints::Manual {
        suballocated_device_memory_block_size: MIN_MEMORY_BLOCK..MAX_MEMORY_BLOCK,
    }
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
    /// Whether another program part opened the device and gave it to this one.
    adopted: bool,
}

impl Gpu {
    /// Opens a device on `adapter`, or says why it could not.
    ///
    /// A GL adapter is asked for the downlevel limit set rather than its own, because its own is
    /// routinely more than a device created from it will grant. The set is then clamped to what
    /// the adapter reports, because the downlevel set names compute limits and a GL 3.3 context
    /// has no compute to name them for.
    ///
    /// # Errors
    ///
    /// Returns the message `request_device` refused with.
    ///
    /// # Vulkan device extensions
    ///
    /// `extensions` names Vulkan device extensions to enable on top of the ones wgpu asks for. A
    /// Vulkan image created with a DRM format modifier and exported as a dma-buf needs some of
    /// them, and a device extension can be enabled only while the device is created, so the list
    /// has to arrive here. The empty slice is the ordinary request and takes the ordinary path,
    /// unchanged in every respect.
    ///
    /// Which names an exported image needs is the console backend's constant to state, and nothing
    /// here counts them. wgpu-hal 29.0.4 enables `VK_KHR_external_memory_fd` and
    /// `VK_EXT_external_memory_dma_buf` on any physical device that has them, so such a list adds
    /// fewer names than it holds.
    ///
    /// The list is all-or-nothing. Where the adapter is Vulkan and its physical device has every
    /// name, the device is created through wgpu's hal with all of them enabled. Everything else —
    /// a GL adapter, one missing name, a driver that refuses — opens the device the ordinary way
    /// and leaves [`Gpu::vulkan_extensions`] empty. A caller reads that method and does what it
    /// can do without them.
    ///
    /// The list has to be **dependency-closed**: an extension that requires another one requires
    /// that name here too. Only the names given are checked against the physical device, and
    /// `VUID-vkCreateDevice-ppEnabledExtensionNames-01387` requires the list to hold every
    /// extension required by a name in it. A list that breaks the rule is undefined behaviour, and
    /// a driver that answers `VK_ERROR_EXTENSION_NOT_PRESENT` reaches the same panic inside
    /// wgpu-hal that a missing name reaches. In `vk.xml`, every name the console backend's
    /// constant holds requires core Vulkan 1.2 or another name in that constant:
    /// `VK_EXT_external_memory_dma_buf` requires `VK_KHR_external_memory_fd`.
    pub fn open(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        extensions: &[&'static CStr],
    ) -> Result<Self, String> {
        let info = adapter.get_info();
        let limits = if info.backend == wgpu::Backend::Gl {
            // Clamped to the adapter's own values, because the downlevel set asks for compute
            // limits a GL 3.3 context reports zero for. `request_device` refuses a limit set as a
            // whole, so asking for one number the adapter cannot meet costs the device — and with
            // it the coverage rasteriser, which is the very thing such a context is for.
            wgpu::Limits::downlevel_defaults()
                .using_resolution(adapter.limits())
                .or_worse_values_from(&adapter.limits())
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
            memory_hints: memory_hints(),
            ..Default::default()
        };
        let (device, queue, extensions) = match extensions::open(&adapter, extensions, &descriptor)
        {
            Some(opened) => opened,
            None => {
                let (device, queue) =
                    futures::executor::block_on(adapter.request_device(&descriptor))
                        .map_err(|error| error.to_string())?;
                (device, queue, Vec::new())
            }
        };

        // Registered outside the match, so that a device from either arm has it. A device opened
        // through the hal reports a loss the same way, and one with no watcher would go on drawing
        // into a dead device with nothing to notice it. No test would see an arm that lost this:
        // the suite injects a loss through `DeviceLoss::report`, because a real one cannot be
        // provoked.
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
            adopted: false,
        })
    }

    /// Adopts a device that a host opened, with its queue, adapter and instance.
    ///
    /// Use this where a host, for example a game engine, owns the one graphics device of the
    /// process and the interface has to draw on that device. The host keeps its own features and
    /// limits. The capabilities are read off `device` and `adapter` the same way
    /// [`Gpu::open`] reads them, so a device without dual-source blending draws text with ordinary
    /// coverage, and so on.
    ///
    /// # Device loss
    ///
    /// wgpu keeps one device-lost callback per device, and setting a callback replaces the
    /// previous one. The host already has its callback on `device`, so this function sets no
    /// callback. The host tells this [`Gpu`] about a loss: it calls
    /// [`DeviceLoss::report`] on [`Gpu::loss`], or it installs the closure that
    /// [`DeviceLoss::observer`] returns inside its own callback. Until the host reports it, a loss
    /// is invisible to every renderer on this device.
    ///
    /// # Recovery
    ///
    /// An adopted device is never replaced by this crate: a
    /// [`SharedGraphics`](crate::SharedGraphics) made with
    /// [`SharedGraphics::with_gpu`](crate::SharedGraphics::with_gpu) opens no device of its own.
    /// After the host recovers, it adopts the new device with
    /// [`SharedGraphics::adopt`](crate::SharedGraphics::adopt).
    ///
    /// ```no_run
    /// # use zgui_render_wgpu::wgpu;
    /// # fn host() -> (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue) { unimplemented!() }
    /// use zgui_render_wgpu::{Gpu, SharedGraphics};
    ///
    /// let (instance, adapter, device, queue) = host();
    /// let gpu = Gpu::from_existing(instance, adapter, device, queue);
    /// let graphics = SharedGraphics::with_gpu(gpu);
    /// assert!(graphics.gpu().is_some());
    /// ```
    pub fn from_existing(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Arc<Self> {
        let capabilities = probe(&adapter, &device);
        Arc::new(Self {
            instance,
            adapter,
            device,
            queue,
            capabilities,
            extensions: Vec::new(),
            loss: Arc::new(DeviceLoss::new()),
            adopted: true,
        })
    }

    /// Returns `true` where the device came from [`Gpu::from_existing`].
    ///
    /// An adopted device belongs to the host. This crate does not replace it after a loss and does
    /// not watch it for one: see [`Gpu::from_existing`].
    pub fn is_adopted(&self) -> bool {
        self.adopted
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

    /// Returns the Vulkan device extensions enabled beyond the ones wgpu asks for.
    ///
    /// The names that were asked for. wgpu-hal appends them to the list `vkCreateDevice` is given,
    /// so a device that opened is a device that has them. Empty on four occasions: where nothing
    /// asked for any, where the adapter is a GL one or the target has no Vulkan backend at all,
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

    /// Lets the device retire what it has finished, so mapped staging memory comes back.
    ///
    /// `PollType::Poll` is documented as a poll which never waits. On the GL backend it has
    /// measured as a block of 20.7 ms a frame — 61% of all wall time — on the oldest target this
    /// runs on. The cause there was contention for a lock the driver holds process-wide rather
    /// than the card itself, so the figure belongs to one driver and one moment.
    ///
    /// The rule it leaves behind does not: the frame loop has one place it may block, and this is
    /// not it. So the call is made here alone, and only where the device is known to have caught
    /// up — after acquisition, where what was handed back is a buffer nothing in flight still
    /// reads. Never on the path that builds a frame.
    pub fn reclaim(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
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
    let subpixel_text = features.contains(wgpu::Features::DUAL_SOURCE_BLENDING);
    // Said once, because it decides what every glyph small enough for the atlas looks like and
    // there is otherwise no way to tell from outside which of the two a device settled on. Text
    // above the atlas threshold is filled as outlines and is grayscale whatever this says, so a
    // page carrying both sizes shows both answers at once.
    tracing::info!(
        target: "zgui::render",
        "text small enough to be cached is drawn {}",
        if subpixel_text {
            "with one coverage value per display subpixel, which assumes the display's stripes run \
             horizontally red, green, blue"
        } else {
            "with one coverage value per pixel"
        }
    );
    RenderCapabilities {
        subpixel_text,
        vector_compute: downlevel
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            // The path renderer's shaders unpack two half floats out of one word, and naga
            // refuses `unpack2x16float` without this. A software Vulkan device is where it is
            // absent — those instructions need native half-float support there and nowhere else,
            // which wgpu-hal knows and reports here. Asking is what keeps the fallback a fallback:
            // the path renderer panics on a shader it cannot compile rather than answering with an
            // error, and a panic unwinds past the branch that would have caught it.
            && downlevel
                .flags
                .contains(wgpu::DownlevelFlags::SHADER_F16_IN_F32)
            && adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm)
                .allowed_usages
                .contains(wgpu::TextureUsages::STORAGE_BINDING),
        mutable_texture_formats: downlevel
            .flags
            .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS),
        // An effect binds one uniform block beyond what an ordinary instanced draw binds, and
        // draws through a third bind group. Every device that reaches this far has both.
        custom_shaders: limits.max_bind_groups >= 3,
        max_texture_size: limits.max_texture_dimension_2d as i32,
    }
}
