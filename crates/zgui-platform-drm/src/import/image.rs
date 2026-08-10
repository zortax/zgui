//! One image, the memory behind it, and the descriptor the kernel imports.
//!
//! This is the Vulkan half of a buffer that reaches the screen without a copy. An image is created
//! in one of the layouts both ends accept, backed by memory that can leave the device, and handed
//! out as a file descriptor together with the layout the driver settled on.
//!
//! # What the driver decides
//!
//! The layout. `VkImageDrmFormatModifierListCreateInfoEXT` carries every candidate and the driver
//! picks one, which is then read back with `vkGetImageDrmFormatModifierPropertiesEXT`. Where each
//! memory plane starts and how long its rows are is read back the same way, because a tiled layout
//! rounds a row up by an amount only the driver knows.
//!
//! # Who releases the image
//!
//! The caller. `Image` hands out the image handle and its memory, and whoever takes them owes
//! both: `vkDestroyImage` first, then `vkFreeMemory`. A step that fails part way through releases
//! what it had already made, so a refusal leaves the device as it was found.

use std::os::fd::{FromRawFd, OwnedFd};

use ash::{ext, khr, vk};
use zgui_drm::format::Modifier;

use crate::import::Unsupported;
use crate::import::modifier::Offered;

/// The memory-plane aspects, in the order a layout's planes are read in.
///
/// Vulkan defines four of them, which is as many memory planes as an image with a DRM format
/// modifier can have. A layout claiming more than four is a driver naming an aspect the
/// specification does not define, and it is refused before anything reads past this table.
const ASPECTS: [vk::ImageAspectFlags; 4] = [
    vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
    vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
    vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
    vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
];

/// Where one memory plane of an image starts, and how long one of its rows is.
///
/// A memory plane is a region of the image's memory rather than a display plane: a layout with
/// several of them keeps different parts of a pixel in different places, and a framebuffer names
/// one offset and one stride for each.
///
/// **Both numbers are 32-bit, and that is the point of the type.** Vulkan answers them as 64-bit
/// values and `drm_mode_fb_cmd2` takes 32-bit ones. The narrowing therefore happens once, here,
/// where it is checked: an image whose layout does not fit is refused when it is built. Nothing
/// downstream holds a 64-bit offset, so nothing downstream can truncate one by accident and put a
/// diagonal on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plane {
    /// How many bytes into the buffer this plane starts.
    offset: u32,
    /// How many bytes one row of this plane takes.
    stride: u32,
}

impl Plane {
    /// Returns how many bytes into the buffer this plane starts.
    pub fn offset(self) -> u32 {
        self.offset
    }

    /// Returns how many bytes one row of this plane takes.
    ///
    /// A tiled layout rounds this up well past the width times the pixel size, so a caller stating
    /// the buffer to the kernel uses this and never works one out.
    pub fn stride(self) -> u32 {
        self.stride
    }

    /// Returns the layout the driver answered, where every field of it fits a framebuffer request.
    ///
    /// Answers `None` for a layout that does not fit. A buffer of that shape exists — a very large
    /// image on a driver that packs several into one allocation — and the kernel has nowhere to
    /// put its offset, so the honest answer is that this buffer cannot be scanned out.
    fn read(layout: &vk::SubresourceLayout) -> Option<Self> {
        Some(Self {
            offset: u32::try_from(layout.offset).ok()?,
            stride: u32::try_from(layout.row_pitch).ok()?,
        })
    }
}

/// The Vulkan handles and extension entry points an exported image is built through.
///
/// Gathered once by the caller and lent to every image it makes. The two loaders resolve one
/// function pointer each, so building them per image would be a `vkGetDeviceProcAddr` per buffer
/// per mode change.
pub(crate) struct Handles<'a> {
    /// The instance the physical device was enumerated from.
    pub(crate) instance: &'a ash::Instance,
    /// The physical device, for the memory type the allocation names.
    pub(crate) physical: vk::PhysicalDevice,
    /// The device the image and its memory belong to.
    pub(crate) device: &'a ash::Device,
    /// `vkGetImageDrmFormatModifierPropertiesEXT`, for reading back which layout was chosen.
    pub(crate) modifiers: ext::image_drm_format_modifier::Device,
    /// `vkGetMemoryFdKHR`, for exporting the memory as a descriptor.
    pub(crate) memory_fd: khr::external_memory_fd::Device,
}

