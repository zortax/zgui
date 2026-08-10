//! Buffers a display scans out of and a renderer draws into, with nothing copied between them.
//!
//! The copied path reads a frame back to system memory and copies it into a buffer the kernel
//! holds: about eight megabytes each way on a 1920 by 1080 display. Neither copy happens when the
//! renderer composes straight into memory the display engine already reads. That memory is a
//! Vulkan image created in a layout the display hardware understands, backed by memory that can
//! leave the device, and handed to the kernel as a file descriptor.
//!
//! **This module makes the images.** Importing the descriptor, registering a framebuffer and
//! flipping to it belong to the display side and are not here.
//!
//! # What has to line up
//!
//! Three things, and each of them fails silently on its own:
//!
//! * **The layout.** The plane says which layouts it can scan out; the graphics driver says which
//!   it can render into and export. `modifier` intersects the two and the driver picks from the
//!   result. A layout only one side knows describes memory the other cannot read.
//! * **The format.** [`FORMAT`] is what a renderer composes into, and it is the format the image
//!   is created in. The two disagreeing exchanges every frame's red and blue.
//! * **The usage.** A supplied presentation copies each frame into the texture through a render
//!   pass, so the image is a colour attachment and the descriptor handed to wgpu says the same.
//!   Four spellings of that one decision sit beside each other in `USAGE`, `HAL_USAGE`,
//!   `IMAGE_USAGE` and `IMAGE_FEATURES`. wgpu checks none of them against the image, so any two
//!   of them disagreeing is undefined behaviour that reports success.
//!
//! # Who destroys what
//!
//! wgpu is told the memory is not its own, so it frees nothing: the image and its memory are this
//! module's to give back. They are given back from the callback wgpu runs when it destroys the
//! texture. That is the one moment where both are true at once: the renderer has let go of the
//! texture, and no submission still names it. The image goes first and the memory after it,
//! because memory a live image is bound to may not be freed.
//!
//! Dropping an [`Imported`] therefore releases the whole buffer. The descriptor closes at once, and
//! the image and its memory go when wgpu reaches the texture. Nothing else has to be called, and
//! nothing is left behind per mode change.
//!
//! The descriptor holds a reference of its own to the memory. So a display still scanning the
//! buffer out keeps that memory alive after the program has freed it.
//!
//! # When this cannot be built
//!
//! [`Unsupported`] says which of four reasons it was: a device that is not a Vulkan one, a Vulkan
//! device extension that was never enabled, a display and a graphics device that share no layout,
//! or a driver that refused a step. None of them stops the program, because the copied path answers
//! every one of them, so each is a value the caller reads and logs.

// Private, because everything either of them publishes is re-exported here and from the crate
// root. A caller reaches `Plane` and `Offered` by one path each, and the split between the two
// files is this module's own business.
mod image;
mod modifier;

use std::ffi::CStr;
use std::fmt;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use ash::{ext, khr, vk};
use zgui_drm::format::Modifier;
use zgui_render_wgpu::{Gpu, wgpu};

pub use crate::import::image::Plane;
pub use crate::import::modifier::Offered;

use crate::import::image::{Handles, Image};
use crate::scanout::FORMAT;

/// The Vulkan device extensions an exported image needs, and what each one is for.
///
/// A device extension can be enabled only while the device is created, so a program that wants
/// this path states these before it opens one — `SharedGraphics::with_extensions` is where they
/// go. A device without them opens and draws exactly as it always did, and [`Imported::create`]
/// answers [`Unsupported::Extension`] naming the one that is absent.
///
/// wgpu-hal enables the last two on any physical device that has them, so on most machines the
/// first is the only name this actually adds. Asking for all three is still what makes the
/// requirement true rather than lucky.
pub const EXTENSIONS: [&CStr; 3] = [
    // The `DRM_FORMAT_MODIFIER_EXT` tiling, the candidate list an image is created from, and
    // reading back which layout the driver chose.
    c"VK_EXT_image_drm_format_modifier",
    // `vkGetMemoryFdKHR`, which turns the image's memory into a descriptor.
    c"VK_KHR_external_memory_fd",
    // Saying that the descriptor is a dma-buf and not an opaque handle. The kernel can import a
    // dma-buf and nothing else here.
    c"VK_EXT_external_memory_dma_buf",
];

