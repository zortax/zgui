//! Which frames the device can sample: the format, and the modifier's layout.

use ash::vk;
use zgui_render_wgpu::wgpu;
use zgui_wgpu::SampleDepth;

use super::{DRM_FORMAT_NV12, DRM_FORMAT_P010, DmaBuf, Handles};
use crate::ImportError;

/// One importable four-character code, in Vulkan's and wgpu's spelling.
#[derive(Clone, Copy, Debug)]
pub(super) struct Format {
    /// The Vulkan format of the image, then the formats its two planes are viewed as.
    pub(super) vulkan: [vk::Format; 3],
    /// The wgpu format of the texture.
    pub(super) wgpu: wgpu::TextureFormat,
    /// The code depth, where it is deeper than 8 bits.
    pub(super) depth: Option<SampleDepth>,
}

/// What the device states about an image of one format, modifier and plane arrangement.
#[derive(Clone, Copy, Debug)]
pub(super) struct Support {
    /// Whether memory for the image has to be dedicated to it.
    pub(super) dedicated_only: bool,
}

/// The creation flags of an image of `format`.
///
/// The converter views each plane in its own single-plane format, which a Vulkan image allows
/// only when it is mutable and lists the formats; wgpu-hal gives its own multi-planar images the
/// same two flags.
pub(super) fn flags(disjoint: bool) -> vk::ImageCreateFlags {
    let flags = vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;
    if disjoint {
        flags | vk::ImageCreateFlags::DISJOINT
    } else {
        flags
    }
}

/// The number of planes both formats have.
const PLANES: usize = 2;

impl Format {
    /// The format of `fourcc`, where `features` let wgpu make textures of it.
    pub(super) fn of(fourcc: u32, features: wgpu::Features) -> Result<Self, ImportError> {
        let format = match fourcc {
            DRM_FORMAT_NV12 => Self {
                vulkan: [
                    vk::Format::G8_B8R8_2PLANE_420_UNORM,
                    vk::Format::R8_UNORM,
                    vk::Format::R8G8_UNORM,
                ],
                wgpu: wgpu::TextureFormat::NV12,
                depth: None,
            },
            DRM_FORMAT_P010 => Self {
                vulkan: [
                    vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                    vk::Format::R16_UNORM,
                    vk::Format::R16G16_UNORM,
                ],
                wgpu: wgpu::TextureFormat::P010,
                depth: Some(SampleDepth::P010),
            },
            other => Err(ImportError::Format(
                other
                    .to_le_bytes()
                    .iter()
                    .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
                    .collect(),
            ))?,
        };
        if !features.contains(format.wgpu.required_features()) {
            return Err(ImportError::Missing(match format.wgpu {
                wgpu::TextureFormat::P010 => "P010 textures",
                _ => "NV12 textures",
            }));
        }
        Ok(format)
    }
}

/// Checks that the device samples `format` in `frame`'s modifier, with one memory plane per
/// format plane, as a dma-buf import of `frame`'s size, and answers what it states about it.
pub(super) fn check(
    handles: &Handles,
    format: Format,
    frame: &DmaBuf<'_>,
    disjoint: bool,
) -> Result<Support, ImportError> {
    let refuse = |why: &str| {
        Err(ImportError::Format(format!(
            "modifier {:#018x}, {why}",
            frame.modifier
        )))
    };
    if frame.planes.len() != PLANES {
        return Err(ImportError::Format(format!(
            "{} planes where the format has {PLANES}",
            frame.planes.len()
        )));
    }
    let entries = modifiers(handles, format.vulkan[0]);
    let Some(entry) = entries
        .iter()
        .find(|entry| entry.drm_format_modifier == frame.modifier)
    else {
        return refuse("which the device does not offer for this format");
    };
    if entry.drm_format_modifier_plane_count as usize != PLANES {
        return refuse("which carries compression metadata in memory planes of its own");
    }
    let features = entry.drm_format_modifier_tiling_features;
    if !features.contains(vk::FormatFeatureFlags::SAMPLED_IMAGE) {
        return refuse("which the device cannot sample");
    }
    if disjoint && !features.contains(vk::FormatFeatureFlags::DISJOINT) {
        return refuse("whose planes the device cannot bind to separate dma-bufs");
    }

    let mut list = vk::ImageFormatListCreateInfo::default().view_formats(&format.vulkan);
    let mut layout = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(frame.modifier)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format.vulkan[0])
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::SAMPLED)
        .flags(flags(disjoint))
        .push_next(&mut external)
        .push_next(&mut layout)
        .push_next(&mut list);
    let mut imported = vk::ExternalImageFormatProperties::default();
    let mut properties = vk::ImageFormatProperties2::default().push_next(&mut imported);
    // SAFETY: the physical device comes from this instance, every structure is Vulkan's own with
    // its `sType` set by `default()`, and the view format list outlives the call.
    unsafe {
        handles
            .instance
            .get_physical_device_image_format_properties2(handles.physical, &info, &mut properties)
    }
    .map_err(|_| {
        ImportError::Format(format!(
            "modifier {:#018x}, which the device cannot make a sampled dma-buf image of",
            frame.modifier
        ))
    })?;

    let limits = properties.image_format_properties.max_extent;
    if frame.width > limits.width || frame.height > limits.height {
        return Err(ImportError::Format(format!(
            "{}×{}, larger than the device's {}×{}",
            frame.width, frame.height, limits.width, limits.height
        )));
    }
    let memory = imported.external_memory_properties;
    if !memory
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
        || !memory
            .compatible_handle_types
            .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
    {
        return refuse("which the device cannot import from a dma-buf");
    }
    let dedicated_only = memory
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY);
    if disjoint && dedicated_only {
        return refuse("whose memory has to be dedicated, which a disjoint image cannot have");
    }
    Ok(Support { dedicated_only })
}

/// Every modifier the device offers for `format`.
fn modifiers(handles: &Handles, format: vk::Format) -> Vec<vk::DrmFormatModifierPropertiesEXT> {
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    // SAFETY: the physical device comes from this instance, and both structures are Vulkan's own
    // with their `sType` set by `default()`. The first call only counts.
    unsafe {
        handles.instance.get_physical_device_format_properties2(
            handles.physical,
            format,
            &mut properties,
        )
    };
    let count = list.drm_format_modifier_count as usize;
    let mut entries = vec![vk::DrmFormatModifierPropertiesEXT::default(); count];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default()
        .drm_format_modifier_properties(&mut entries);
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    // SAFETY: as above, and the entry pointer names `entries`, whose length the builder stated.
    unsafe {
        handles.instance.get_physical_device_format_properties2(
            handles.physical,
            format,
            &mut properties,
        )
    };
    let written = list.drm_format_modifier_count as usize;
    entries.truncate(written.min(count));
    entries
}
