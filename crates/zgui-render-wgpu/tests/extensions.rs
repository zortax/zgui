//! Asking a device for the Vulkan extensions wgpu does not enable on its own.
//!
//! A buffer a display controller scans out of directly is a Vulkan image created with a DRM format
//! modifier and exported as a dma-buf, and it needs device extensions. A device extension can be
//! enabled only while the device is created — so the list has to be stated before anything exists
//! to state it against.
//!
//! Nothing here creates such an image. What is asserted is the contract around the list: it
//! reaches every device this crate opens, a machine that cannot grant it still gets a working
//! device, and the device that replaces a lost one asks for the same list. None of that depends on
//! how long the list is, and the five below are the five the console backend's own constant holds.
//!
//! What is **not** asserted is that a driver enabled them. Vulkan offers no call that reads back
//! which device extensions a device enabled, so the list reached through wgpu's hal below is
//! wgpu-hal's own record of what it handed `vkCreateDevice` rather than anything the driver said. A
//! driver refusing a name shows up as `vkCreateDevice` failing, which the crate reports as a device
//! it could not open. Reading the record still covers this crate's own plumbing: that the names
//! asked for reach the call at all.

// Reaching wgpu-hal's record needs its hal, and reaching that is unsafe.
#![allow(
    unsafe_code,
    reason = "wgpu-hal's record of the extension list is reached through its hal"
)]

mod support;

use std::ffi::CStr;
use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Scale, Size};
use zgui_render::{FrameOutcome, RenderTarget, Renderer};
use zgui_render_wgpu::gpu::adapter;
use zgui_render_wgpu::{Gpu, SharedGraphics, WgpuRenderer, wgpu};
use zgui_scene::{Quad, Scene, SubpixelSprite};

use support::{SIDE, device_lock, opaque, present, rect};

/// The names the console path asks for, in this file's own order: the image, the descriptor it is
/// exported through, the dma-buf statement on that descriptor, the semaphore a finished frame
/// signals, and the queue-family release that hands the image over.
const DMA_BUF: [&CStr; 5] = [
    c"VK_EXT_image_drm_format_modifier",
    c"VK_KHR_external_memory_fd",
    c"VK_EXT_external_memory_dma_buf",
    c"VK_KHR_external_semaphore_fd",
    c"VK_EXT_queue_family_foreign",
];

/// A name the registry does not hold.
///
/// `ZGUI` is not among the author tags recorded in `vk.xml`, so no registered extension is called
/// this. A test of the refusal path needs a request a physical device is not going to grant, and
/// this is as close as a name can come.
const ABSENT: &CStr = c"VK_ZGUI_not_a_real_extension";

/// The format every renderer here presents in.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Returns a target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0))
}

/// Returns the shared device, or `None` when this machine has none.
///
/// Skipping out loud instead of failing: these tests assert what a device does, and a machine
/// without one has nothing to say about it.
fn open(graphics: &SharedGraphics) -> Option<Arc<Gpu>> {
    match graphics.open_gpu() {
        Ok(gpu) => Some(gpu),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            for candidate in &failure.candidates {
                eprintln!("    {}: {}", candidate.name, candidate.reason);
            }
            None
        }
    }
}

/// Returns a renderer on the shared device, or `None` when this machine has none.
fn renderer(graphics: &SharedGraphics) -> Option<WgpuRenderer> {
    match graphics.renderer_offscreen(target(), FORMAT, false) {
        Ok(renderer) => Some(renderer),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            None
        }
    }
}

/// Returns a scene holding one quad of `colour` over the whole target.
fn filled(colour: Color) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene.paints.add(zgui_scene::Paint::Solid(colour));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    scene
}

/// Returns the Vulkan device extensions wgpu-hal recorded handing `vkCreateDevice`.
///
/// Apart from this crate's own record of what it asked for, and useful for that reason: the two
/// agreeing says the names reached the call. Neither is the driver's answer — Vulkan has no call
/// that gives one. `None` where the device is not a Vulkan one.
#[cfg(vulkan_hal)]
fn enabled_on(gpu: &Gpu) -> Option<Vec<&'static CStr>> {
    // SAFETY: the guard is read through and dropped. Nothing here destroys the device or anything
    // reachable from it, which is all `as_hal` asks of a caller.
    let hal = unsafe { gpu.device().as_hal::<wgpu::hal::api::Vulkan>() }?;
    Some(hal.enabled_device_extensions().to_vec())
}

/// The same, on a target whose wgpu has no Vulkan backend to read one off.
#[cfg(not(vulkan_hal))]
fn enabled_on(_gpu: &Gpu) -> Option<Vec<&'static CStr>> {
    None
}