/// What a frame is composed into, as wgpu states it.
///
/// A supplied presentation refuses a texture without this, because a frame reaches the texture
/// through a render pass. Nothing else is asked for, and that is a rule in both directions.
/// Asking for less would have the renderer refuse the texture. Asking for more is worse: wgpu
/// validates none of this against the image it is handed, so a texture advertising
/// `TEXTURE_BINDING` over an image created without `SAMPLED` is accepted, is reported as working,
/// and is undefined the first time anything reads it.
const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT;

/// The same decision, as wgpu's hal states it.
///
/// wgpu 29.0.4 reads none of it. `texture_from_raw` takes the label, the format and the copy extent
/// out of the hal descriptor and reads neither the usage nor the memory flags nor the view formats,
/// and wgpu-core maps [`USAGE`] itself for the barriers it records. So this is stated for two
/// reasons: the descriptor has the field, and a field that says something untrue is a trap for
/// whoever reads it next. A wgpu that starts reading it finds it already right.
const HAL_USAGE: wgpu::TextureUses = wgpu::TextureUses::COLOR_TARGET;

/// The same decision again, as Vulkan states it.
///
/// This is what the image is created with and what the layouts were asked about: a layout is
/// renderable for one usage and refused for another, so asking about one usage and creating with
/// another would offer layouts the driver then rejects.
const IMAGE_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::COLOR_ATTACHMENT;

/// What a layout has to be able to do to hold an image of [`IMAGE_USAGE`].
///
/// The fourth spelling of the one decision, and the one that keeps a layout out. A driver publishes
/// what each layout supports for a format, and a layout without this cannot be drawn into whatever
/// else it can do. It travels beside [`IMAGE_USAGE`] because the two are one statement: a usage
/// added to that constant without its feature here would be asked of the driver in one place and
/// left unchecked in the other.
const IMAGE_FEATURES: vk::FormatFeatureFlags = vk::FormatFeatureFlags::COLOR_ATTACHMENT;

/// The Vulkan format [`FORMAT`] is.
///
/// Eight bits a channel, blue first, unsigned normalised. It pairs with the `XRGB8888` fourcc a
/// scanout registers its framebuffers under, which stores its bytes in the same order.
const VK_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;

/// The label the image and the texture carry, for a driver's own diagnostics.
const LABEL: &str = "zgui.scanout";

/// One buffer a display can scan out of and a renderer can draw into.
///
/// Dropping this releases everything it names. See the module documentation for what runs when.
#[derive(Debug)]
pub struct Imported {
    /// What the renderer composes into.
    texture: wgpu::Texture,
    /// The descriptor the kernel imports.
    dmabuf: OwnedFd,
    /// The layout the driver chose.
    modifier: Modifier,
    /// Offset and stride of every memory plane.
    layouts: Vec<Plane>,
}

impl Imported {
    /// Creates `count` buffers of `width` by `height`, each in a layout `scanout` names.
    ///
    /// `scanout` is what the display plane published, which
    /// [`FormatModifiers::modifiers`](zgui_drm::format::FormatModifiers::modifiers) answers for
    /// the fourcc the scanout uses. Every buffer is created from the whole intersection, so the
    /// driver may lay two of them out differently — each one carries the layout it got.
    ///
    /// `gpu` has to be the device the renderer will draw on. A texture belongs to the device that
    /// created it and wgpu states no device on a texture handle, so nothing here can check that,
    /// and a set created on another device is refused much later by the renderer.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported`], which names which of the four reasons it was. Whatever was built
    /// before a refusal is released, so a machine that cannot do this is left as it was found.
    pub fn create(
        gpu: &Gpu,
        scanout: &[Modifier],
        width: u32,
        height: u32,
        count: usize,
    ) -> Result<Vec<Self>, Unsupported> {
        vulkan(gpu, |adapter, device| {
            let candidates = negotiate(adapter, scanout)?;
            let instance = adapter.shared_instance().raw_instance();
            let raw = device.raw_device();
            let handles = Handles {
                instance,
                physical: adapter.raw_physical_device(),
                device: raw,
                modifiers: ext::image_drm_format_modifier::Device::new(instance, raw),
                memory_fd: khr::external_memory_fd::Device::new(instance, raw),
            };

            // A refusal part way through returns, and returning drops the buffers already made:
            // every descriptor closes, and every image and allocation goes back when wgpu reaches
            // the texture. A refusal on the third of three must not leave two of them on the
            // device for the rest of the program.
            let mut buffers = Vec::with_capacity(count);
            for _ in 0..count {
                let image =
                    Image::create(&handles, &candidates, VK_FORMAT, IMAGE_USAGE, width, height)?;
                buffers.push(wrap(gpu, device, image, width, height));
            }
            Ok(buffers)
        })
    }

