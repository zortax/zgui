//! Making the buffers a display scans out of and a renderer draws into.
//!
//! Everything here is a fact about a real graphics driver, so none of it can be stood in for. The
//! layout the driver chooses, how many memory planes it lays the image out in, how far apart the
//! rows end up and whether the memory leaves the device at all are the driver's own answers, and
//! the only way to learn them is to ask one.
//!
//! A machine with no such driver says so on standard error and asserts nothing — the shape `cargo
//! xtask ledger ignored` prescribes for a test that cannot be switched off. The pure half of this
//! milestone, the intersection and the narrowing, is unit-tested inside the modules that own it
//! and runs everywhere.
//!
//! Nothing here needs DRM master, and nothing here puts a picture on a screen. The display is read
//! for one thing only: which layouts its primary plane can scan out.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

use zgui_drm::Device;
use zgui_drm::device::Interface;
use zgui_drm::format::{Format, Modifier};
use zgui_platform_drm::{EXTENSIONS, FORMAT, Imported, Offered, Output, Unsupported};
use zgui_render_wgpu::target::swapchain::Supplied;
use zgui_render_wgpu::{Gpu, SharedGraphics, wgpu};

/// How many buffers a set holds here.
///
/// Three, the number the imported path drives a display from: one on the screen, one waiting for
/// its flip and one free to draw into. A set of one would prove nothing about a set.
const BUFFERS: usize = 3;

/// The extent used where no display states one.
const EXTENT: (u32, u32) = (1920, 1080);

/// How long one clear of one buffer is given before the device is called stuck.
///
/// Clearing a 1920x1080 image is measured in tens of microseconds, so this is five orders of
/// magnitude of slack. It exists for the case where the submission never completes at all, which
/// is what an image bound to no memory produces: the wait would otherwise never return, and a
/// wedged run says far less than a failure with a reason.
const DRAWN: Duration = Duration::from_secs(20);

/// A layout code under a vendor byte nobody was given.
///
/// The top eight bits of a modifier are the vendor, and `drm_fourcc.h` assigns them. `0xff` is
/// assigned to nobody, so no driver renders into this layout and no plane scans it out, and that
/// is the case worth asserting against.
const ABSENT: Modifier = Modifier(0xff00_0000_0000_0001);

/// Serialises every test in this binary onto the one graphics device.
///
/// A program has one device. These tests would otherwise create and destroy several at once on
/// several threads, which is neither what the code under test does nor what a driver is asked to
/// do anywhere.
fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    // A test that failed while holding it poisoned it; the next test still wants the device.
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Returns graphics asking for what an exported image needs, and the device they opened.
///
/// The graphics is answered as well as the device, because it owns the instance every device here
/// comes from and dropping it would close that.
fn opened(test: &str) -> Option<(SharedGraphics, Arc<Gpu>)> {
    let graphics = SharedGraphics::with_extensions(EXTENSIONS.to_vec());
    match graphics.open_gpu() {
        Ok(gpu) => Some((graphics, gpu)),
        Err(failure) => {
            eprintln!("{test}: no usable graphics device, so nothing was asserted: {failure}");
            for candidate in &failure.candidates {
                eprintln!("    {}: {}", candidate.name, candidate.reason);
            }
            None
        }
    }
}

/// What a display on this machine can scan out, and at what extent.
struct Screen {
    /// The layouts the display's primary plane accepts for the scanout fourcc.
    layouts: Vec<Modifier>,
    /// How wide the buffers are made.
    width: u32,
    /// How tall the buffers are made.
    height: u32,
    /// Where the layouts came from, for the run's output.
    source: String,
}

