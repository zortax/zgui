//! One multi-planar Vulkan image over a dma-buf frame's memory, handed to wgpu.
//!
//! The image states the frame's modifier and each plane's offset and stride explicitly, so the
//! driver reads the memory in the arrangement the decoder wrote it in. Planes in one dma-buf share
//! one dedicated allocation; planes in separate dma-bufs make a disjoint image with one allocation
//! per memory plane. wgpu is told the memory is not its own, and destroys the image and frees the
//! memory through the callback given here once no submission names the texture.

use std::os::fd::{AsRawFd, IntoRawFd};

use ash::vk;
use zgui_render_wgpu::wgpu;

use super::format::{self, Format, Support};
use super::{DmaBuf, DmaBufPlane, Handles};
use crate::ImportError;

/// The aspect of memory plane `index` of a modifier image.
const MEMORY_PLANES: [vk::ImageAspectFlags; 2] = [
    vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
    vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
];

/// A refusal from the driver at `step`.
fn refused(step: &str, error: vk::Result) -> ImportError {
    ImportError::Platform(format!("{step}: {error}"))
}

/// An image and its memory while the import is still being built. Gives both back when dropped.
struct Building<'a> {
    /// The device both belong to.
    device: &'a ash::Device,
    /// The image.
    raw: vk::Image,
    /// The memory bound or about to be bound to it.
    memory: Vec<vk::DeviceMemory>,
}

impl Building<'_> {
    /// Returns the image and its memory, and disarms this guard.
    fn take(mut self) -> (vk::Image, Vec<vk::DeviceMemory>) {
        (
            std::mem::replace(&mut self.raw, vk::Image::null()),
            std::mem::take(&mut self.memory),
        )
    }
}

impl Drop for Building<'_> {
    fn drop(&mut self) {
        // SAFETY: the image and the memory were made on this device and handed to nothing else.
        // The image goes first: memory a live image is bound to may not be freed.
        unsafe {
            if self.raw != vk::Image::null() {
                self.device.destroy_image(self.raw, None);
            }
            for memory in self.memory.drain(..) {
                self.device.free_memory(memory, None);
            }
        }
    }
}