/// One image in a layout a display can scan out, and everything that describes it.
///
/// The image and the memory are handles, so this type states what was made and the caller decides
/// who destroys it. [`Image::create`] is the only thing that makes one, and it releases whatever it
/// had made when a later step is refused.
pub(crate) struct Image {
    /// The image itself.
    pub(crate) raw: vk::Image,
    /// The memory it is bound to, which has to outlive it.
    pub(crate) memory: vk::DeviceMemory,
    /// The layout the driver chose out of the candidates it was given.
    pub(crate) modifier: Modifier,
    /// Where each memory plane starts and how long its rows are.
    pub(crate) layouts: Vec<Plane>,
    /// The descriptor the kernel imports the memory through.
    pub(crate) dmabuf: OwnedFd,
}

impl Image {
    /// Creates one image of `width` by `height`, in one of `candidates`, exported as a dma-buf.
    ///
    /// `usage` is what the image is created with and what `candidates` was gathered for. The two
    /// have to be the same, because a layout is renderable for one usage and not for another.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] naming the step the driver refused, and the same when a
    /// driver answers a layout this cannot state: one that is not among the candidates it was
    /// given, one with more memory planes than Vulkan defines, or one whose offset or stride does
    /// not fit a framebuffer request.
    pub(crate) fn create(
        handles: &Handles<'_>,
        candidates: &[Offered],
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        width: u32,
        height: u32,
    ) -> Result<Self, Unsupported> {
        let codes: Vec<u64> = candidates.iter().map(|entry| entry.modifier.0).collect();
        let raw = image(handles, &codes, format, usage, width, height)?;
        // From here on the image exists, so every later refusal has to give it back. The guard
        // does that, and taking it apart at the end stops it.
        let mut building = Building {
            device: handles.device,
            raw,
            memory: vk::DeviceMemory::null(),
        };
        building.memory = allocate(handles, raw)?;

        let modifier = chosen(handles, raw)?;
        let Some(offer) = candidates.iter().find(|entry| entry.modifier == modifier) else {
            return Err(Unsupported::Driver {
                step: "reading back the layout the image was created in",
                reason: format!(
                    "the driver chose {:#018x}, which was none of the {} it was given",
                    modifier.0,
                    candidates.len()
                ),
            });
        };
        let layouts = layouts(handles, raw, offer.planes)?;
        let dmabuf = export(handles, building.memory)?;

        let (raw, memory) = building.take();
        Ok(Self {
            raw,
            memory,
            modifier,
            layouts,
            dmabuf,
        })
    }
}

/// An image and its memory while the rest of the export is still being built.
///
/// Gives both back when it is dropped. A step refused half way through then leaves the device as it
/// was found, with no orphaned image on it. [`Building::take`] is what a finished export stops it
/// with.
struct Building<'a> {
    /// The device both belong to.
    device: &'a ash::Device,
    /// The image, or null before it exists.
    raw: vk::Image,
    /// The memory, or null before it is allocated.
    memory: vk::DeviceMemory,
}

impl Building<'_> {
    /// Returns the image and its memory, and disarms this guard.
    fn take(mut self) -> (vk::Image, vk::DeviceMemory) {
        let raw = std::mem::replace(&mut self.raw, vk::Image::null());
        let memory = std::mem::replace(&mut self.memory, vk::DeviceMemory::null());
        (raw, memory)
    }
}

impl Drop for Building<'_> {
    fn drop(&mut self) {
        // The image goes first: memory a live image is bound to may not be freed.
        if self.raw != vk::Image::null() {
            // SAFETY: the image was created on this device and never handed to anything else, so
            // nothing else can be using it and nothing else will destroy it.
            unsafe { self.device.destroy_image(self.raw, None) };
        }
        if self.memory != vk::DeviceMemory::null() {
            // SAFETY: the memory was allocated on this device, the only image bound to it was
            // destroyed above, and no descriptor exported from it has been handed out — an export
            // that succeeded is the last step, after which this guard is disarmed.
            unsafe { self.device.free_memory(self.memory, None) };
        }
    }
}

/// Creates the image itself, from the whole candidate list.
fn image(
    handles: &Handles<'_>,
    candidates: &[u64],
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    width: u32,
    height: u32,
) -> Result<vk::Image, Unsupported> {
    let mut list =
        vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(candidates);
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        // One level, one layer and one sample. A supplied presentation refuses anything else, and
        // a display scans out one image of one mip level.
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        // The tiling is a modifier, and `OPTIMAL` would be an arrangement the driver keeps to
        // itself. A display controller reads memory it was given a modifier code for, and the same
        // code goes to the kernel when the framebuffer is registered. Nothing checks that the
        // memory holds what the code names, so a framebuffer registered under a code the image is
        // not in scans out as a scrambled picture with every call reporting success.
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external)
        .push_next(&mut list);

    // SAFETY: every structure in the chain is Vulkan's own with its `sType` set by `default()`,
    // the modifier list points at `candidates` which outlives the call, and the device is the one
    // the handles were gathered from.
    unsafe { handles.device.create_image(&info, None) }
        .map_err(|error| refused("creating the image", error))
}

