//! Asking a device for the Vulkan extensions wgpu does not enable on its own.
//!
//! A buffer a display controller scans out of directly is a Vulkan image created with a DRM format
//! modifier and exported as a dma-buf, and it needs device extensions. A device extension can be
//! enabled only while the device is created — so the list has to be stated before anything exists
//! to state it against.
//!
//! Nothing here creates such an image. What is asserted is the contract around the list: it
//! reaches every device this crate opens, a machine that cannot grant it still gets a working
//! device, the device that replaces a lost one asks for the same list, and what a device enabled
//! is read off the device instead of assumed.

mod support;

use std::ffi::CStr;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Scale, Size};
use zgui_render::{FrameOutcome, RenderTarget, Renderer};
use zgui_render_wgpu::{Gpu, SharedGraphics, WgpuRenderer, wgpu};
use zgui_scene::{Quad, Scene};

use support::{SIDE, device_lock, opaque, present, rect};

/// The names asked for where a Vulkan image with a DRM format modifier becomes a dma-buf.
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
fn the_five_a_scanout_buffer_needs_are_enabled_where_the_driver_has_them() {
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
        "the list is all-or-nothing, so a device reports all five or none"
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