/// Returns whether `gpu` enabled any of them, and says on the run's output where it did not.
///
/// A machine without the extensions still has something to say — that the refusal is reported and
/// the device is open — so the tests below read this and stop instead of failing.
fn granted(gpu: &Gpu) -> bool {
    if gpu.vulkan_extensions().is_empty() {
        eprintln!(
            "reported: {} grants none of the dma-buf extensions",
            gpu.describe()
        );
        return false;
    }
    true
}

#[test]
fn a_name_no_driver_has_leaves_the_device_open_and_says_nothing_was_enabled() {
    let _device = device_lock();
    let graphics = SharedGraphics::with_extensions(vec![ABSENT]);
    assert_eq!(
        graphics.extensions(),
        [ABSENT],
        "the graphics forgot what it was asked for"
    );

    let Some(gpu) = open(&graphics) else {
        return;
    };
    // The whole contract in one line: a request that cannot be granted costs the capability and
    // nothing else. Refusing to open at all would take a program that wanted a fast path and give
    // it no graphics whatsoever.
    assert!(
        gpu.vulkan_extensions().is_empty(),
        "a name no driver has was reported as enabled: {:?}",
        gpu.vulkan_extensions()
    );

    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    assert_eq!(
        present(&mut renderer, &filled(opaque(255, 0, 0))).rgba(SIDE / 2, SIDE / 2),
        [255, 0, 0, 255],
        "the device that survived the refusal cannot draw"
    );
}

#[test]
fn one_absent_name_costs_the_whole_list() {
    let _device = device_lock();
    // Four a driver may have and one no driver has. A device that enabled the four would report a
    // capability the caller reads as the whole list, and the image it then creates would fail deep
    // in a frame with an error that names a format.
    let mut mixed = DMA_BUF.to_vec();
    mixed[2] = ABSENT;
    let graphics = SharedGraphics::with_extensions(mixed);
    let Some(gpu) = open(&graphics) else {
        return;
    };

    assert!(
        gpu.vulkan_extensions().is_empty(),
        "part of a list was enabled and reported: {:?}",
        gpu.vulkan_extensions()
    );
}

#[test]
fn the_names_a_scanout_buffer_needs_are_enabled_where_the_driver_has_them() {
    let _device = device_lock();
    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(gpu) = open(&graphics) else {
        return;
    };
    if !granted(&gpu) {
        return;
    }

    assert_eq!(
        gpu.vulkan_extensions(),
        DMA_BUF.as_slice(),
        "the list is all-or-nothing, so a device reports every name or none"
    );
    assert_eq!(
        gpu.adapter().get_info().backend,
        wgpu::Backend::Vulkan,
        "only a Vulkan device can have enabled a Vulkan device extension"
    );
    eprintln!(
        "reported: {} enabled {:?}",
        gpu.describe(),
        gpu.vulkan_extensions()
    );

    // This device was created through wgpu's hal, so that it draws at all is the assertion.
    // Everything the renderer builds — pipelines, buffers, the composed target — is built on it
    // here.
    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    assert_eq!(
        present(&mut renderer, &filled(opaque(0, 128, 255))).rgba(SIDE / 2, SIDE / 2),
        [0, 128, 255, 255],
        "a device opened through the hal composes nothing"
    );
}

#[test]
fn an_empty_list_opens_the_device_it_always_did() {
    let _device = device_lock();
    let graphics = SharedGraphics::new();
    assert!(
        graphics.extensions().is_empty(),
        "the ordinary constructor asks for an extension"
    );

    let Some(gpu) = open(&graphics) else {
        return;
    };
    assert!(
        gpu.vulkan_extensions().is_empty(),
        "a device nothing asked anything of reported an extension: {:?}",
        gpu.vulkan_extensions()
    );

    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    assert_eq!(
        present(&mut renderer, &filled(opaque(0, 255, 0))).rgba(SIDE / 2, SIDE / 2),
        [0, 255, 0, 255]
    );
}