/// Allocates memory for `raw`: dedicated to it, device-local and exportable as a dma-buf.
///
/// Dedicated because a driver may require it. `VK_EXT_image_drm_format_modifier` permits an
/// implementation to demand a dedicated allocation, and NVIDIA 595.84 reports `DEDICATED_ONLY` for
/// a tiled layout of this format exported as a dma-buf. Device-local because a display engine reads
/// this memory and nothing on the processor ever does.
fn allocate(handles: &Handles<'_>, raw: vk::Image) -> Result<vk::DeviceMemory, Unsupported> {
    // SAFETY: the image was created on this device immediately above.
    let requirements = unsafe { handles.device.get_image_memory_requirements(raw) };
    let Some(index) = memory_type(handles, requirements.memory_type_bits) else {
        return Err(Unsupported::Driver {
            step: "choosing a memory type for the image",
            reason: format!(
                "no device-local memory type is among the {:#x} this image accepts",
                requirements.memory_type_bits
            ),
        });
    };

    let mut export = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(raw);
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(index)
        .push_next(&mut export)
        .push_next(&mut dedicated);

    // SAFETY: the size and the memory type are what the driver just reported for this image, and
    // every structure in the chain is Vulkan's own with its `sType` set by `default()`.
    let memory = unsafe { handles.device.allocate_memory(&info, None) }
        .map_err(|error| refused("allocating the image's memory", error))?;
    // SAFETY: the memory was allocated for this image, is at least as large as the image needs and
    // nothing else is bound to it. The offset is zero, which a dedicated allocation requires.
    unsafe { handles.device.bind_image_memory(raw, memory, 0) }.map_err(|error| {
        // The memory is this function's own until it is bound, and a refusal here means it never
        // was, so it is given back instead of left allocated for the life of the device.
        //
        // SAFETY: nothing is bound to it and no descriptor was exported from it.
        unsafe { handles.device.free_memory(memory, None) };
        refused("binding the image to its memory", error)
    })?;
    Ok(memory)
}