/// Makes the image for `frame` and hands it to wgpu as a texture of `format`.
pub(super) fn import(
    handles: &Handles,
    device: &wgpu::Device,
    format: Format,
    frame: &DmaBuf<'_>,
    disjoint: bool,
    support: Support,
) -> Result<wgpu::Texture, ImportError> {
    let raw = create(handles, format, frame, disjoint)?;
    let mut building = Building {
        device: &handles.device,
        raw,
        memory: Vec::new(),
    };
    if disjoint {
        let mut binds = Vec::new();
        for (index, plane) in frame.planes.iter().enumerate() {
            let mut plane_info =
                vk::ImagePlaneMemoryRequirementsInfo::default().plane_aspect(MEMORY_PLANES[index]);
            let info = vk::ImageMemoryRequirementsInfo2::default()
                .image(raw)
                .push_next(&mut plane_info);
            let mut requirements = vk::MemoryRequirements2::default();
            // SAFETY: the image is a disjoint modifier image of this device with this memory plane.
            unsafe {
                handles
                    .device
                    .get_image_memory_requirements2(&info, &mut requirements)
            };
            let memory = allocate(handles, plane, requirements.memory_requirements, None)?;
            building.memory.push(memory);
            binds.push((memory, MEMORY_PLANES[index]));
        }
        let mut plane_infos: Vec<vk::BindImagePlaneMemoryInfo<'_>> = binds
            .iter()
            .map(|(_, aspect)| vk::BindImagePlaneMemoryInfo::default().plane_aspect(*aspect))
            .collect();
        let infos: Vec<vk::BindImageMemoryInfo<'_>> = binds
            .iter()
            .zip(plane_infos.iter_mut())
            .map(|((memory, _), plane)| {
                vk::BindImageMemoryInfo::default()
                    .image(raw)
                    .memory(*memory)
                    .push_next(plane)
            })
            .collect();
        // SAFETY: each memory was imported for its own plane of this image, and nothing else is
        // bound to the image.
        unsafe { handles.device.bind_image_memory2(&infos) }
            .map_err(|error| refused("binding the planes to their memory", error))?;
    } else {
        let mut dedication = vk::MemoryDedicatedRequirements::default();
        let requirements = {
            let mut requirements = vk::MemoryRequirements2::default().push_next(&mut dedication);
            let info = vk::ImageMemoryRequirementsInfo2::default().image(raw);
            // SAFETY: the image was created on this device immediately above.
            unsafe {
                handles
                    .device
                    .get_image_memory_requirements2(&info, &mut requirements)
            };
            requirements.memory_requirements
        };
        let dedicated = support.dedicated_only
            || dedication.requires_dedicated_allocation == vk::TRUE
            || dedication.prefers_dedicated_allocation == vk::TRUE;
        let memory = allocate(
            handles,
            &frame.planes[0],
            requirements,
            dedicated.then_some(raw),
        )?;
        building.memory.push(memory);
        // SAFETY: the memory was imported for this image alone; the offset is the allocation's
        // start, and each plane's own offset is stated in the image.
        unsafe { handles.device.bind_image_memory(raw, memory, 0) }
            .map_err(|error| refused("binding the image to its memory", error))?;
    }

    let (raw, memory) = building.take();
    let destroyer = handles.device.clone();
    let destroy: wgpu::hal::DropCallback = Box::new(move || {
        // SAFETY: wgpu runs this once, after the last submission naming the texture finished.
        // The image goes first: memory a live image is bound to may not be freed.
        unsafe {
            destroyer.destroy_image(raw, None);
            for memory in memory {
                destroyer.free_memory(memory, None);
            }
        }
    });

    let size = wgpu::Extent3d {
        width: frame.width,
        height: frame.height,
        depth_or_array_layers: 1,
    };
    // SAFETY: the hal device is the one the image was made on, and is released before return.
    let hal_device =
        unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or(ImportError::Backend)?;
    // SAFETY: the image is a 2D, single-level, single-sampled sampled image of `format` and this
    // size, bound to memory wgpu does not own, and destroyed by `destroy` alone.
    let hal = unsafe {
        hal_device.texture_from_raw(
            raw,
            &wgpu::hal::TextureDescriptor {
                label: Some("zgui.video.dma_buf"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: format.wgpu,
                usage: wgpu::TextureUses::RESOURCE,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: Vec::new(),
            },
            Some(destroy),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    drop(hal_device);
    // SAFETY: the texture was made on this device, matches the descriptor, and holds the
    // decoder's finished samples.
    Ok(unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
            hal,
            &wgpu::TextureDescriptor {
                label: Some("zgui.video.dma_buf"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: format.wgpu,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        )
    })
}

/// Creates the image with the frame's modifier and plane layouts.
fn create(
    handles: &Handles,
    format: Format,
    frame: &DmaBuf<'_>,
    disjoint: bool,
) -> Result<vk::Image, ImportError> {
    let layouts: Vec<vk::SubresourceLayout> = frame
        .planes
        .iter()
        .map(|plane| vk::SubresourceLayout {
            offset: u64::from(plane.offset),
            size: 0,
            row_pitch: u64::from(plane.stride),
            array_pitch: 0,
            depth_pitch: 0,
        })
        .collect();
    let mut explicit = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(frame.modifier)
        .plane_layouts(&layouts);
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut list = vk::ImageFormatListCreateInfo::default().view_formats(&format.vulkan);
    let info = vk::ImageCreateInfo::default()
        .flags(format::flags(disjoint))
        .image_type(vk::ImageType::TYPE_2D)
        .format(format.vulkan[0])
        .extent(vk::Extent3D {
            width: frame.width,
            height: frame.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external)
        .push_next(&mut explicit)
        .push_next(&mut list);
    // SAFETY: every structure is Vulkan's own with its `sType` set by `default()`, the plane
    // layouts and view formats outlive the call, and the device stated it makes this image.
    unsafe { handles.device.create_image(&info, None) }
        .map_err(|error| refused("creating the image", error))
}

/// Imports `plane`'s descriptor as memory meeting `requirements`, dedicated to `dedicated` when
/// the driver asks for that.
fn allocate(
    handles: &Handles,
    plane: &DmaBufPlane<'_>,
    requirements: vk::MemoryRequirements,
    dedicated: Option<vk::Image>,
) -> Result<vk::DeviceMemory, ImportError> {
    let mut properties = vk::MemoryFdPropertiesKHR::default();
    // SAFETY: the descriptor is a live dma-buf for the duration of the call, and the output
    // structure is Vulkan's own with its `sType` set by `default()`.
    unsafe {
        handles.memory_fd.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            plane.fd.as_raw_fd(),
            &mut properties,
        )
    }
    .map_err(|error| refused("reading the dma-buf's memory types", error))?;
    let accepted = requirements.memory_type_bits & properties.memory_type_bits;
    if accepted == 0 {
        return Err(ImportError::Platform(
            "no memory type both the image and the dma-buf accept".to_owned(),
        ));
    }
    let index = accepted.trailing_zeros();
    // A dedicated allocation is exactly the image's size. Any other import is the dma-buf's own
    // size, which has to hold everything the image needs.
    let available = rustix::fs::seek(plane.fd, rustix::fs::SeekFrom::End(0)).ok();
    if available.is_some_and(|available| available < requirements.size) {
        return Err(ImportError::Platform(format!(
            "the dma-buf holds {} bytes where the image needs {}",
            available.unwrap_or(0),
            requirements.size
        )));
    }
    let size = match (dedicated, available) {
        (None, Some(available)) => available,
        _ => requirements.size,
    };

    // Vulkan takes ownership of the descriptor it imports, so it is given a duplicate.
    let owned = rustix::io::fcntl_dupfd_cloexec(plane.fd, 0)
        .map_err(|error| ImportError::Platform(format!("duplicating the dma-buf: {error}")))?;
    let mut import = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(owned.as_raw_fd());
    let mut single = vk::MemoryDedicatedAllocateInfo::default();
    let mut info = vk::MemoryAllocateInfo::default()
        .allocation_size(size)
        .memory_type_index(index)
        .push_next(&mut import);
    if let Some(image) = dedicated {
        single = single.image(image);
        info = info.push_next(&mut single);
    }
    // SAFETY: the descriptor is a dma-buf of at least `size` bytes, the memory type is one both
    // the image and the descriptor accept, and every structure is Vulkan's own.
    let memory = unsafe { handles.device.allocate_memory(&info, None) }
        .map_err(|error| refused("importing the dma-buf", error))?;
    // The allocation now owns the duplicate.
    let _ = owned.into_raw_fd();
    Ok(memory)
}
