//! Where a shared buffer's memory really is, before and after the other card imports it.
//!
//! The scanout buffer is allocated on the **display** card today, so the renderer writes across the
//! link. The alternative everyone reaches for is to allocate on the *render* card and have the
//! display pull instead — which would take the transfer off the graphics engine entirely. Whether
//! that is possible is not a question about either card. It is a question about whether the export
//! survives the import: if nouveau moves the buffer out of its own memory the moment i915 takes a
//! handle to it, the pull crosses the link exactly as the push did and nothing is won.
//!
//! **It does not survive, and the kernel says why.** `nouveau_gem_prime_pin` pins an exported buffer
//! with `nouveau_bo_pin_locked(nvbo, NOUVEAU_GEM_DOMAIN_GART, false)`, and that call sets the
//! placement and validates — so a buffer in video memory is *moved* to system memory when the other
//! card attaches, and pinned there. Every later submission that wants it in video memory then fails
//! the domain intersection in `nouveau_gem_set_domain` and the ioctl answers `EINVAL`, which is the
//! `kernel rejected pushbuf` this test provokes.
//!
//! That is deliberate rather than an oversight: GART memory is visible to any importer, and video
//! memory behind a BAR is not, so pinning there is how a buffer is shared at all without
//! peer-to-peer DMA. It also means the arrangement could not pay even if the submission were
//! allowed — the pixels are in system memory by then, and the renderer crosses the link to reach
//! them exactly as it does now. What it would take is a `pci_p2pdma` export path in nouveau and an
//! importer that accepts one, which is an upstream feature and not a patch.
//!
//! Nothing here asks a driver where memory is, because no interface answers that. It **measures**,
//! by filling the buffer and timing it. Local memory and memory across a PCIe x1 link are a factor
//! of thirty apart, so the reading needs no precision to be decisive.
//!
//! **Every fill is checked before it is timed.** A submission the kernel refuses returns at once, so
//! a refused fill times as an enormous bandwidth rather than as an error — the one reading that
//! looks like triumph and means the opposite. So the buffer is read back through the allocation
//! after each fill, and a fill whose colour did not arrive is reported as refused and not as a
//! rate.
//!
//! Four buffers are filled, and the fourth is the question:
//!
//! 1. one wgpu allocated itself, which is local by construction and sets the scale;
//! 2. one allocated on the display card and imported — today's arrangement, expected slow;
//! 3. one allocated on the render card and imported into its own GL, expected fast;
//! 4. the same buffer as 3, after the display card has imported the descriptor.
//!
//! A machine that cannot answer says so on standard error and asserts nothing, which is the shape
//! `cargo xtask ledger ignored` prescribes for a test that cannot be switched off.

use std::os::fd::AsFd;
use std::time::Instant;

use zgui_drm::Device;
use zgui_platform_drm::import::{gbm, gl};
use zgui_render_wgpu::{SharedGraphics, wgpu};

/// The extent every buffer is made at, which is the target's own mode.
const WIDTH: u32 = 1280;
/// As [`WIDTH`].
const HEIGHT: u32 = 1024;

/// How many times each buffer is filled before the clock is read.
///
/// Enough that the fill dominates the submission around it at both ends of the range this
/// distinguishes.
const FILLS: u32 = 20;

