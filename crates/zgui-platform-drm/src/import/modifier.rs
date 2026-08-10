//! Which layouts a display and a graphics driver both accept.
//!
//! A plane publishes the layouts its hardware can scan out, as the `IN_FORMATS` blob
//! [`Device::plane_formats`](zgui_drm::Device::plane_formats) reads back. Vulkan publishes the
//! layouts it can render into and export, as a list per format plus one question per layout about
//! the image a caller means to create. A buffer that reaches the screen with no copy is in a
//! layout on both lists, and this module puts the two lists together.
//!
//! # The driver chooses
//!
//! Every layout in the intersection goes into `VkImageDrmFormatModifierListCreateInfoEXT` and the
//! driver picks one of them. Nothing here ranks the codes. The six NVIDIA block-linear layouts this
//! machine publishes differ only in block height, which trades address locality against the bytes a
//! row wastes, and only the driver knows how it will lay the image out. The extension says the
//! same: an application holding several modifiers for one format should hand over all of them and
//! let the implementation choose. `vkGetImageDrmFormatModifierPropertiesEXT` then says what was
//! chosen.

use ash::vk;
use zgui_drm::format::Modifier;

/// A layout Vulkan can render into and export, and how many memory planes it stores an image in.
///
/// The plane count comes from the same answer the layout does, and it says how many times an
/// image's offset and stride are read back. Asking for it a second time later would mean matching
/// the chosen code against the list again anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offered {
    /// The DRM format modifier the layout is named by.
    pub modifier: Modifier,
    /// How many memory planes an image in this layout is stored in.
    ///
    /// One for every layout measured so far, tiled and linear alike. A framebuffer names one
    /// handle, one offset and one stride per memory plane, so this is what the layout list an
    /// image reports has to be as long as.
    pub planes: u32,
}

/// Returns the layouts this physical device can render `format` into and export as a dma-buf.
///
/// Two questions per layout, because a driver answers them separately. The format's own list says
/// which layouts exist and what each one can be used for, and that is where a layout lacking
/// `features` is dropped. `vkGetPhysicalDeviceImageFormatProperties2` then says whether an image
/// of `usage` in that layout is possible at all, and whether its memory can leave the device. A
/// layout that is renderable and not exportable would produce an image nothing can scan out of.
///
/// `usage` and `features` describe one image and have to agree. `features` is what a layout must
/// be able to do; `usage` is what the image will be created with. They are separate parameters
/// because Vulkan states them in two vocabularies, and the caller holds both halves side by side.
///
/// # What one driver answers
///
/// A layout carrying no `COLOR_ATTACHMENT` is a layout nothing can be drawn into, and the linear
/// layout is one of them. NVIDIA 595.84 publishes seven layouts for `B8G8R8A8_UNORM`: six
/// block-linear ones carrying `COLOR_ATTACHMENT`, and the linear one, which carries sampling,
/// storage, blit and transfer and no colour attachment. So this answers six of that driver's seven,
/// and linear is the one that goes.
///
/// The same driver answers `vkGetPhysicalDeviceImageFormatProperties2` for linear with
/// `COLOR_ATTACHMENT` usage as though the image were possible. So the second question passes a
/// layout the first refuses, and only the first keeps it out. Handing that layout to the driver
/// among the candidates would ask it to create an image its own format list says nothing can be
/// drawn into.
///
/// Answers an empty list on a driver without `VK_EXT_image_drm_format_modifier`, which reports no
/// layouts and refuses no query. The caller has already checked that the extension is enabled;
/// this stays total so that a driver answering nothing is a refusal with a reason and not a panic
/// inside a frame loop.
pub(crate) fn offered(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    features: vk::FormatFeatureFlags,
) -> Vec<Offered> {
    listed(instance, physical, format)
        .into_iter()
        .filter(|entry| entry.drm_format_modifier_tiling_features.contains(features))
        .filter(|entry| exportable(instance, physical, format, usage, entry.drm_format_modifier))
        .map(|entry| Offered {
            modifier: Modifier(entry.drm_format_modifier),
            planes: entry.drm_format_modifier_plane_count,
        })
        .collect()
}