/// Returns the display's own answer, or the driver's own list where no display can be read.
///
/// A machine with no DRM device, no display plugged in or a driver that publishes no `IN_FORMATS`
/// still has a graphics driver to ask, and the whole of the image path is worth exercising there.
/// What stands in is the driver's own list, which makes the intersection the whole of it — so the
/// image is still made and read back, and only the display's half of the agreement is missing.
///
/// The obvious stand-in, the linear layout, is the wrong one. Every display engine scans it out
/// and only some drivers render into it: NVIDIA 595.84 lists `B8G8R8A8_UNORM` in the linear layout
/// without `COLOR_ATTACHMENT`, so a linear stand-in would report "no shared layout" on the very
/// machine this milestone was measured on.
fn screen(test: &str, gpu: &Gpu) -> Screen {
    let driver = |reason: &str| Screen {
        layouts: driver_offer(gpu),
        width: EXTENT.0,
        height: EXTENT.1,
        source: format!("the driver's own list, because {reason}"),
    };

    let device = match Device::open_first_with(Interface::Preferred) {
        Ok(device) => device,
        Err(error) => return driver(&format!("no DRM device could be opened ({error})")),
    };
    let outputs = match Output::discover(&device) {
        Ok(outputs) => outputs,
        Err(error) => return driver(&format!("the DRM device could not be read ({error})")),
    };
    let Some(output) = outputs.first() else {
        return driver("no display is plugged in");
    };
    let plane = output.pipe.plane;
    let published = match device.plane_formats(plane) {
        Ok(Some(published)) => published,
        Ok(None) => return driver(&format!("plane {plane} publishes no IN_FORMATS property")),
        Err(error) => return driver(&format!("plane {plane} could not be read ({error})")),
    };

    let layouts = published.modifiers(Format::XRGB8888).to_vec();
    if layouts.is_empty() {
        return driver(&format!("plane {plane} names no layout for XRGB8888"));
    }
    eprintln!(
        "{test}: plane {plane} on connector {} scans XRGB8888 out in {} layout(s): {}",
        output.pipe.connector,
        layouts.len(),
        listed(&layouts)
    );
    Screen {
        layouts,
        width: output.mode.width(),
        height: output.mode.height(),
        source: format!("plane {plane}"),
    }
}

/// Returns every layout this graphics driver renders into and exports.
///
/// Asked through the refusal, which is the only place the driver's own side is published. Nothing
/// shares a layout with the empty list, so the call always answers that refusal, and the refusal
/// names both sides.
fn driver_offer(gpu: &Gpu) -> Vec<Modifier> {
    match Imported::layouts_shared_with(gpu, &[]) {
        Err(Unsupported::NoSharedLayout { vulkan, .. }) => codes(&vulkan),
        _ => Vec::new(),
    }
}

/// Returns how many descriptors this process holds open.
///
/// Read from the kernel rather than counted here, so a descriptor leaked anywhere below shows up.
/// `None` on a machine whose `/proc` cannot be read, where the question cannot be asked at all.
fn descriptors() -> Option<usize> {
    std::fs::read_dir("/proc/self/fd").ok().map(Iterator::count)
}

/// Returns the layout codes of `offered`, which a report states.
fn codes(offered: &[Offered]) -> Vec<Modifier> {
    offered.iter().map(|entry| entry.modifier).collect()
}

/// Clears one buffer through a render pass and waits for the device to finish.
///
/// The whole of what a frame does to the image, minus the drawing: the image becomes a colour
/// attachment, wgpu records the barrier out of `UNDEFINED`, and the queue runs it.
fn clear(gpu: &Gpu, buffer: &Imported) {
    let view = buffer
        .texture()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.import.clear"),
        });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("zgui.import.clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.47,
                    b: 0.78,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    }));
    gpu.queue().submit([encoder.finish()]);
    gpu.wait();
}

/// Returns `layouts` as the hexadecimal a modifier is written in everywhere else.
///
/// The derived spelling is decimal, and a modifier read in decimal says nothing at all: the vendor
/// sits in the top byte and the layout in the bottom bytes, and both are read as digits.
fn listed(layouts: &[Modifier]) -> String {
    let written: Vec<String> = layouts
        .iter()
        .map(|layout| format!("{:#018x}", layout.0))
        .collect();
    written.join(", ")
}