#[test]
fn the_hal_path_adds_the_list_to_the_device_and_nothing_else() {
    let _device = device_lock();
    // The ordinary path's device, read off the driver and then released, so this machine holds one
    // device at a time exactly as a program does.
    let Some(ordinary) = ({
        let graphics = SharedGraphics::new();
        open(&graphics).and_then(|gpu| enabled_on(&gpu))
    }) else {
        eprintln!("skipped: no Vulkan device to read an extension list off");
        return;
    };

    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(gpu) = open(&graphics) else {
        return;
    };
    if !granted(&gpu) {
        return;
    }
    let extended = enabled_on(&gpu).expect("a device that enabled them is a Vulkan device");

    // wgpu-hal's own list, read off the device.
    for name in DMA_BUF {
        assert!(
            extended.contains(&name),
            "{name:?} was reported as enabled and is not on the device: {extended:?}"
        );
    }
    // And the device is the ordinary one plus that list and nothing else. wgpu-hal derives the
    // rest of the extension list from the feature set it is handed, so a hal path that derived one
    // of its own — the adapter's whole set instead of the descriptor's — moves names into or out
    // of this list. Nothing in wgpu's own API can see that: `Device::features` is read back off
    // the descriptor and never off the device, so a device opened with the wrong features still
    // reports the right ones.
    let lost: Vec<&CStr> = ordinary
        .iter()
        .copied()
        .filter(|name| !extended.contains(name))
        .collect();
    assert!(
        lost.is_empty(),
        "the hal path opened a device without {lost:?}, which the ordinary path enables"
    );
    let uninvited: Vec<&CStr> = extended
        .iter()
        .copied()
        .filter(|name| !ordinary.contains(name) && !DMA_BUF.contains(name))
        .collect();
    assert!(
        uninvited.is_empty(),
        "the hal path put {uninvited:?} on the device, which nothing asked for"
    );

    // Two of them are already on the ordinary device: wgpu-hal enables
    // `VK_KHR_external_memory_fd` and `VK_EXT_external_memory_dma_buf` on any physical device that
    // has them. So the names this adds are the rest, and the callback skips a name the list
    // already holds.
    let added: Vec<&CStr> = DMA_BUF
        .iter()
        .copied()
        .filter(|name| !ordinary.contains(name))
        .collect();
    eprintln!(
        "reported: the ordinary device already had {} of them; this added {added:?}",
        DMA_BUF.len() - added.len()
    );
}

/// The tile's side, in texels.
const TILE: i32 = 16;

/// Returns a tile whose three channels carry different coverage, as per-channel text does.
fn subpixel_ramp() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((TILE * TILE * 4) as usize);
    for _ in 0..TILE {
        for x in 0..TILE {
            let base = (255 * x / (TILE - 1)) as u8;
            bytes.extend_from_slice(&[base, base.saturating_add(40), base.saturating_add(80), 255]);
        }
    }
    bytes
}