/// Returns the layouts both `offered` and `scanout` name, in the order Vulkan offered them.
///
/// The order carries no preference, because the driver chooses from whatever list it is given.
/// Vulkan's order is kept for one reason: the plane's side of the answer is a bare code, and
/// Vulkan's side carries the plane count that comes with it.
///
/// An empty answer is the ordinary report that this display and this graphics device share no
/// layout. It is what a machine whose renderer draws on one card and whose display hangs off
/// another gives, and the caller falls back to copying.
pub(crate) fn intersect(offered: &[Offered], scanout: &[Modifier]) -> Vec<Offered> {
    offered
        .iter()
        .copied()
        .filter(|entry| scanout.contains(&entry.modifier))
        .collect()
}

/// Returns every layout the driver lists for `format`, whatever it can be used for.
///
/// The two-call shape every Vulkan enumeration has: the first call fills in the count and the
/// second fills in the entries. The count is read again after the second call, because a driver
/// may write fewer entries than there is room for.
fn listed(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    format: vk::Format,
) -> Vec<vk::DrmFormatModifierPropertiesEXT> {
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    // SAFETY: the physical device comes from this instance, and both structures are Vulkan's own
    // with their `sType` set by `default()`. The call writes only through the pointers in that
    // chain, and the entry pointer is still null here, so it writes the count and nothing else.
    unsafe { instance.get_physical_device_format_properties2(physical, format, &mut properties) };
    let count = list.drm_format_modifier_count;
    if count == 0 {
        return Vec::new();
    }

    let mut entries = vec![vk::DrmFormatModifierPropertiesEXT::default(); count as usize];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default()
        .drm_format_modifier_properties(&mut entries);
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    // SAFETY: as above, and the entry pointer now names `entries`, whose length the same builder
    // wrote into the count field. The driver writes at most that many entries.
    unsafe { instance.get_physical_device_format_properties2(physical, format, &mut properties) };
    let written = list.drm_format_modifier_count as usize;

    entries.truncate(written.min(count as usize));
    entries
}

/// Returns `true` when an image of `usage` in `modifier` is possible and its memory can be
/// exported.
///
/// One call answers both. A driver refuses the query outright for a combination it cannot make,
/// which is the ordinary way a layout drops out, and reports separately whether memory of that
/// shape can be handed to something outside the device. Only `EXPORTABLE` is read here. Importing
/// is what the *kernel* does with the descriptor afterwards, and the DRM device answers for that.
fn exportable(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    modifier: u64,
) -> bool {
    let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(modifier)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let info = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .push_next(&mut external)
        .push_next(&mut drm);

    let mut answered = vk::ExternalImageFormatProperties::default();
    let mut properties = vk::ImageFormatProperties2::default().push_next(&mut answered);
    // SAFETY: the physical device comes from this instance, every structure in both chains is
    // Vulkan's own with its `sType` set by `default()`, and each one lives until after the call.
    // A combination the driver cannot make is reported as an error rather than being undefined.
    let asked = unsafe {
        instance.get_physical_device_image_format_properties2(physical, &info, &mut properties)
    };
    if asked.is_err() {
        return false;
    }

    answered
        .external_memory_properties
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
}

#[cfg(test)]
mod tests {
    //! The intersection, which needs no device, and the filter, which does.
    //!
    //! The intersection decides whether a frame reaches the screen at all: a layout named by only
    //! one of the two sides describes memory the other cannot read, and a display handed one shows
    //! a scrambled picture or nothing.
    //!
    //! The filter before it is checked here and not in `tests/`, because the check is that two
    //! answers from one driver agree and both of them are this module's own. Reaching them from
    //! outside would mean publishing a driver's raw format list, and `ash`'s types with it, so that
    //! a test could read what the code beside it already has.

