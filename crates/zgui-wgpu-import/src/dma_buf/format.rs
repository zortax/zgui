//! Which frames the device can sample: the format, and the modifier's layout.

use ash::vk;
use zgui_render_wgpu::wgpu;
use zgui_wgpu::SampleDepth;

use super::{DRM_FORMAT_NV12, DRM_FORMAT_P010, DmaBuf, Handles};
use crate::ImportError;

/// One importable four-character code, in Vulkan's and wgpu's spelling.
#[derive(Clone, Copy, Debug)]
pub(super) struct Format {
    /// The Vulkan format of the image.
    pub(super) vulkan: vk::Format,
    /// The wgpu format of the texture.
    pub(super) wgpu: wgpu::TextureFormat,
    /// The code depth, where it is deeper than 8 bits.
    pub(super) depth: Option<SampleDepth>,
}

/// The number of planes both formats have.
const PLANES: usize = 2;

impl Format {
    /// The format of `fourcc`.
    pub(super) fn of(fourcc: u32) -> Result<Self, ImportError> {
        match fourcc {
            DRM_FORMAT_NV12 => Ok(Self {
                vulkan: vk::Format::G8_B8R8_2PLANE_420_UNORM,
                wgpu: wgpu::TextureFormat::NV12,
                depth: None,
            }),
            DRM_FORMAT_P010 => Ok(Self {
                vulkan: vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                wgpu: wgpu::TextureFormat::P010,
                depth: Some(SampleDepth::P010),
            }),
            other => Err(ImportError::Format(
                other
                    .to_le_bytes()
                    .iter()
                    .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
                    .collect(),
            )),
        }
    }
}

/// Checks that the device samples `format` in `frame`'s modifier, with one memory plane per
/// format plane.
pub(super) fn check(
    handles: &Handles,
    format: Format,
    frame: &DmaBuf<'_>,
) -> Result<(), ImportError> {
    if frame.planes.len() != PLANES {
        return Err(ImportError::Format(format!(
            "{} planes where the format has {PLANES}",
            frame.planes.len()
        )));
    }
    let entries = modifiers(handles, format.vulkan);
    let Some(entry) = entries
        .iter()
        .find(|entry| entry.drm_format_modifier == frame.modifier)
    else {
        return Err(ImportError::Format(format!(
            "modifier {:#018x}, which the device does not offer for this format",
            frame.modifier
        )));
    };
    if entry.drm_format_modifier_plane_count as usize != PLANES {
        return Err(ImportError::Format(format!(
            "modifier {:#018x}, which carries {} memory planes: compression metadata",
            frame.modifier, entry.drm_format_modifier_plane_count
        )));
    }
    if !entry
        .drm_format_modifier_tiling_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
    {
        return Err(ImportError::Format(format!(
            "modifier {:#018x}, which the device cannot sample",
            frame.modifier
        )));
    }
    Ok(())
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