#[test]
fn a_buffer_is_created_in_a_layout_the_display_and_the_driver_both_named() {
    let test = "a_buffer_is_created_in_a_layout_the_display_and_the_driver_both_named";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };
    let screen = screen(test, &gpu);

    let shared = match Imported::layouts_shared_with(&gpu, &screen.layouts) {
        Ok(shared) => shared,
        Err(refusal) => {
            eprintln!("{test}: no buffer can be exported here, so nothing was asserted: {refusal}");
            return;
        }
    };
    eprintln!(
        "{test}: {} shares {} layout(s) with {}: {}",
        gpu.describe(),
        shared.len(),
        screen.source,
        listed(&codes(&shared))
    );

    let buffers = Imported::create(&gpu, &screen.layouts, screen.width, screen.height, BUFFERS)
        .expect("a device that shares a layout can export a buffer in it");
    assert_eq!(buffers.len(), BUFFERS, "one buffer per slot was asked for");

    for (slot, buffer) in buffers.iter().enumerate() {
        // The driver picks out of the candidates it was given, and every candidate is a layout
        // both ends named. One that is not on the list is memory the display cannot read.
        let offer = shared
            .iter()
            .find(|entry| entry.modifier == buffer.modifier())
            .unwrap_or_else(|| {
                panic!(
                    "buffer {slot} is in {:#018x}, which was not among the {} candidates: {}",
                    buffer.modifier().0,
                    shared.len(),
                    listed(&codes(&shared))
                )
            });

        // The plane count comes with the layout, and it says how many offsets and strides a
        // framebuffer over this buffer states. Reading one fewer leaves part of the picture
        // unaddressed; reading one more names memory the image does not have.
        assert_eq!(
            buffer.layouts().len(),
            offer.planes as usize,
            "buffer {slot} is in {:#018x}, which has {} memory plane(s), and {} layout(s) were \
             read",
            buffer.modifier().0,
            offer.planes,
            buffer.layouts().len()
        );

        let Some(&first) = buffer.layouts().first() else {
            panic!("buffer {slot} states no memory plane at all, so nothing addresses its pixels");
        };
        // Rows cannot be closer together than the pixels of one row. A stride below this is a
        // layout read wrong, and it reaches a screen as a diagonal.
        assert!(
            first.stride() >= screen.width * 4,
            "buffer {slot} is {} wide at four bytes a pixel and its rows are {} bytes apart",
            screen.width,
            first.stride()
        );

        // The descriptor is the point of the export, and an invalid one is what a driver that
        // reported success and exported nothing leaves behind.
        let stat = rustix::fs::fstat(buffer.dmabuf()).unwrap_or_else(|error| {
            panic!("buffer {slot} exported a descriptor that is not a file: {error}")
        });
        let size = u64::try_from(stat.st_size).expect("a file's size is not negative");
        let needed =
            u64::from(first.offset()) + u64::from(first.stride()) * u64::from(screen.height);
        assert!(
            size >= needed,
            "buffer {slot} exported {size} bytes for a picture that ends at {needed}"
        );

        eprintln!(
            "{test}: buffer {slot} is {:#018x}, {} memory plane(s), offset {}, stride {}, \
             {size} bytes",
            buffer.modifier().0,
            buffer.layouts().len(),
            first.offset(),
            first.stride()
        );
    }
}

#[test]
fn every_buffer_of_a_set_is_its_own_image_and_its_own_descriptor() {
    let test = "every_buffer_of_a_set_is_its_own_image_and_its_own_descriptor";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };
    let screen = screen(test, &gpu);

    let buffers =
        match Imported::create(&gpu, &screen.layouts, screen.width, screen.height, BUFFERS) {
            Ok(buffers) => buffers,
            Err(refusal) => {
                eprintln!(
                    "{test}: no buffer can be exported here, so nothing was asserted: {refusal}"
                );
                return;
            }
        };

    // Three descriptors, each naming a file of its own. `(st_dev, st_ino)` identifies the dma-buf
    // *file* rather than the allocation behind it — two exports of one allocation are two inodes
    // on this driver — so what this catches is a set built by duplicating one descriptor rather
    // than by exporting each buffer. The images being distinct is the export call being made once
    // per image, which the loop above does.
    let mut seen = HashSet::new();
    for (slot, buffer) in buffers.iter().enumerate() {
        let stat = rustix::fs::fstat(buffer.dmabuf()).expect("the descriptor is a file");
        assert!(
            seen.insert((stat.st_dev, stat.st_ino)),
            "buffer {slot} carries a descriptor an earlier one already carried"
        );
    }
}

