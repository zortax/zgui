//! Linux: dma-bufs as one multi-planar Vulkan image over the same memory.
//!
//! VA-API, V4L2 and GStreamer hand decoded frames out as dma-bufs: one or two file descriptors,
//! the offset and row stride of each plane, and the DRM format modifier that names how the memory
//! is tiled. The frame becomes one `NV12` or `P010` Vulkan image, created with exactly that
//! modifier and those plane layouts, bound to memory imported from the descriptors. Only the
//! finished image reaches wgpu, through its hal, and wgpu destroys it through the callback this
//! module gives it.
//!
//! # What has to hold
//!
//! * **The device extensions.** A modifier image needs `VK_EXT_image_drm_format_modifier`, which
//!   wgpu does not enable on its own. A program that imports dma-bufs opens its device with
//!   [`EXTENSIONS`]; the umbrella crate's `App::with_vulkan_extensions` is where they go.
//! * **The decoder's writes.** The importer waits on each descriptor until every write the kernel
//!   knows about has finished, so the image never shows a frame half decoded.
//! * **No compression metadata.** wgpu adopts an image as uninitialised and its first barrier
//!   leaves `UNDEFINED`. Plain and tiled memory keeps its samples through that, but a modifier
//!   with compression metadata (AMD DCC, Intel CCS) may have the metadata reset. Such a modifier
//!   has more memory planes than the format has planes, and the importer refuses it as
//!   [`ImportError::Format`]; the producer then asks its decoder for an uncompressed surface or
//!   falls back to a copy.

mod cache;
mod format;
mod image;
mod wait;

use std::ffi::CStr;
use std::os::fd::BorrowedFd;
use std::sync::{Arc, Mutex};

use ash::{khr, vk};
use zgui_render_wgpu::wgpu;
use zgui_wgpu::{ColorSpace, Planes, VideoFrame};

use crate::ImportError;

/// The Vulkan device extensions a dma-buf import needs.
///
/// Dependency-closed on Vulkan 1.2, which every driver this matters for implements:
/// `VK_EXT_image_drm_format_modifier` requires only extensions that are core there.
pub const EXTENSIONS: [&CStr; 3] = [
    // The `DRM_FORMAT_MODIFIER_EXT` tiling, and stating a frame's modifier and plane layouts.
    c"VK_EXT_image_drm_format_modifier",
    // Importing memory from a file descriptor.
    c"VK_KHR_external_memory_fd",
    // Saying that the descriptor is a dma-buf.
    c"VK_EXT_external_memory_dma_buf",
];

/// A DRM four-character code.
const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*code)
}

/// `DRM_FORMAT_NV12`: 8-bit luma, then interleaved 8-bit Cb and Cr at half size.
pub const DRM_FORMAT_NV12: u32 = fourcc(b"NV12");
/// `DRM_FORMAT_P010`: 10-bit codes in the high bits of 16, laid out as NV12.
pub const DRM_FORMAT_P010: u32 = fourcc(b"P010");
/// `DRM_FORMAT_MOD_LINEAR`: rows one after another, no tiling.
pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;

/// One plane of a dma-buf frame.
#[derive(Clone, Copy, Debug)]
pub struct DmaBufPlane<'a> {
    /// The descriptor the plane's memory is reached through.
    pub fd: BorrowedFd<'a>,
    /// Where the plane starts in that memory, in bytes.
    pub offset: u32,
    /// The distance between the starts of two rows, in bytes.
    pub stride: u32,
}

/// A decoded frame in dma-bufs, as a decoder exports it.
#[derive(Clone, Copy, Debug)]
pub struct DmaBuf<'a> {
    /// The width of the luma plane, in samples.
    pub width: u32,
    /// The height of the luma plane, in samples.
    pub height: u32,
    /// The DRM four-character code: [`DRM_FORMAT_NV12`] or [`DRM_FORMAT_P010`].
    pub fourcc: u32,
    /// The DRM format modifier the memory is laid out in.
    pub modifier: u64,
    /// The luma plane, then the chroma plane.
    pub planes: &'a [DmaBufPlane<'a>],
}