    /// Returns the layouts this device can render into and export, and this display can scan out.
    ///
    /// What [`Imported::create`] chooses from, answered on its own so that a caller can say in a
    /// log what the two ends agreed on before it allocates anything. The list is what
    /// `VkImageDrmFormatModifierListCreateInfoEXT` is given, and it states no preference: the
    /// driver picks, and [`Imported::modifier`] is what it picked.
    ///
    /// ```no_run
    /// use zgui_drm::format::Modifier;
    /// use zgui_platform_drm::{EXTENSIONS, Imported};
    /// use zgui_render_wgpu::SharedGraphics;
    ///
    /// let graphics = SharedGraphics::with_extensions(EXTENSIONS.to_vec());
    /// let gpu = graphics.open_gpu().expect("a graphics device");
    ///
    /// // What the display plane published, as a scanout reads it back from the card.
    /// let scanout = [Modifier::LINEAR];
    /// let shared = Imported::layouts_shared_with(&gpu, &scanout).expect("a shared layout");
    ///
    /// assert!(shared.iter().all(|offered| scanout.contains(&offered.modifier)));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the same [`Unsupported`] the constructor would, for everything up to the point
    /// where an image is created.
    pub fn layouts_shared_with(
        gpu: &Gpu,
        scanout: &[Modifier],
    ) -> Result<Vec<Offered>, Unsupported> {
        vulkan(gpu, |adapter, _| negotiate(adapter, scanout))
    }

    /// Returns the texture the renderer composes into.
    ///
    /// Cloning it is how a set is handed to a renderer that presents into caller-supplied
    /// textures. The buffer lives until every clone has gone.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Returns the descriptor the kernel imports.
    ///
    /// Borrowed, because this owns it. The descriptor is closed when this is dropped, so a caller
    /// that kept the number instead of the borrow would name whatever the process opened next.
    pub fn dmabuf(&self) -> BorrowedFd<'_> {
        self.dmabuf.as_fd()
    }

    /// Returns the layout the driver chose for this buffer.
    pub fn modifier(&self) -> Modifier {
        self.modifier
    }

    /// Returns where each memory plane starts and how long its rows are.
    ///
    /// One entry per memory plane of [`Imported::modifier`], in order. A framebuffer names one
    /// handle, one offset and one stride for each of them.
    pub fn layouts(&self) -> &[Plane] {
        &self.layouts
    }
}

/// Why a buffer a display can scan out of directly could not be made here.
///
/// Every one of these states an ordinary fact about a machine, and the caller answers all four the
/// same way: keep the copied path. They are told apart because the remedies differ. A missing
/// extension is a program that asked for the wrong thing, and no shared layout is hardware that
/// cannot do this at all.
///
/// ```
/// use zgui_drm::format::Modifier;
/// use zgui_platform_drm::{Offered, Unsupported};
///
/// let refused = Unsupported::NoSharedLayout {
///     vulkan: vec![Offered {
///         modifier: Modifier::LINEAR,
///         planes: 1,
///     }],
///     scanout: vec![Modifier(0x0300_0000_0060_6014)],
/// };
///
/// // A layout is written in hexadecimal, one word wide, wherever a caller logs one.
/// let stated = refused.to_string();
/// assert!(stated.contains("0x0000000000000000"), "{stated}");
/// assert!(stated.contains("0x0300000000606014"), "{stated}");
/// ```
#[derive(Debug)]
pub enum Unsupported {
    /// The graphics device is on another backend, and only Vulkan can export an image here.
    ///
    /// The GL backend is where this comes from. It reaches dma-buf export through EGL and through
    /// an interface nothing in this workspace speaks, so a machine whose only adapter is a GL one
    /// keeps copying every frame.
    Backend(wgpu::Backend),
    /// A Vulkan device extension this path needs was not enabled on the device.
    ///
    /// A device extension can be enabled only while the device is created, so this is reported
    /// long after the point where it could have been fixed. [`EXTENSIONS`] is the list a program
    /// states before it opens one.
    Extension(&'static CStr),
    /// The graphics device and the display plane name no layout in common.
    ///
    /// What a renderer on one card and a display on another gives. Two drivers often share only
    /// the linear layout, and a plane that scans out of no linear buffer then shares nothing.
    NoSharedLayout {
        /// The layouts Vulkan can render into and export.
        vulkan: Vec<Offered>,
        /// The layouts the display plane can scan out.
        scanout: Vec<Modifier>,
    },
    /// The driver would not do a step, or answered something that cannot be stated.
    Driver {
        /// What was being done.
        step: &'static str,
        /// What the driver said, or what about its answer could not be used.
        reason: String,
    },
}

impl fmt::Display for Unsupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(backend) => write!(
                formatter,
                "the graphics device is a {backend:?} one, and only Vulkan can export an image"
            ),
            Self::Extension(name) => write!(
                formatter,
                "the graphics device did not enable {name:?}, which an exported image needs"
            ),
            Self::NoSharedLayout { vulkan, scanout } => {
                let offered = written(vulkan.iter().map(|entry| entry.modifier));
                let published = written(scanout.iter().copied());
                write!(
                    formatter,
                    "the graphics device renders and exports [{offered}] and the display scans \
                     out [{published}], which have no layout in common"
                )
            }
            Self::Driver { step, reason } => write!(formatter, "{step}: {reason}"),
        }
    }
}

