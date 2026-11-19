//! A Vulkan swap chain this crate creates, acquires from and presents to itself.
//!
//! # Why not wgpu's
//!
//! wgpu lays the first barrier of every acquired image down from `VK_IMAGE_LAYOUT_UNDEFINED`, so
//! the image's contents are gone before a frame draws into it. And it presents with no
//! `VkPresentRegionsKHR`, so the compositor is told the whole image changed. Both are decided
//! inside wgpu, and neither can be changed from outside it.
//!
//! # The images keep their contents
//!
//! The specification of `vkQueuePresentKHR` states it: "Upon reacquiring the image and
//! transitioning it away from the `VK_IMAGE_LAYOUT_PRESENT_SRC_KHR` layout, the contents will be
//! the same as they were prior to transitioning the image to the present source layout and
//! presenting it." So the barrier that takes an image back starts from `PRESENT_SRC_KHR`, never
//! from `UNDEFINED`, and a frame copies only what that image owes. A swap chain made again
//! discards the contents, and every image then owes everything.
//!
//! # wgpu's tracker
//!
//! Each image is wrapped once as a `wgpu::Texture`. wgpu tracks it as a colour target after the
//! first frame, and the two barriers here keep that true: the acquire barrier moves the image from
//! `PRESENT_SRC_KHR` to `COLOR_ATTACHMENT_OPTIMAL` before wgpu records anything into it, and the
//! release barrier moves it back after the frame. The same pattern as the scanout buffers of the
//! console backend.
//!
//! # Order on the queue
//!
//! Per frame, on the queue wgpu submits frames on:
//!
//! 1. `vkAcquireNextImageKHR` signals a semaphore of the acquire ring.
//! 2. The acquire barrier waits for that semaphore at the colour attachment stage and signals the
//!    ring entry's fence. A barrier orders against everything submitted after it, so the frame's
//!    own copy, submitted next, is ordered after the semaphore.
//! 3. wgpu submits the copy.
//! 4. The release barrier signals the image's own semaphore.
//! 5. `vkQueuePresentKHR` waits for that semaphore, with the damage chained on.
//!
//! The renderer draws on one thread, and every submission here is made from it, so the queue is
//! never used from two threads at once. The console backend submits its barriers the same way.
//!
//! # Semaphores
//!
//! The acquire semaphores are a ring one longer than the image count. An entry is used again only
//! once its fence says the barrier that waited on it finished, which also bounds how far the
//! processor runs ahead of the device. The semaphore a present waits on is the image's own: it is
//! signalled again only after the image is acquired again, which is after the present that waited
//! on it was done with it.

use std::sync::Arc;

use ash::vk;
use zgui_geom::{Device, Rect, Size};

use crate::gpu::device::Gpu;
use crate::target::acquire::Acquisition;
use crate::target::debt::Debt;
use crate::target::native::{ChainConfig, NativeImage, clipped_damage};

/// How long an acquisition waits for an image, as wgpu's own does.
const ACQUIRE_TIMEOUT: u64 = 1_000_000_000;

/// How long a fence of the acquire ring is waited on before the device is called stuck.
const FENCE_TIMEOUT: u64 = 2_000_000_000;

/// The part of an image a barrier covers: the colour aspect, one level, one layer.
const WHOLE: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

/// One image of the swap chain, with what moves it between the frame and the compositor.
struct Image {
    /// The image, as wgpu sees it.
    texture: wgpu::Texture,
    /// The barrier that takes the image for its first frame, from `UNDEFINED`.
    first: vk::CommandBuffer,
    /// The barrier that takes the image back from the compositor, from `PRESENT_SRC_KHR`.
    again: vk::CommandBuffer,
    /// The barrier that gives the image to the compositor.
    release: vk::CommandBuffer,
    /// What the release barrier signals and the present waits for.
    rendered: vk::Semaphore,
    /// Whether a frame was drawn into the image since the swap chain was made.
    used: bool,
}

/// One entry of the acquire ring.
struct Acquire {
    /// What `vkAcquireNextImageKHR` signals and the acquire barrier waits for.
    semaphore: vk::Semaphore,
    /// What the acquire barrier signals.
    fence: vk::Fence,
    /// Whether a submission was handed the fence and it was not waited on since.
    pending: bool,
}