/// The Vulkan handles and extension entry points an import goes through.
pub(crate) struct Handles {
    /// The instance the physical device was enumerated from.
    pub(crate) instance: ash::Instance,
    /// The physical device, for formats and memory types.
    pub(crate) physical: vk::PhysicalDevice,
    /// The device the images and their memory belong to.
    pub(crate) device: ash::Device,
    /// `vkGetMemoryFdPropertiesKHR`, for the memory types a descriptor can be imported as.
    pub(crate) memory_fd: khr::external_memory_fd::Device,
}

/// Imports dma-buf frames on one Vulkan device.
pub struct DmaBufImporter {
    /// The handles every import goes through.
    handles: Handles,
    /// Images already made over a decoder's recycled buffers.
    cache: Mutex<cache::Cache>,
}

impl DmaBufImporter {
    /// An importer for `device`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Backend`] on a device that is not a Vulkan one, and
    /// [`ImportError::Missing`] when one of [`EXTENSIONS`] or wgpu's `NV12` textures is absent.
    pub fn new(device: &wgpu::Device) -> Result<Self, ImportError> {
        if !device
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_NV12)
        {
            return Err(ImportError::Missing("NV12 textures"));
        }
        // SAFETY: the hal device is read for its handles. The clones stay valid while the device
        // lives, which is as long as an importer for it is used.
        let hal =
            unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or(ImportError::Backend)?;
        let enabled = hal.enabled_device_extensions();
        if let Some(missing) = EXTENSIONS.iter().find(|name| !enabled.contains(name)) {
            return Err(ImportError::Missing(
                missing.to_str().unwrap_or("a Vulkan device extension"),
            ));
        }
        let instance = hal.shared_instance().raw_instance().clone();
        let raw = hal.raw_device().clone();
        let memory_fd = khr::external_memory_fd::Device::new(&instance, &raw);
        Ok(Self {
            handles: Handles {
                physical: hal.raw_physical_device(),
                instance,
                device: raw,
                memory_fd,
            },
            cache: Mutex::new(cache::Cache::default()),
        })
    }

    /// Wraps `frame` as a video frame on `device`, the device this importer was made for.
    ///
    /// Waits until the decoder's writes to the frame have finished. `color` is the colour space
    /// the bitstream states, since a dma-buf carries none. `guard` is whatever keeps the decoder
    /// from reusing the buffer, such as its surface handle; the frame holds it until the
    /// conversion pass has read the planes.
    ///
    /// A buffer imported before is answered from a cache keyed by the identity of its memory, so
    /// a decoder cycling through its pool creates each image once.
    ///
    /// # Errors
    ///
    /// [`ImportError::Format`] for a four-character code, a modifier or a plane count the device
    /// cannot sample, and [`ImportError::Platform`] when the decoder's writes do not finish or
    /// the driver refuses a step.
    pub fn import(
        &self,
        device: &wgpu::Device,
        frame: &DmaBuf<'_>,
        color: ColorSpace,
        guard: impl Send + 'static,
    ) -> Result<VideoFrame, ImportError> {
        let format = format::Format::of(frame.fourcc)?;
        format::check(&self.handles, format, frame)?;
        wait::written(frame.planes)?;

        let key = cache::Key::of(frame)?;
        let cached = self.cache().get(&key);
        let texture = match cached {
            Some(texture) => texture,
            None => {
                let disjoint = !key.shared();
                let texture = Arc::new(image::import(
                    &self.handles,
                    device,
                    format,
                    frame,
                    disjoint,
                )?);
                self.cache().insert(key, Arc::clone(&texture));
                texture
            }
        };
        let mut frame = VideoFrame::new(Planes::Multiplanar(texture), color).with_guard(guard);
        if let Some(depth) = format.depth {
            frame = frame.with_depth(depth);
        }
        Ok(frame)
    }

    /// The cache, locked; a poisoned lock is inherited.
    fn cache(&self) -> std::sync::MutexGuard<'_, cache::Cache> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