/// Returns `layouts` written the way a modifier is written everywhere else: hexadecimal, one word
/// wide.
///
/// The derived spelling is decimal, and a layout in decimal cannot be read at all. The vendor sits
/// in the top byte and what the vendor means by it sits in the rest, and both are lost among
/// sixteen digits.
fn written(layouts: impl Iterator<Item = Modifier>) -> String {
    let codes: Vec<String> = layouts
        .map(|layout| format!("{:#018x}", layout.0))
        .collect();
    codes.join(", ")
}

/// A refusal reads as an error wherever one is expected, which is how a caller logs it.
///
/// It carries no source. What the driver said is a `VkResult`, which is a value and implements no
/// error trait, so it is written into the message where it happened.
impl std::error::Error for Unsupported {}

/// Runs `with` against this device's raw Vulkan handles, or says why they cannot be used.
///
/// Two questions: whether this is a Vulkan device at all, and whether the device extensions an
/// exported image needs are on it. Everything past this point is Vulkan calls that assume both.
///
/// The extension list read is the **device's own**, and not what the program asked for. wgpu-hal
/// enables two of the three on any physical device that reports them, so a device that never asked
/// can still carry them. A device that asked and whose driver refused reports the difference
/// nowhere else.
fn vulkan<T>(
    gpu: &Gpu,
    with: impl FnOnce(&wgpu::hal::vulkan::Adapter, &wgpu::hal::vulkan::Device) -> Result<T, Unsupported>,
) -> Result<T, Unsupported> {
    let backend = gpu.adapter().get_info().backend;
    // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed. Both guards are
    // read through and dropped, which its own documentation permits at any time. The images made
    // through them are created rather than destroyed, and the device outlives every one of them.
    let Some(adapter) = (unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return Err(Unsupported::Backend(backend));
    };
    // SAFETY: as above.
    let Some(device) = (unsafe { gpu.device().as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return Err(Unsupported::Backend(backend));
    };

    let enabled = device.enabled_device_extensions();
    if let Some(missing) = EXTENSIONS.iter().find(|name| !enabled.contains(name)) {
        return Err(Unsupported::Extension(missing));
    }
    with(&adapter, &device)
}

/// Returns the layouts this physical device and `scanout` both accept, or why they share none.
fn negotiate(
    adapter: &wgpu::hal::vulkan::Adapter,
    scanout: &[Modifier],
) -> Result<Vec<Offered>, Unsupported> {
    let offered = modifier::offered(
        adapter.shared_instance().raw_instance(),
        adapter.raw_physical_device(),
        VK_FORMAT,
        IMAGE_USAGE,
        IMAGE_FEATURES,
    );
    let candidates = modifier::intersect(&offered, scanout);
    if candidates.is_empty() {
        return Err(Unsupported::NoSharedLayout {
            vulkan: offered,
            scanout: scanout.to_vec(),
        });
    }
    Ok(candidates)
}

/// Wraps the image as a texture the renderer can draw into.
///
/// The two descriptors state the same image twice, once for wgpu and once for its hal, because
/// wgpu derives the second from the first only for a texture it creates itself.
fn wrap(
    gpu: &Gpu,
    device: &wgpu::hal::vulkan::Device,
    image: Image,
    width: u32,
    height: u32,
) -> Imported {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    // The image and its memory move into the callback, and the callback is the only thing that
    // still names them. wgpu runs it when it destroys the texture, after the renderer has let go
    // and after every submission that named it has finished, which is the one moment both are
    // free. The device handle is cloned and not borrowed: it is a handle and a table of function
    // pointers, and wgpu keeps the device it belongs to alive for as long as any texture on it
    // exists.
    let raw = device.raw_device().clone();
    let (handle, memory) = (image.raw, image.memory);
    let release: wgpu::hal::DropCallback = Box::new(move || {
        // SAFETY: wgpu has destroyed the texture that named this image and nothing else ever held
        // it, so no submission is using it and nothing else will destroy it. The image is
        // destroyed before its memory is freed, as the specification requires of memory an image is
        // bound to. The device outlives this: wgpu holds it for as long as any resource created on
        // it exists, and this runs while it is destroying one.
        unsafe {
            raw.destroy_image(handle, None);
            raw.free_memory(memory, None);
        }
    });

    let hal = wgpu::hal::TextureDescriptor {
        label: Some(LABEL),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: HAL_USAGE,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };
    // SAFETY: `handle` was created on this device with exactly the extent, format, level count,
    // layer count, sample count and usage `hal` states, and it is bound to memory that outlives
    // it. `TextureMemory::External` says the memory is not wgpu's to free, and `release` is what
    // does free it.
    let texture = unsafe {
        device.texture_from_raw(
            handle,
            &hal,
            Some(release),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };

    let descriptor = wgpu::TextureDescriptor {
        label: Some(LABEL),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: USAGE,
        view_formats: &[],
    };
    // SAFETY: the hal texture was created immediately above, on this device, from a descriptor
    // that states the same extent, format and usage as this one. It is handed over once and
    // nothing else names it.
    let texture = unsafe {
        gpu.device()
            .create_texture_from_hal::<wgpu::hal::api::Vulkan>(texture, &descriptor)
    };

    Imported {
        texture,
        dmabuf: image.dmabuf,
        modifier: image.modifier,
        layouts: image.layouts,
    }
}

#[cfg(test)]
mod tests {
    //! The four spellings of one usage, which no device is needed to compare.

    use super::{HAL_USAGE, IMAGE_FEATURES, IMAGE_USAGE, USAGE, VK_FORMAT};
    use crate::scanout::FORMAT;
    use zgui_render_wgpu::wgpu;

    #[test]
    fn the_usage_a_supplied_presentation_requires_is_the_usage_the_image_is_created_with() {
        // Equality in all four, and not containment. Each is read by a different layer and nothing
        // at run time compares them, so the failure is silent in both directions. Too little and
        // the renderer refuses the texture, which at least says so. Too much and wgpu accepts a
        // texture claiming what the image cannot do — `create_texture_from_hal` validates no usage
        // at all — and the first shader that samples it reads whatever is there.
        assert_eq!(
            USAGE,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            "a frame is drawn into this texture, and nothing else is ever done with it"
        );
        assert_eq!(
            HAL_USAGE,
            wgpu::TextureUses::COLOR_TARGET,
            "the hal descriptor says the same about the same image"
        );
        assert_eq!(
            IMAGE_USAGE,
            ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
            "and the driver creates the image from this"
        );
        assert_eq!(
            IMAGE_FEATURES,
            ash::vk::FormatFeatureFlags::COLOR_ATTACHMENT,
            "and a layout is kept only where it can do exactly that"
        );
    }

    #[test]
    fn the_image_is_created_in_the_format_a_frame_is_composed_into() {
        // `FORMAT` is eight bits a channel with blue first, and so is this. The two disagreeing
        // puts every frame on the screen with its red and blue exchanged, and nothing reports it.
        assert_eq!(FORMAT, wgpu::TextureFormat::Bgra8Unorm);
        assert_eq!(VK_FORMAT, ash::vk::Format::B8G8R8A8_UNORM);
    }
}