/// A Vulkan swap chain owned by this crate.
pub(crate) struct Chain {
    /// The device, kept alive for as long as anything here exists on it.
    gpu: Arc<Gpu>,
    /// The device's function table.
    device: ash::Device,
    /// The swap chain functions.
    swapchain_fns: ash::khr::swapchain::Device,
    /// The surface functions.
    surface_fns: ash::khr::surface::Instance,
    /// The physical device the surface is asked about.
    physical: vk::PhysicalDevice,
    /// The queue frames are submitted and presented on.
    queue: vk::Queue,
    /// The surface, which wgpu's `Surface` owns and outlives this.
    surface: vk::SurfaceKHR,
    /// Whether presents may name the rectangles that changed.
    incremental: bool,
    /// Whether a swap chain may be viewed in another format.
    mutable_format: bool,
    /// The pool every command buffer here is allocated from.
    pool: vk::CommandPool,
    /// The swap chain, null before the first configuration.
    swapchain: vk::SwapchainKHR,
    /// The images of the swap chain.
    images: Vec<Image>,
    /// The acquire ring.
    acquires: Vec<Acquire>,
    /// The entry of the ring the next acquisition uses.
    next: usize,
    /// The extent of the images.
    extent: Size<i32, Device>,
    /// Whether the images can be copied out of.
    readable: bool,
    /// What each image owes the composed target.
    debt: Debt,
    /// The frame extent of the last present, or `None` when the next present has to name the
    /// whole frame.
    presented: Option<Size<i32, Device>>,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("vulkan::Chain")
            .field("images", &self.images.len())
            .field("extent", &self.extent)
            .field("incremental", &self.incremental)
            .finish_non_exhaustive()
    }
}

/// The name of the extension that carries the rectangles of a present.
pub(crate) const INCREMENTAL_PRESENT: &std::ffi::CStr = ash::khr::incremental_present::NAME;

impl Chain {
    /// A chain for `surface`, or `None` where the device or the surface cannot have one.
    pub(crate) fn new(gpu: &Arc<Gpu>, surface: &wgpu::Surface<'static>) -> Option<Self> {
        // SAFETY: `as_hal` asks that the surface is not destroyed while the guard lives. The guard
        // is read through and dropped here, and the surface outlives this chain.
        let raw_surface = unsafe {
            surface
                .as_hal::<wgpu::hal::api::Vulkan>()
                .and_then(|hal| hal.raw_native_handle())
        }?;
        // SAFETY: as above, for the device. The handles read out are owned by the device, which
        // the `Arc<Gpu>` held here keeps alive.
        let hal = unsafe { gpu.device().as_hal::<wgpu::hal::api::Vulkan>() }?;
        let device = hal.raw_device().clone();
        let instance = hal.shared_instance();
        let swapchain_fns = ash::khr::swapchain::Device::new(instance.raw_instance(), &device);
        let surface_fns =
            ash::khr::surface::Instance::new(instance.entry(), instance.raw_instance());
        let enabled = hal.enabled_device_extensions();
        let incremental = enabled.contains(&INCREMENTAL_PRESENT);
        let mutable_format = enabled.contains(&ash::khr::swapchain_mutable_format::NAME);
        let physical = hal.raw_physical_device();
        let family = hal.queue_family_index();
        let queue = hal.raw_queue();

        // SAFETY: the physical device and the surface are live, and the family is one of the
        // physical device's own.
        let supported = unsafe {
            surface_fns.get_physical_device_surface_support(physical, family, raw_surface)
        };
        if !matches!(supported, Ok(true)) {
            return None;
        }

        let info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY: the family is the one the device's queue was created from.
        let pool = unsafe { device.create_command_pool(&info, None) }.ok()?;
        tracing::info!(
            incremental,
            "a window presents through a swap chain this renderer made itself"
        );
        Some(Self {
            gpu: Arc::clone(gpu),
            device,
            swapchain_fns,
            surface_fns,
            physical,
            queue,
            surface: raw_surface,
            incremental,
            mutable_format,
            pool,
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::new(),
            acquires: Vec::new(),
            next: 0,
            extent: Size::new(0, 0),
            readable: false,
            debt: Debt::new(0),
            presented: None,
        })
    }