#[test]
fn a_device_opened_through_the_hal_still_blends_against_a_second_colour_output() {
    let _device = device_lock();
    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    if !granted(renderer.gpu()) {
        return;
    }
    if !renderer.capabilities().subpixel_text {
        eprintln!("reported: this device has no dual-source blending to exercise");
        return;
    }

    // Dual-source blending is a `VkPhysicalDeviceFeatures` bit, enabled from the feature set the
    // device was created with. wgpu reports a device's features off the descriptor, so asking the
    // device answers the descriptor and proves nothing; this draws with it instead. It is a floor:
    // a driver that enforces the bit fails here when the hal path opens with a feature set of its
    // own, and NVIDIA 595.84 was measured not to enforce it.
    let mut atlas = Atlas::new(AtlasLimits::default());
    let tile = atlas
        .get_or_insert(
            AtlasKey::new(1, TextureKind::Subpixel),
            Size::new(TILE, TILE),
            subpixel_ramp,
        )
        .expect("one small tile fits in a fresh atlas");
    atlas
        .flush_uploads(renderer.atlas())
        .expect("the device accepts the upload");

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    scene.push_subpixel_sprite(SubpixelSprite::new(
        rect(0.0, 0.0, TILE as f32, TILE as f32),
        tile,
        opaque(0, 0, 0),
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let sample = pixels.rgba(TILE / 2, TILE / 2);
    assert!(
        sample[0] != sample[2],
        "the hal device claims per-channel coverage and drew {sample:?}"
    );
}

#[test]
fn an_adapter_that_grants_the_list_is_preferred_over_one_that_does_not() {
    let _device = device_lock();
    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(gpu) = open(&graphics) else {
        return;
    };
    if granted(&gpu) {
        // The loop found one, which is the property. Nothing further to prove here.
        return;
    }

    // It settled for a device without them, so no adapter can have had them — the candidate loop
    // accepts the first adapter that opens and presents, and an adapter lacking the names still
    // opens. Every adapter is opened on its own here and asked. A machine where one of them says
    // yes is a machine where the loop chose the wrong one and copies every frame through the
    // processor for the rest of the process, with one log line to say so.
    let instance = graphics.instance().clone();
    for tier in adapter::tiers(graphics.backends()) {
        for candidate in adapter::candidates(&instance, tier) {
            let name = adapter::describe(&candidate.get_info());
            match Gpu::open(instance.clone(), candidate, &DMA_BUF) {
                Ok(other) => assert!(
                    other.vulkan_extensions().is_empty(),
                    "{name} grants the list and the candidate loop settled for a device without it"
                ),
                Err(reason) => eprintln!("    {name}: {reason}"),
            }
        }
    }
}

#[test]
fn enabling_them_changes_no_feature_and_no_limit_of_the_device() {
    let _device = device_lock();
    // Read off a device opened the ordinary way, then released before the second one is opened, so
    // this machine holds one device at a time exactly as a program does.
    let Some((features, limits, capabilities)) = ({
        let ordinary = SharedGraphics::new();
        open(&ordinary).map(|gpu| {
            (
                gpu.device().features(),
                gpu.device().limits(),
                gpu.capabilities(),
            )
        })
    }) else {
        return;
    };

    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(gpu) = open(&graphics) else {
        return;
    };
    if !granted(&gpu) {
        return;
    }

    // The hal path creates the device itself. Deriving its features or its limits separately would
    // hand an adapter capabilities the ordinary path never grants it — subpixel text on one path
    // and not the other, a texture size that differs by a power of two — with nothing anywhere to
    // report the difference. The two paths read one derivation, and this test says so.
    assert_eq!(
        gpu.device().features(),
        features,
        "the device with the extensions has a different feature set"
    );
    assert_eq!(
        gpu.device().limits(),
        limits,
        "the device with the extensions has different limits"
    );
    assert_eq!(
        gpu.capabilities(),
        capabilities,
        "the device with the extensions reports different capabilities"
    );
}

#[test]
fn the_device_that_replaces_a_lost_one_enables_what_the_lost_one_did() {
    let _device = device_lock();
    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    let before: Vec<&CStr> = renderer.gpu().vulkan_extensions().to_vec();
    let dead = Arc::as_ptr(renderer.gpu());

    // A driver reports a loss on whatever thread it likes and the frame loop reads the flag, so
    // this is what a real loss delivers to the renderer.
    renderer
        .gpu()
        .loss()
        .report(wgpu::DeviceLostReason::Unknown, "injected by a test");
    let scene = filled(opaque(255, 255, 255));
    assert!(matches!(
        renderer.draw(&scene, &DamageSet::full()),
        FrameOutcome::Recovered
    ));
    assert_ne!(
        Arc::as_ptr(renderer.gpu()),
        dead,
        "the renderer is still on the device that died"
    );

    // The one that matters. A replacement that quietly dropped the list would leave a program
    // running on a device its buffers cannot be imported into, with every frame after the loss
    // failing somewhere far away from here.
    assert_eq!(
        renderer.gpu().vulkan_extensions(),
        before.as_slice(),
        "the replacement device did not enable what the dead one had"
    );
    assert_eq!(
        graphics.extensions(),
        DMA_BUF.as_slice(),
        "the loss changed what the graphics asks for"
    );
    if before.is_empty() {
        eprintln!(
            "reported: this machine grants none of them, so the equality above holds between two \
             empty lists"
        );
    }
    assert_eq!(
        present(&mut renderer, &scene).rgba(SIDE / 2, SIDE / 2),
        [255, 255, 255, 255],
        "the rebuilt renderer draws nothing"
    );
}

#[test]
fn a_backend_that_is_not_vulkan_opens_a_device_and_reports_none_of_them() {
    let _device = device_lock();
    // GL is the backend this crate keeps as a fallback, and it has no notion of a Vulkan device
    // extension. Asking a GL adapter for one is answered.
    let graphics =
        SharedGraphics::with_backends_and_extensions(wgpu::Backends::GL, DMA_BUF.to_vec());
    let Some(gpu) = open(&graphics) else {
        return;
    };

    assert_ne!(gpu.adapter().get_info().backend, wgpu::Backend::Vulkan);
    assert!(
        gpu.vulkan_extensions().is_empty(),
        "a device on {:?} claimed a Vulkan device extension",
        gpu.adapter().get_info().backend
    );
    eprintln!("reported: {} enabled none of them", gpu.describe());

    let Some(mut renderer) = renderer(&graphics) else {
        return;
    };
    assert_eq!(
        present(&mut renderer, &filled(opaque(255, 0, 255))).rgba(SIDE / 2, SIDE / 2),
        [255, 0, 255, 255],
        "the fallback backend answered the request and then drew nothing"
    );
}

#[test]
fn a_renderer_asked_for_first_lands_on_a_device_that_carries_the_list() {
    let _device = device_lock();
    // The other tests take `open_gpu` first, as a caller supplying its own textures has to. This
    // is the other order — a renderer before any device exists — and it opens the device through a
    // path of its own. Both read the one list on the graphics.
    let graphics = SharedGraphics::with_extensions(DMA_BUF.to_vec());
    let Some(renderer) = renderer(&graphics) else {
        return;
    };
    let opened = graphics
        .open_gpu()
        .expect("a device that opened for a renderer is the device this answers");

    assert!(
        Arc::ptr_eq(renderer.gpu(), &opened),
        "the renderer and the shared device are two devices"
    );
    assert_eq!(
        renderer.gpu().vulkan_extensions(),
        opened.vulkan_extensions(),
        "one device answered two different lists"
    );
    if granted(&opened) {
        assert_eq!(renderer.gpu().vulkan_extensions(), DMA_BUF.as_slice());
    }
}