#[test]
fn a_frame_drawn_into_a_buffer_reaches_the_device() {
    let test = "a_frame_drawn_into_a_buffer_reaches_the_device";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };
    let screen = screen(test, &gpu);

    let buffers =
        match Imported::create(&gpu, &screen.layouts, screen.width, screen.height, BUFFERS) {
            Ok(buffers) => buffers,
            Err(refusal) => {
                eprintln!(
                    "{test}: no buffer can be exported here, so nothing was asserted: {refusal}"
                );
                return;
            }
        };

    // Every other test here reads what the driver *said*. This one makes the driver use what it
    // made. A render pass that clears is the smallest thing that binds the image as a colour
    // attachment, records the barrier out of `UNDEFINED`, submits, and finishes.
    //
    // What it proves: the image has memory behind it, the layout the driver chose can hold a
    // colour attachment, and the descriptor wgpu was handed describes an image wgpu can use. An
    // image bound to nothing gets no further than this — measured, and the device stops answering
    // rather than reporting anything, which is why the wait below has a deadline.
    //
    // What it does not prove: which pixels landed. Reading them back would need `COPY_SRC` on the
    // texture, and every usage added here has to be added to the Vulkan image as well or wgpu
    // reports success over an image that cannot do it. The frame a person can see arrives with the
    // flip, one milestone along.
    //
    // The clears run on a thread of their own so that the wait can be given up on. `Gpu::wait`
    // polls the device with no deadline, and a submission the hardware will never finish is
    // therefore a test binary that hangs until whatever runs it gives up — which reads as a build
    // that stopped rather than as a defect with a name.
    let (finished, arrived) = mpsc::channel();
    let worker = {
        let gpu = Arc::clone(&gpu);
        thread::Builder::new()
            .name("zgui.import.clear".to_owned())
            .spawn(move || {
                for (slot, buffer) in buffers.iter().enumerate() {
                    clear(&gpu, buffer);
                    if finished.send(slot).is_err() {
                        return;
                    }
                }
            })
            .expect("a thread can be started")
    };

    for slot in 0..BUFFERS {
        match arrived.recv_timeout(DRAWN) {
            Ok(done) => {
                eprintln!("{test}: buffer {done} was cleared and the submission completed");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => panic!(
                "clearing buffer {slot} did not finish inside {DRAWN:?}, so the device is still \
                 working on a frame it cannot complete: the image has no memory behind it, or the \
                 layout the driver chose cannot hold a colour attachment"
            ),
            // The thread ended before it reported. Joining re-raises whatever it panicked with,
            // which is the real reason and is more use than anything this could invent.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                worker
                    .join()
                    .expect("the thread that cleared the buffers ended without reporting");
                panic!("the clears ended after {slot} of {BUFFERS} with nothing to say");
            }
        }
    }
    worker
        .join()
        .expect("the thread that cleared the buffers panicked");

    assert!(
        !gpu.loss().is_lost(),
        "the device was lost while drawing into the buffers it made"
    );
}

#[test]
fn the_texture_is_one_a_supplied_presentation_takes() {
    let test = "the_texture_is_one_a_supplied_presentation_takes";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };
    let screen = screen(test, &gpu);

    let buffers =
        match Imported::create(&gpu, &screen.layouts, screen.width, screen.height, BUFFERS) {
            Ok(buffers) => buffers,
            Err(refusal) => {
                eprintln!(
                    "{test}: no buffer can be exported here, so nothing was asserted: {refusal}"
                );
                return;
            }
        };

    // The renderer's own rules, asked of the renderer rather than restated here. A texture that
    // fails them is refused at the far end of the wiring, after the mode has been set, and there
    // is nothing this milestone could do about it there.
    let textures: Vec<wgpu::Texture> = buffers
        .iter()
        .map(|buffer| buffer.texture().clone())
        .collect();
    assert_eq!(
        Supplied::unusable(&textures),
        None,
        "the renderer refuses the textures this made"
    );

    let first = &textures[0];
    assert_eq!(first.format(), FORMAT);
    assert_eq!(first.width(), screen.width);
    assert_eq!(first.height(), screen.height);
}