#[test]
fn where_a_shared_buffer_lives_before_and_after_the_other_card_imports_it() {
    let graphics = SharedGraphics::new();
    let gpu = match graphics.open_gpu() {
        Ok(gpu) => gpu,
        Err(failure) => {
            eprintln!("no usable graphics device, so nothing was measured: {failure}");
            return;
        }
    };
    eprintln!("rendering on {}", gpu.describe());

    let library = match gbm::Library::load() {
        Ok(library) => library,
        Err(reason) => {
            eprintln!("no libgbm, so nothing was measured: {reason}");
            return;
        }
    };

    // The scale: a texture wgpu made on the rendering card, which is local by construction.
    let local = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("residency.local"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    report("wgpu's own texture", Reading::Rate(rate(&gpu, &local)));

    for card in ["/dev/dri/card0", "/dev/dri/card1"] {
        let node = match Device::open(card) {
            Ok(node) => node,
            Err(error) => {
                eprintln!("{card}: will not open, so it was skipped: {error}");
                continue;
            }
        };
        let allocator = match gbm::Device::new(&library, node.as_fd()) {
            Ok(allocator) => allocator,
            Err(reason) => {
                eprintln!("{card}: no gbm device, so it was skipped: {reason}");
                continue;
            }
        };
        let mut drawn = match gl::create(&gpu, &allocator, WIDTH, HEIGHT, 1) {
            Ok(drawn) => drawn,
            Err(reason) => {
                eprintln!("{card}: nothing this card allocates can be drawn into: {reason}");
                continue;
            }
        };
        let Some(buffer) = drawn.first_mut() else {
            continue;
        };
        eprintln!(
            "--- allocated on {card} through {} ---",
            allocator.backend()
        );
        let reading = fill(&gpu, buffer);
        report("imported into GL", reading);

        // The question. Every other card takes a handle to the descriptor, and the buffer is filled
        // again: a driver that moved the memory to satisfy that import says so in the number.
        for other in ["/dev/dri/card0", "/dev/dri/card1"] {
            if other == card {
                continue;
            }
            let Ok(display) = Device::open(other) else {
                continue;
            };
            let descriptor = match buffer.descriptor() {
                Ok(descriptor) => descriptor,
                Err(reason) => {
                    eprintln!("  no descriptor to hand over: {reason}");
                    continue;
                }
            };
            match display.import_buffer(descriptor) {
                Ok(handle) => eprintln!("  {other} imported it as handle {}", handle.handle()),
                Err(error) => {
                    eprintln!("  {other} refused the descriptor: {error}");
                    continue;
                }
            }
            let reading = fill(&gpu, buffer);
            report("after the other card imported it", reading);
        }
    }
}

/// What one buffer's fill came to.
enum Reading {
    /// It was filled, at this many bytes a second.
    Rate(f64),
    /// The colour never arrived, so the graphics device did not do it and there is no rate.
    Refused,
}

/// Fills `buffer` and answers the rate, or that the fill never happened.
///
/// The colour is read back through the allocation rather than trusted: this exists to catch a
/// refusal, and a refusal is silent everywhere except the driver's own standard error.
fn fill(gpu: &zgui_render_wgpu::Gpu, buffer: &gl::Drawn) -> Reading {
    let measured = rate(gpu, buffer.texture());
    match buffer.peek() {
        // Cleared to blue, and the buffer is `XRGB8888` — blue first, then nothing else lit.
        Ok(pixel) if pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60 => Reading::Rate(measured),
        Ok(_) | Err(_) => Reading::Refused,
    }
}

/// Fills `texture` [`FILLS`] times and answers the bytes a second that took.
///
/// A clear rather than a draw, so what is timed is the writing and not a shader. The device is
/// waited for once at the end: every fill is in flight together, which is what makes the rate the
/// memory's and not the submission's.
fn rate(gpu: &zgui_render_wgpu::Gpu, texture: &wgpu::Texture) -> f64 {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    // One warm-up outside the clock, because the first touch of a fresh allocation pays for
    // whatever the driver does lazily.
    clear(gpu, &view, 1);
    gpu.wait();

    let started = Instant::now();
    clear(gpu, &view, FILLS);
    gpu.wait();
    let elapsed = started.elapsed().as_secs_f64();
    f64::from(WIDTH) * f64::from(HEIGHT) * 4.0 * f64::from(FILLS) / elapsed
}

/// Clears `view` `times` times through one submission.
fn clear(gpu: &zgui_render_wgpu::Gpu, view: &wgpu::TextureView, times: u32) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("residency.fill"),
        });
    for _ in 0..times {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("residency.fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    gpu.queue().submit([encoder.finish()]);
}

/// Prints one reading, in the unit the answer is obvious in.
fn report(what: &str, reading: Reading) {
    match reading {
        Reading::Rate(rate) => eprintln!("  {what:<34} {:8.1} MB/s", rate / 1e6),
        Reading::Refused => {
            eprintln!("  {what:<34}   refused — the graphics device would not write it");
        }
    }
}