    /// Makes the swap chain again for `config`.
    pub(crate) fn configure(&mut self, config: &ChainConfig) -> Result<(), String> {
        let format = vk_format(config.format)
            .ok_or_else(|| format!("{:?} has no Vulkan format here", config.format))?;
        let view_formats: Vec<vk::Format> = config
            .view_formats
            .iter()
            .map(|format| vk_format(*format))
            .collect::<Option<_>>()
            .ok_or("a view format has no Vulkan format here")?;
        if !view_formats.is_empty() && !self.mutable_format {
            return Err("the device cannot view a swap chain in another format".to_owned());
        }

        // SAFETY: the physical device and the surface are live.
        let capabilities = unsafe {
            self.surface_fns
                .get_physical_device_surface_capabilities(self.physical, self.surface)
        }
        .map_err(|error| format!("the surface capabilities: {error}"))?;
        // SAFETY: as above.
        let surface_formats = unsafe {
            self.surface_fns
                .get_physical_device_surface_formats(self.physical, self.surface)
        }
        .map_err(|error| format!("the surface formats: {error}"))?;
        let color_space = surface_formats
            .iter()
            .filter(|offered| offered.format == format)
            .map(|offered| offered.color_space)
            .min_by_key(|space| *space != vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .ok_or_else(|| format!("the surface does not offer {format:?}"))?;
        // SAFETY: as above.
        let modes = unsafe {
            self.surface_fns
                .get_physical_device_surface_present_modes(self.physical, self.surface)
        }
        .map_err(|error| format!("the presentation modes: {error}"))?;
        let mode = wgpu::hal::vulkan::conv::map_present_mode(config.present_mode);
        if !modes.contains(&mode) {
            return Err(format!("the surface does not offer {mode:?}"));
        }
        let alpha = wgpu::hal::vulkan::conv::map_composite_alpha_mode(config.alpha_mode);
        if !capabilities.supported_composite_alpha.contains(alpha) {
            return Err(format!("the surface does not offer {alpha:?}"));
        }

        let extent = vk::Extent2D {
            width: (config.size.width.max(1) as u32).clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width.max(1),
            ),
            height: (config.size.height.max(1) as u32).clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height.max(1),
            ),
        };
        let wanted = crate::gpu::surface::FRAME_LATENCY + 1;
        let count = match capabilities.max_image_count {
            0 => wanted.max(capabilities.min_image_count),
            most => wanted.max(capabilities.min_image_count).min(most),
        };
        self.readable = capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC);
        let usage = if self.readable {
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        };

        // Nothing in flight may still name an image this destroys. wgpu's own configure waits the
        // same way.
        self.idle();
        self.release_images();

        let mut listed = vec![format];
        listed.extend(view_formats.iter().copied().filter(|view| *view != format));
        let mut format_list = vk::ImageFormatListCreateInfo::default().view_formats(&listed);
        let old = self.swapchain;
        let mut info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(count)
            .image_format(format)
            .image_color_space(color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(alpha)
            .present_mode(mode)
            // Every pixel belongs to the images, covered or not, so an image that keeps its
            // contents keeps all of them.
            .clipped(false)
            .old_swapchain(old);
        if listed.len() > 1 {
            info = info
                .flags(vk::SwapchainCreateFlagsKHR::MUTABLE_FORMAT)
                .push_next(&mut format_list);
        }
        // SAFETY: the surface is live and supported on this queue family, every field was checked
        // against what the surface offers above, and `old` is this chain's own swap chain or null.
        let created = unsafe { self.swapchain_fns.create_swapchain(&info, None) };
        if old != vk::SwapchainKHR::null() {
            // SAFETY: the device is idle and nothing of the old swap chain is left: its images
            // were released above. A retired swap chain may be destroyed once nothing uses it.
            unsafe { self.swapchain_fns.destroy_swapchain(old, None) };
            self.swapchain = vk::SwapchainKHR::null();
        }
        self.swapchain = created.map_err(|error| format!("creating the swap chain: {error}"))?;
        self.extent = Size::new(extent.width as i32, extent.height as i32);

        // SAFETY: the swap chain was created immediately above.
        let raw_images = unsafe { self.swapchain_fns.get_swapchain_images(self.swapchain) }
            .map_err(|error| format!("the swap chain images: {error}"))?;
        for raw in raw_images {
            let image = self.wrap(raw, config)?;
            self.images.push(image);
        }
        while self.acquires.len() < self.images.len() + 1 {
            let acquire = self.ring_entry()?;
            self.acquires.push(acquire);
        }
        self.debt = Debt::new(self.images.len());
        self.presented = None;
        Ok(())
    }

    /// Acquires the image the next frame is copied into, and submits the barrier that takes it.
    pub(crate) fn acquire(&mut self) -> (Acquisition, Option<NativeImage>) {
        if self.swapchain == vk::SwapchainKHR::null() {
            return (Acquisition::Outdated, None);
        }
        let ring = self.next;
        if !self.settle(ring) {
            return (Acquisition::Timeout, None);
        }
        let semaphore = self.acquires[ring].semaphore;
        // SAFETY: the swap chain is live, and the semaphore is unsignalled with no pending
        // operation: its last wait finished, as the fence settled above says.
        let acquired = unsafe {
            self.swapchain_fns.acquire_next_image(
                self.swapchain,
                ACQUIRE_TIMEOUT,
                semaphore,
                vk::Fence::null(),
            )
        };
        let (index, suboptimal) = match acquired {
            Ok(acquired) => acquired,
            Err(error) => return (self.classify(error), None),
        };
        let slot = index as usize;
        let Some(image) = self.images.get_mut(slot) else {
            return (Acquisition::Validation, None);
        };
        let command = if image.used { image.again } else { image.first };
        image.used = true;
        let texture = image.texture.clone();

        let waits = [semaphore];
        let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let commands = [command];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&stages)
            .command_buffers(&commands);
        let fence = self.acquires[ring].fence;
        // SAFETY: the command buffer was recorded for simultaneous use and names an image of this
        // swap chain. The fence is unsignalled and not pending. The queue is used only from the
        // thread that draws, which is this one.
        let submitted = unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit), fence)
        };
        if let Err(error) = submitted {
            tracing::warn!(%error, "the barrier that takes a swap chain image was refused");
            return (self.classify(error), None);
        }
        self.acquires[ring].pending = true;
        self.next = (ring + 1) % self.acquires.len();
        let acquisition = if suboptimal {
            Acquisition::Suboptimal
        } else {
            Acquisition::Success
        };
        (
            acquisition,
            Some(NativeImage {
                slot,
                texture,
                readable: self.readable,
            }),
        )
    }

    /// Records rectangles a frame wrote into the composed target.
    pub(crate) fn note_written(&mut self, rects: &[Rect<i32, Device>]) {
        self.debt.note_written(rects);
    }

    /// Takes what the image at `slot` owes, inside a frame of `frame` pixels.
    pub(crate) fn take_owed(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
    ) -> Vec<Rect<i32, Device>> {
        let frame = Size::new(
            frame.width.min(self.extent.width),
            frame.height.min(self.extent.height),
        );
        self.debt.take(slot, frame, false)
    }

    /// Makes every image owe everything.
    pub(crate) fn forget(&mut self) {
        self.debt.forget();
        self.presented = None;
    }

    /// Releases the image at `slot` and presents it.
    ///
    /// Answers what the present said, and how many pixels it named as changed.
    pub(crate) fn present(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
        damage: &[Rect<i32, Device>],
    ) -> (Acquisition, u64) {
        let Some(image) = self.images.get(slot) else {
            return (Acquisition::Validation, 0);
        };
        let signals = [image.rendered];
        let commands = [image.release];
        let submit = vk::SubmitInfo::default()
            .command_buffers(&commands)
            .signal_semaphores(&signals);
        // SAFETY: the command buffer was recorded for simultaneous use and names an image of this
        // swap chain. The semaphore is unsignalled: the present that last waited on it took it.
        let submitted = unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit), vk::Fence::null())
        };
        if let Err(error) = submitted {
            tracing::warn!(%error, "the barrier that gives a swap chain image back was refused");
            return (self.classify(error), 0);
        }

        // A changed frame extent is a changed viewport source, and a compositor compares the new
        // buffer with what it showed at the old one. So such a present names the whole frame.
        let full = self.presented != Some(frame);
        let frame = Size::new(
            frame.width.min(self.extent.width),
            frame.height.min(self.extent.height),
        );
        let rects: Vec<vk::RectLayerKHR> = if full {
            Vec::new()
        } else {
            clipped_damage(damage, frame)
                .iter()
                .map(|rect| vk::RectLayerKHR {
                    offset: vk::Offset2D {
                        x: rect.origin.x,
                        y: rect.origin.y,
                    },
                    extent: vk::Extent2D {
                        width: rect.size.width as u32,
                        height: rect.size.height as u32,
                    },
                    layer: 0,
                })
                .collect()
        };
        let region = [vk::PresentRegionKHR::default().rectangles(&rects)];
        let mut regions = vk::PresentRegionsKHR::default().regions(&region);
        let swapchains = [self.swapchain];
        let indices = [slot as u32];
        let mut info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signals)
            .swapchains(&swapchains)
            .image_indices(&indices);
        if self.incremental && !rects.is_empty() {
            info = info.push_next(&mut regions);
        }
        // SAFETY: the image was acquired from this swap chain and released above, the semaphore is
        // signalled by that release, and the rectangles lie inside the image. The queue is used
        // only from this thread.
        let presented = unsafe { self.swapchain_fns.queue_present(self.queue, &info) };
        let damaged = if rects.is_empty() {
            damage_area(&[Rect::new(zgui_geom::Point::new(0, 0), frame)])
        } else {
            rects
                .iter()
                .map(|rect| u64::from(rect.extent.width) * u64::from(rect.extent.height))
                .sum()
        };
        let answer = match presented {
            Ok(false) => {
                self.presented = Some(frame);
                Acquisition::Success
            }
            Ok(true) => {
                self.presented = Some(frame);
                Acquisition::Suboptimal
            }
            Err(error) => {
                // The compositor did not get this frame, so the next present is compared with a
                // frame it never showed.
                self.presented = None;
                return (self.classify(error), 0);
            }
        };
        (answer, damaged)
    }

    /// The answer an acquisition gives for a Vulkan error.
    fn classify(&mut self, error: vk::Result) -> Acquisition {
        match error {
            vk::Result::TIMEOUT | vk::Result::NOT_READY => Acquisition::Timeout,
            vk::Result::ERROR_OUT_OF_DATE_KHR => {
                self.forget();
                Acquisition::Outdated
            }
            vk::Result::ERROR_SURFACE_LOST_KHR => {
                self.forget();
                Acquisition::Lost
            }
            vk::Result::ERROR_DEVICE_LOST => {
                self.gpu.loss().report(
                    wgpu::DeviceLostReason::Unknown,
                    "the device was lost while presenting",
                );
                Acquisition::Validation
            }
            other => {
                tracing::warn!(error = %other, "a swap chain call failed");
                Acquisition::Validation
            }
        }
    }

    /// Waits until the ring entry at `ring` may be used again. Answers `false` if it did not
    /// finish in time.
    fn settle(&mut self, ring: usize) -> bool {
        let entry = &mut self.acquires[ring];
        if !entry.pending {
            return true;
        }
        let fences = [entry.fence];
        // SAFETY: the fence belongs to this device and was handed to a submission.
        let waited = unsafe { self.device.wait_for_fences(&fences, true, FENCE_TIMEOUT) };
        if waited.is_err() {
            tracing::warn!("a swap chain barrier did not finish in time");
            return false;
        }
        // SAFETY: the fence is signalled and no pending submission names it.
        if unsafe { self.device.reset_fences(&fences) }.is_err() {
            return false;
        }
        entry.pending = false;
        true
    }

    /// Waits until the device has finished everything, so that nothing in flight names an image.
    fn idle(&mut self) {
        if let Err(error) = self.gpu.device().poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!(%error, "waiting for the device before a swap chain was replaced");
        }
        // SAFETY: the device is live. Waiting for it to go idle has no other precondition.
        if let Err(error) = unsafe { self.device.device_wait_idle() } {
            tracing::warn!(%error, "waiting for the device before a swap chain was replaced");
        }
        for ring in 0..self.acquires.len() {
            self.settle(ring);
        }
    }

    /// Gives back every image's wgpu texture, command buffers and semaphore.
    ///
    /// Called with the device idle.
    fn release_images(&mut self) {
        for image in self.images.drain(..) {
            let commands = [image.first, image.again, image.release];
            // SAFETY: the device is idle, so none of these is pending, and they belong to this
            // chain's pool and device.
            unsafe {
                self.device.free_command_buffers(self.pool, &commands);
                self.device.destroy_semaphore(image.rendered, None);
            }
            drop(image.texture);
        }
    }

    /// Wraps one image of the swap chain for wgpu, and records the barriers that move it.
    fn wrap(&mut self, raw: vk::Image, config: &ChainConfig) -> Result<Image, String> {
        let size = wgpu::Extent3d {
            width: self.extent.width as u32,
            height: self.extent.height as u32,
            depth_or_array_layers: 1,
        };
        let uses = if self.readable {
            wgpu::TextureUses::COLOR_TARGET | wgpu::TextureUses::COPY_SRC
        } else {
            wgpu::TextureUses::COLOR_TARGET
        };
        let usage = if self.readable {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let hal_descriptor = wgpu::hal::TextureDescriptor {
            label: Some("zgui.native.image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: uses,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: config.view_formats.clone(),
        };
        // SAFETY: the device guard is read through and dropped here. The image belongs to the
        // swap chain and lives until the swap chain is destroyed, after the texture is dropped and
        // the device is idle; the drop callback does nothing, so wgpu never destroys it. The
        // descriptor states the image's own extent, format, usage and view formats, and the swap
        // chain was made mutable-format exactly when there are view formats.
        let hal_texture = unsafe {
            let hal = self
                .gpu
                .device()
                .as_hal::<wgpu::hal::api::Vulkan>()
                .ok_or("the device is not a Vulkan device")?;
            hal.texture_from_raw(
                raw,
                &hal_descriptor,
                Some(Box::new(|| {})),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };
        let descriptor = wgpu::TextureDescriptor {
            label: Some("zgui.native.image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage,
            view_formats: &config.view_formats,
        };
        // SAFETY: the hal texture was made from this device's own handle immediately above and
        // respects `descriptor`. An image just taken from a swap chain is in `UNDEFINED`, which is
        // what `UNINITIALIZED` states.
        let texture = unsafe {
            self.gpu
                .device()
                .create_texture_from_hal::<wgpu::hal::api::Vulkan>(
                    hal_texture,
                    &descriptor,
                    wgpu::TextureUses::UNINITIALIZED,
                )
        };

        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(3);
        // SAFETY: the pool belongs to this device and only this thread allocates from it.
        let commands = unsafe { self.device.allocate_command_buffers(&info) }
            .map_err(|error| format!("allocating the barriers: {error}"))?;
        let (first, again, release) = (commands[0], commands[1], commands[2]);
        record(&self.device, first, raw, Barrier::First)?;
        record(&self.device, again, raw, Barrier::Again)?;
        record(&self.device, release, raw, Barrier::Release)?;
        // SAFETY: the structure is Vulkan's own with its `sType` set.
        let rendered = unsafe {
            self.device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        }
        .map_err(|error| format!("creating a semaphore: {error}"))?;
        Ok(Image {
            texture,
            first,
            again,
            release,
            rendered,
            used: false,
        })
    }

    /// Makes one more entry of the acquire ring.
    fn ring_entry(&self) -> Result<Acquire, String> {
        // SAFETY: the structures are Vulkan's own with their `sType` set.
        let semaphore = unsafe {
            self.device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        }
        .map_err(|error| format!("creating a semaphore: {error}"))?;
        // SAFETY: as above.
        let fence = unsafe {
            self.device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|error| format!("creating a fence: {error}"))?;
        Ok(Acquire {
            semaphore,
            fence,
            pending: false,
        })
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        self.idle();
        self.release_images();
        for acquire in self.acquires.drain(..) {
            // SAFETY: the device is idle, so neither is in use.
            unsafe {
                self.device.destroy_semaphore(acquire.semaphore, None);
                self.device.destroy_fence(acquire.fence, None);
            }
        }
        // SAFETY: the device is idle and every image was released, so nothing uses the swap
        // chain or the pool. The surface is wgpu's and is left alone.
        unsafe {
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_fns.destroy_swapchain(self.swapchain, None);
            }
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}

/// Which of the three barriers of an image a command buffer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Barrier {
    /// The first frame of an image: from `UNDEFINED` to the colour attachment layout.
    First,
    /// Every later frame: from the present layout to the colour attachment layout.
    Again,
    /// After a frame: from the colour attachment layout to the present layout.
    Release,
}

impl Barrier {
    /// The image barrier, for `image`.
    fn barrier(self, image: vk::Image) -> vk::ImageMemoryBarrier<'static> {
        let barrier = vk::ImageMemoryBarrier::default()
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(WHOLE);
        let attachment =
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE;
        match self {
            Self::First => barrier
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(attachment),
            Self::Again => barrier
                .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(attachment),
            Self::Release => barrier
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(attachment | vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::empty()),
        }
    }

    /// The source and destination stages.
    ///
    /// The two acquire barriers start at the colour attachment stage, which is the stage the
    /// acquire semaphore is waited at, so the semaphore, the barrier and the copy after it form
    /// one chain. The release waits for the copy, and for a readback that verifies it.
    fn stages(self) -> (vk::PipelineStageFlags, vk::PipelineStageFlags) {
        match self {
            Self::First | Self::Again => (
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ),
            Self::Release => (
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            ),
        }
    }
}

/// Records `barrier` over `image` into `command`, for simultaneous use.
fn record(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    barrier: Barrier,
) -> Result<(), String> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);
    let (source, destination) = barrier.stages();
    let barriers = [barrier.barrier(image)];
    // SAFETY: the command buffer was allocated from this device's pool and is in the initial
    // state, and the image belongs to this device.
    unsafe {
        device
            .begin_command_buffer(command, &begin)
            .map_err(|error| format!("recording a barrier: {error}"))?;
        device.cmd_pipeline_barrier(
            command,
            source,
            destination,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &barriers,
        );
        device
            .end_command_buffer(command)
            .map_err(|error| format!("recording a barrier: {error}"))
    }
}