#[test]
fn a_display_and_a_device_that_share_no_layout_are_refused_by_name() {
    let test = "a_display_and_a_device_that_share_no_layout_are_refused_by_name";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };

    // A layout no driver has stands in for the machine this really happens on: a renderer on one
    // card and a display on another, where the two drivers publish plenty and share none of it.
    let Err(refusal) = Imported::create(&gpu, &[ABSENT], EXTENT.0, EXTENT.1, BUFFERS) else {
        panic!("a buffer was exported in a layout with a vendor byte nobody was given");
    };

    match &refusal {
        Unsupported::NoSharedLayout { vulkan, scanout } => {
            assert_eq!(
                scanout,
                &[ABSENT],
                "the refusal forgot what it was asked for"
            );
            assert!(
                !vulkan.iter().any(|entry| entry.modifier == ABSENT),
                "a driver reported a layout with a vendor byte nobody was given"
            );
            // What the caller writes to a log, and the reason the refusal is a value: a console
            // that fell back to copying every frame has to say which two lists disagreed.
            let stated = refusal.to_string();
            assert!(
                stated.contains(&listed(&[ABSENT])),
                "the refusal does not say which layout the display wanted: {stated}"
            );
            eprintln!(
                "{test}: {} renders and exports {} layout(s): {}",
                gpu.describe(),
                vulkan.len(),
                listed(&codes(vulkan))
            );
        }
        // A GL adapter, or a device the extensions were refused on. Both are answers to the same
        // question — this machine cannot do it — and both name themselves.
        other => {
            let stated = other.to_string();
            assert!(
                !stated.is_empty(),
                "a refusal the caller has to log said nothing"
            );
            eprintln!("{test}: refused earlier than the layouts: {stated}");
        }
    }
}

#[test]
fn a_device_that_never_asked_for_the_extensions_refuses_and_names_one() {
    let test = "a_device_that_never_asked_for_the_extensions_refuses_and_names_one";
    let _guard = device_lock();
    // The ordinary constructor, which asks for no Vulkan device extension at all. This is what a
    // program that forgot gets, and the point is that it finds out here rather than inside the
    // first frame with an error naming a format.
    let graphics = SharedGraphics::new();
    let Ok(gpu) = graphics.open_gpu() else {
        eprintln!("{test}: no usable graphics device, so nothing was asserted");
        return;
    };

    match Imported::create(&gpu, &[Modifier::LINEAR], EXTENT.0, EXTENT.1, 1) {
        Err(Unsupported::Extension(name)) => {
            assert!(
                EXTENSIONS.contains(&name),
                "the refusal named {name:?}, which this path does not ask for"
            );
            eprintln!("{test}: {} lacks {name:?}", gpu.describe());
        }
        Err(Unsupported::Backend(backend)) => {
            eprintln!("{test}: this machine's adapter is a {backend:?} one, which asks no earlier");
        }
        Err(other) => {
            panic!("a device that enabled nothing was refused for another reason: {other}")
        }
        // wgpu-hal enables two of the three on its own. A driver that also enabled the third
        // without being asked would reach here, and that is a machine where the whole path works
        // by luck rather than by request.
        Ok(_) => eprintln!(
            "{test}: {} enabled every extension without being asked",
            gpu.describe()
        ),
    }
}

#[test]
fn a_released_set_gives_back_every_descriptor_it_held() {
    let test = "a_released_set_gives_back_every_descriptor_it_held";
    let _guard = device_lock();
    let Some((_graphics, gpu)) = opened(test) else {
        return;
    };
    let screen = screen(test, &gpu);

    let make = || Imported::create(&gpu, &screen.layouts, screen.width, screen.height, BUFFERS);

    // One set made and released first, so that whatever the driver opens on the way to its first
    // export is already open when the count below is taken.
    match make() {
        Ok(buffers) => drop(buffers),
        Err(refusal) => {
            eprintln!("{test}: no buffer can be exported here, so nothing was asserted: {refusal}");
            return;
        }
    }
    let Some(before) = descriptors() else {
        eprintln!("{test}: /proc/self/fd cannot be read, so nothing was asserted");
        return;
    };

    let buffers = make().expect("a second set is made the same way the first was");
    let held = descriptors().expect("/proc/self/fd was readable a moment ago");
    assert!(
        held >= before + BUFFERS,
        "a set of {BUFFERS} exported buffers holds {} descriptors more than the {before} before it",
        held - before
    );
    drop(buffers);

    // The descriptors close with the set, and that is all this measures. One descriptor held per
    // buffer per mode change is a program that runs out of them on a laptop lid, so it is worth
    // measuring on its own. The image and its memory are released by the same drop, through the
    // callback wgpu runs while it destroys the texture, and a count of open files cannot see that.
    assert_eq!(
        descriptors().expect("/proc/self/fd was readable a moment ago"),
        before,
        "a released set left descriptors open"
    );

    // And the device still hands out another set afterwards.
    let again = make().expect("the device that released a set can make another");
    assert_eq!(again.len(), BUFFERS);
}