    use super::{Offered, intersect, listed, offered};
    use crate::import::{IMAGE_FEATURES, IMAGE_USAGE, VK_FORMAT, vulkan};
    use zgui_drm::format::Modifier;
    use zgui_render_wgpu::{Gpu, SharedGraphics};

    /// Returns a device that enabled what an exported image needs, or `None`.
    ///
    /// The graphics is answered beside the device because it owns the instance the device came
    /// from. A machine with neither says so and asserts nothing, which is the shape `cargo xtask
    /// ledger ignored` prescribes.
    fn opened(test: &str) -> Option<(SharedGraphics, std::sync::Arc<Gpu>)> {
        let graphics = SharedGraphics::with_extensions(crate::import::EXTENSIONS.to_vec());
        match graphics.open_gpu() {
            Ok(gpu) => Some((graphics, gpu)),
            Err(failure) => {
                eprintln!("{test}: no usable graphics device, so nothing was asserted: {failure}");
                None
            }
        }
    }

    #[test]
    fn every_layout_offered_is_one_the_driver_said_can_be_drawn_into() {
        // The filter as a whole, which a deletion would take away. A predicate tested on its own
        // survives that: the offered list simply grows by the layouts the driver cannot draw into,
        // every later step still answers, and an image comes out in a layout whose own format entry
        // says nothing can be drawn into it.
        let test = "every_layout_offered_is_one_the_driver_said_can_be_drawn_into";
        let Some((_graphics, gpu)) = opened(test) else {
            return;
        };

        let asked = vulkan(&gpu, |adapter, _| {
            let instance = adapter.shared_instance().raw_instance();
            let physical = adapter.raw_physical_device();
            Ok((
                listed(instance, physical, VK_FORMAT),
                offered(instance, physical, VK_FORMAT, IMAGE_USAGE, IMAGE_FEATURES),
            ))
        });
        let Ok((published, kept)) = asked else {
            eprintln!("{test}: this device exports no image, so nothing was asserted");
            return;
        };

        for entry in &kept {
            let Some(raw) = published
                .iter()
                .find(|listed| listed.drm_format_modifier == entry.modifier.0)
            else {
                panic!(
                    "{:#018x} was offered and the driver never published it",
                    entry.modifier.0
                );
            };
            assert!(
                raw.drm_format_modifier_tiling_features
                    .contains(IMAGE_FEATURES),
                "{:#018x} was offered and the driver says it supports {:?}",
                entry.modifier.0,
                raw.drm_format_modifier_tiling_features
            );
            assert_eq!(
                entry.planes, raw.drm_format_modifier_plane_count,
                "{:#018x} was offered with a plane count the driver did not give it",
                entry.modifier.0
            );
        }

        // Whether this machine can tell the two apart at all. A driver that can draw into
        // everything it publishes leaves the assertion above vacuous, and a run that says so is
        // worth more than a green line that means nothing.
        assert!(
            kept.len() <= published.len(),
            "more layouts were offered than the driver published"
        );
        let refused = published.len() - kept.len();
        eprintln!(
            "{test}: the driver published {} layout(s) for this format and offers {}; {refused} \
             were dropped",
            published.len(),
            kept.len()
        );
    }

    /// Three of the six NVIDIA block-linear layouts this machine publishes for this format.
    ///
    /// They differ only in their last digit, which is the block height.
    const TILED: [Modifier; 3] = [
        Modifier(0x0300_0000_0060_6010),
        Modifier(0x0300_0000_0060_6014),
        Modifier(0x0300_0000_0060_6015),
    ];

    /// Returns what Vulkan offering `modifiers` looks like, each in one memory plane.
    fn offering(modifiers: &[Modifier]) -> Vec<Offered> {
        modifiers
            .iter()
            .map(|modifier| Offered {
                modifier: *modifier,
                planes: 1,
            })
            .collect()
    }