/// Returns the first device-local memory type among `accepted`.
///
/// `accepted` is the bitmask the image reported, so the answer is a type the image can be bound
/// to. A device with no device-local type among them is answered with `None`, and never with a
/// host-visible type: a scanout buffer the processor writes through would give back the copy this
/// whole path exists to remove.
fn memory_type(handles: &Handles<'_>, accepted: u32) -> Option<u32> {
    // SAFETY: the physical device was enumerated from this instance.
    let properties = unsafe {
        handles
            .instance
            .get_physical_device_memory_properties(handles.physical)
    };
    properties
        .memory_types_as_slice()
        .iter()
        .enumerate()
        // The mask is 32 bits wide and Vulkan defines at most 32 memory types, so an index past
        // that names no bit. Shifting by it would be a panic in debug and nothing at all in
        // release.
        .filter(|(index, _)| u32::try_from(*index).is_ok_and(|bit| bit < u32::BITS))
        .find(|(index, kind)| {
            accepted & (1_u32 << index) != 0
                && kind
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .and_then(|(index, _)| u32::try_from(index).ok())
}

/// Returns which layout the driver laid `raw` out in.
fn chosen(handles: &Handles<'_>, raw: vk::Image) -> Result<Modifier, Unsupported> {
    let mut answered = vk::ImageDrmFormatModifierPropertiesEXT::default();
    // SAFETY: the image was created on this device with `DRM_FORMAT_MODIFIER_EXT` tiling, which is
    // what this query requires of it, and the structure is Vulkan's own with its `sType` set.
    unsafe {
        handles
            .modifiers
            .get_image_drm_format_modifier_properties(raw, &mut answered)
    }
    .map_err(|error| refused("reading back the layout the image was created in", error))?;
    Ok(Modifier(answered.drm_format_modifier))
}

/// Returns where each of the image's `planes` memory planes starts, and how long its rows are.
fn layouts(handles: &Handles<'_>, raw: vk::Image, planes: u32) -> Result<Vec<Plane>, Unsupported> {
    let planes = usize::try_from(planes).unwrap_or(usize::MAX);
    let Some(aspects) = ASPECTS.get(..planes) else {
        return Err(Unsupported::Driver {
            step: "reading the image's memory planes",
            reason: format!(
                "the layout claims {planes} memory planes, and Vulkan defines {}",
                ASPECTS.len()
            ),
        });
    };

    aspects
        .iter()
        .enumerate()
        .map(|(index, aspect)| {
            let subresource = vk::ImageSubresource::default()
                .aspect_mask(*aspect)
                .mip_level(0)
                .array_layer(0);
            // SAFETY: the image has `DRM_FORMAT_MODIFIER_EXT` tiling, so a memory plane aspect is
            // legal here, and `aspect` is the aspect of a plane the layout states the image has.
            // The one level and the one layer are what the image was created with.
            let layout = unsafe {
                handles
                    .device
                    .get_image_subresource_layout(raw, subresource)
            };
            Plane::read(&layout).ok_or_else(|| Unsupported::Driver {
                step: "reading the image's memory planes",
                reason: format!(
                    "memory plane {index} starts at {} with a stride of {}, and a framebuffer \
                     states both in 32 bits",
                    layout.offset, layout.row_pitch
                ),
            })
        })
        .collect()
}

/// Exports the memory as a descriptor the kernel can import.
///
/// The descriptor is the application's to close, and it holds a reference of its own to the memory
/// behind it. So it stays usable after `vkFreeMemory`, and the memory lives until every holder has
/// let go. A display can go on scanning a buffer out while the program that made it shuts down.
fn export(handles: &Handles<'_>, memory: vk::DeviceMemory) -> Result<OwnedFd, Unsupported> {
    let info = vk::MemoryGetFdInfoKHR::default()
        .memory(memory)
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    // SAFETY: the memory was allocated on this device with `DMA_BUF_EXT` among its export handle
    // types, as this call requires of it, and the structure is Vulkan's own.
    let raw = unsafe { handles.memory_fd.get_memory_fd(&info) }
        .map_err(|error| refused("exporting the memory as a descriptor", error))?;
    // SAFETY: the driver just created this descriptor for this call and hands it over: the
    // specification states that the application owns it and has to close it. Nothing else in this
    // process holds the number.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// Returns a refusal naming the step the driver would not do, and what it answered.
fn refused(step: &'static str, error: vk::Result) -> Unsupported {
    Unsupported::Driver {
        step,
        reason: format!("the driver answered {error:?}"),
    }
}

#[cfg(test)]
mod tests {
    //! The narrowing, which needs no device.
    //!
    //! A layout arrives from a driver in 64 bits and reaches the kernel in 32. Nothing else in the
    //! path checks it, because nothing else ever sees the wide value.

    use super::Plane;
    use ash::vk;

    /// Returns a layout of `offset` and `row_pitch`, as `vkGetImageSubresourceLayout` fills one in.
    fn layout(offset: u64, row_pitch: u64) -> vk::SubresourceLayout {
        vk::SubresourceLayout {
            offset,
            size: 0,
            row_pitch,
            array_pitch: 0,
            depth_pitch: 0,
        }
    }

    #[test]
    fn a_layout_that_fits_is_carried_across_unchanged() {
        // The ordinary one: a 1920-wide image at four bytes a pixel, 7680 bytes a row.
        let plane = Plane::read(&layout(0, 7680)).expect("an ordinary layout fits");

        assert_eq!(plane.offset(), 0);
        assert_eq!(plane.stride(), 7680);
    }

    #[test]
    fn the_largest_layout_a_framebuffer_can_state_still_fits() {
        let plane = Plane::read(&layout(u64::from(u32::MAX), u64::from(u32::MAX)))
            .expect("the widest values a framebuffer request holds are not refused");

        assert_eq!(plane.offset(), u32::MAX);
        assert_eq!(plane.stride(), u32::MAX);
    }

    #[test]
    fn an_offset_the_kernel_has_nowhere_to_put_is_refused() {
        // One past what fits. A truncated offset would name a place inside the buffer that holds
        // some other part of the picture, and the display would scan out whatever is there.
        assert!(Plane::read(&layout(u64::from(u32::MAX) + 1, 7680)).is_none());
    }

    #[test]
    fn a_stride_the_kernel_has_nowhere_to_put_is_refused() {
        // Truncating a stride is the classic diagonal: every row lands a little further left than
        // the one above it.
        assert!(Plane::read(&layout(0, u64::from(u32::MAX) + 1)).is_none());
    }
}