/// The Vulkan format of a swap chain format this crate draws into.
fn vk_format(format: wgpu::TextureFormat) -> Option<vk::Format> {
    Some(match format {
        wgpu::TextureFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        wgpu::TextureFormat::Bgra8UnormSrgb => vk::Format::B8G8R8A8_SRGB,
        wgpu::TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        wgpu::TextureFormat::Rgba8UnormSrgb => vk::Format::R8G8B8A8_SRGB,
        wgpu::TextureFormat::Rgb10a2Unorm => vk::Format::A2B10G10R10_UNORM_PACK32,
        wgpu::TextureFormat::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        _ => return None,
    })
}

/// The pixels `rects` cover.
fn damage_area(rects: &[Rect<i32, Device>]) -> u64 {
    rects
        .iter()
        .map(|rect| rect.size.width.max(0) as u64 * rect.size.height.max(0) as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use ash::vk;

    use super::{Barrier, vk_format};

    #[test]
    fn an_image_taken_back_keeps_its_contents() {
        // Only the first frame of an image may discard what it holds. Every later acquire starts
        // from the present layout, which is what the specification keeps the contents across.
        let image = vk::Image::null();
        assert_eq!(
            Barrier::First.barrier(image).old_layout,
            vk::ImageLayout::UNDEFINED
        );
        assert_eq!(
            Barrier::Again.barrier(image).old_layout,
            vk::ImageLayout::PRESENT_SRC_KHR
        );
        assert_eq!(
            Barrier::Release.barrier(image).new_layout,
            vk::ImageLayout::PRESENT_SRC_KHR
        );
    }

    #[test]
    fn the_acquire_barrier_chains_with_the_semaphore_wait() {
        // The acquire semaphore is waited at the colour attachment stage, so the barrier has to
        // start at that stage for the copy after it to be ordered after the semaphore.
        for barrier in [Barrier::First, Barrier::Again] {
            assert!(
                barrier
                    .stages()
                    .0
                    .contains(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            );
        }
    }

    #[test]
    fn the_eight_bit_formats_have_vulkan_formats() {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            assert!(vk_format(format).is_some(), "{format:?}");
        }
    }
}