    /// Returns the codes of `offered`, which a table states its expectation as.
    fn codes(offered: &[Offered]) -> Vec<Modifier> {
        offered.iter().map(|entry| entry.modifier).collect()
    }

    #[test]
    fn a_layout_both_sides_name_is_the_answer() {
        // The measured case, and the one that matters: the plane and the driver both publish the
        // whole family, and every one of them is a candidate.
        let table: &[(&[Modifier], &[Modifier], &[Modifier])] = &[
            (
                &[TILED[0], TILED[1], Modifier::LINEAR],
                &[TILED[0], TILED[1], Modifier::LINEAR],
                &[TILED[0], TILED[1], Modifier::LINEAR],
            ),
            (
                &[TILED[0], TILED[1], Modifier::LINEAR],
                &[TILED[1], Modifier::LINEAR],
                &[TILED[1], Modifier::LINEAR],
            ),
            (
                &[Modifier::LINEAR],
                &[Modifier::LINEAR],
                &[Modifier::LINEAR],
            ),
        ];

        for (vulkan, scanout, expected) in table {
            assert_eq!(
                codes(&intersect(&offering(vulkan), scanout)),
                *expected,
                "{vulkan:?} against {scanout:?}"
            );
        }
    }

    #[test]
    fn a_layout_only_one_side_names_is_left_out() {
        // Each side has one the other does not, and neither may reach an image. A layout only the
        // driver has produces memory the display engine cannot read; one only the plane has is a
        // layout the driver cannot be asked for.
        let vulkan = offering(&[TILED[0], TILED[1]]);
        let scanout = [TILED[1], TILED[2]];

        assert_eq!(codes(&intersect(&vulkan, &scanout)), [TILED[1]]);
    }

    #[test]
    fn two_lists_with_nothing_in_common_answer_nothing() {
        // A renderer on one card and a display on another. Both sides publish plenty and share
        // none of it, and the answer says so rather than picking the nearest.
        let vulkan = offering(&[TILED[0], TILED[1]]);
        let scanout = [Modifier::LINEAR, TILED[2]];

        assert!(intersect(&vulkan, &scanout).is_empty());
    }

    #[test]
    fn an_empty_side_answers_nothing() {
        let vulkan = offering(&[TILED[0], Modifier::LINEAR]);

        assert!(
            intersect(&vulkan, &[]).is_empty(),
            "a plane that publishes no layout for this format takes nothing this way"
        );
        assert!(
            intersect(&[], &[TILED[0], Modifier::LINEAR]).is_empty(),
            "a driver that offers no layout can render into none of them"
        );
        assert!(intersect(&[], &[]).is_empty());
    }

    #[test]
    fn the_order_is_the_one_vulkan_offered() {
        // The plane's order is the order its blob happens to list, and the driver's is the order
        // its own answer came in. The second is what the plane count rides along with, so it is
        // the one kept — and a caller reading the first entry as "preferred" would be reading a
        // preference neither side stated.
        let vulkan = offering(&[TILED[2], TILED[0], TILED[1]]);
        let scanout = [TILED[0], TILED[1], TILED[2]];

        assert_eq!(
            codes(&intersect(&vulkan, &scanout)),
            [TILED[2], TILED[0], TILED[1]]
        );
    }

    #[test]
    fn the_plane_count_travels_with_the_layout_it_belongs_to() {
        // The count decides how many times an image's offset and stride are read back. Carrying
        // the wrong one reads a memory plane that does not exist, or leaves one unstated in the
        // framebuffer.
        let vulkan = vec![
            Offered {
                modifier: TILED[0],
                planes: 1,
            },
            Offered {
                modifier: TILED[1],
                planes: 2,
            },
        ];

        assert_eq!(
            intersect(&vulkan, &[TILED[1]]),
            [Offered {
                modifier: TILED[1],
                planes: 2,
            }]
        );
    }
}
