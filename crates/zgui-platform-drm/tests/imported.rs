//! Driving a display from buffers the renderer draws into.
//!
//! Everything here is a fact about a real graphics driver and a real kernel, so none of it can be
//! stood in for. Which layout the driver picks, whether the kernel will import that memory at all,
//! whether it will take a framebuffer over it in that layout with those offsets, and whether the
//! device finishes the barriers that pass the image back and forth are four answers only the
//! machine has.
//!
//! # What runs here, and what cannot
//!
//! Nearly all of it. `PRIME_FD_TO_HANDLE` and `ADDFB2` need no DRM master, so every buffer is
//! imported and every framebuffer is registered for real while a compositor is driving the screen.
//! So are both barriers, which touch no display at all.
//!
//! What cannot run is the modeset and the flip, which are the two ioctls that need master. A
//! machine that has it runs them here too; a machine that does not gets as far as the commit and
//! is told which step refused. That still asserts the ordering the frame depends on — the barrier
//! runs and finishes **before** anything is committed — because a refusal naming the barrier and a
//! refusal naming the commit are different messages.
//!
//! Running this under `VK_LAYER_KHRONOS_validation` checks the barriers themselves: the layer
//! tracks every image's layout and which queue family owns it, so a half that states the wrong
//! layout, the wrong family or a scope narrower than the frame is reported here and nowhere else.
//!
//! A machine with no display, no graphics device or no shared layout says so on standard error and
//! asserts nothing, which is the shape `cargo xtask ledger ignored` prescribes for a test that
//! cannot be switched off.

use std::cell::RefCell;
use std::collections::HashSet;
use std::os::fd::{AsRawFd, OwnedFd};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use zgui_drm::commit;
use zgui_drm::device::Interface;
use zgui_drm::format::{Format, Modifier};
use zgui_drm::{Device, Error};
use zgui_platform_drm::{
    Copied, Cursor, DrmDisplay, EXTENSIONS, FORMAT, Handover, Imported, Output, Scanout,
};
use zgui_render_wgpu::target::swapchain::Supplied;
use zgui_render_wgpu::{Gpu, SharedGraphics, wgpu};

/// How many buffers the imported shape drives a display from.
///
/// Three: one on the screen, one an outstanding flip names, and one the renderer draws into. The
/// crate states the same number, and this is the count a caller sees.
const BUFFERS: usize = 3;

/// The fourcc the imported shape registers its framebuffers under.
///
/// `XRGB8888` is `B, G, R, x` in memory, and so is a `B8G8R8A8_UNORM` image.
const FOURCC: Format = Format::XRGB8888;

/// Serialises every test in this binary onto the one graphics device and the one display.
///
/// A program has one of each. These tests would otherwise create and destroy several devices at
/// once on several threads, which is neither what the code under test does nor what a driver is
/// asked to do anywhere.
fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    // A test that failed while holding it poisoned it; the next test still wants the device.
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A display, a graphics device asking for what an exported image needs, and the graphics they
/// came from.
///
/// The graphics is answered as well, because it owns the instance the device came from and
/// dropping it would close that.
struct Machine {
    /// Kept alive for the instance behind the device.
    _graphics: SharedGraphics,
    /// The graphics device the images are made on.
    gpu: Arc<Gpu>,
    /// The display device the buffers are imported into.
    ///
    /// Shared, because a [`DrmDisplay`] holds the device open for as long as anything draws to it
    /// and a test that builds one still owns the device itself.
    device: Arc<Device>,
    /// The display they are registered for.
    output: Output,
}

/// Returns everything a test here needs, or the reason this machine has nothing to say.
///
/// Four ways to get nothing, and each names itself: no graphics device, no display device, a
/// display device that cannot be read, and nothing plugged in.
fn machine(test: &str) -> Option<Machine> {
    let graphics = SharedGraphics::with_extensions(EXTENSIONS.to_vec());
    let gpu = match graphics.open_gpu() {
        Ok(gpu) => gpu,
        Err(failure) => {
            eprintln!("{test}: no usable graphics device, so nothing was asserted: {failure}");
            for candidate in &failure.candidates {
                eprintln!("    {}: {}", candidate.name, candidate.reason);
            }
            return None;
        }
    };
    let device = match Device::open_first_with(Interface::Preferred) {
        Ok(device) => device,
        Err(error) => {
            eprintln!(
                "{test}: no DRM device on this machine, so nothing was asserted: {error}\n\
                 load the virtual driver with `sudo modprobe vkms` to run it"
            );
            return None;
        }
    };
    let outputs = match Output::discover(&device) {
        Ok(outputs) => outputs,
        Err(error) => {
            eprintln!("{test}: the DRM device could not be read, so nothing was asserted: {error}");
            return None;
        }
    };
    let Some(output) = outputs.into_iter().next() else {
        eprintln!("{test}: no display is plugged in, so nothing was asserted");
        return None;
    };
    eprintln!(
        "{test}: {} on connector {} crtc {} plane {} at {}x{}",
        gpu.describe(),
        output.pipe.connector,
        output.pipe.crtc,
        output.pipe.plane,
        output.mode.width(),
        output.mode.height()
    );
    Some(Machine {
        _graphics: graphics,
        gpu,
        device: Arc::new(device),
        output,
    })
}

/// Returns a display driven from imported buffers, or the reason this machine cannot have one.
///
/// The pointer is stated as being on a plane rather than read off the device, because whether this
/// machine's display engine composites one is not what these tests are about — and a machine
/// without one would otherwise assert nothing at all. The refusal for a display that really has no
/// plane is asserted on its own.
fn imported(test: &str, machine: &Machine) -> Option<Scanout> {
    match Scanout::imported(&machine.device, &machine.output, &machine.gpu, true) {
        Ok(scanout) => Some(scanout),
        Err(reason) => {
            eprintln!("{test}: this display cannot be driven from imported buffers: {reason}");
            None
        }
    }
}

/// Returns what the display's own plane says it can scan [`FOURCC`] out in.
///
/// # Errors
///
/// Returns whatever the device refused the plane's format list with.
fn published(device: &Device, plane: u32) -> Result<Vec<Modifier>, Error> {
    Ok(device
        .plane_formats(plane)?
        .map(|published| published.modifiers(FOURCC).to_vec())
        .unwrap_or_default())
}

/// Returns the ids the kernel gave this display's framebuffers.
fn ids(scanout: &Scanout) -> Vec<u32> {
    scanout
        .framebuffers()
        .into_iter()
        .map(|framebuffer| framebuffer.id())
        .collect()
}

/// Returns how many descriptors this process holds open.
///
/// Read from the kernel rather than counted here, so a descriptor leaked anywhere below shows up.
fn descriptors() -> Option<usize> {
    std::fs::read_dir("/proc/self/fd").ok().map(Iterator::count)
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

/// Records and submits a clear over `buffer`, without waiting for it.
///
/// The whole of what a frame does to the image, minus the drawing: the image becomes a colour
/// attachment, wgpu records the barrier out of `UNDEFINED`, and the queue takes the work. Nothing
/// waits here on purpose — the release barrier is submitted on the same queue immediately
/// afterwards, and its own wait is what covers both.
fn draw(gpu: &Gpu, buffer: &Imported) {
    let view = buffer
        .texture()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.imported.frame"),
        });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("zgui.imported.frame"),
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
}

#[test]
fn every_buffer_of_a_display_is_imported_and_registered_in_the_layout_the_driver_chose() {
    let test =
        "every_buffer_of_a_display_is_imported_and_registered_in_the_layout_the_driver_chose";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    let scans = published(&machine.device, machine.output.pipe.plane)
        .expect("a plane this backend chose can be read");
    let Some(scanout) = imported(test, &machine) else {
        return;
    };

    let buffers = scanout.buffers();
    assert_eq!(buffers.len(), BUFFERS, "one image per buffer was asked for");
    let framebuffers = scanout.framebuffers();
    assert_eq!(
        framebuffers.len(),
        BUFFERS,
        "one framebuffer per image was registered"
    );

    // Distinct ids, and none of them zero. The kernel allocates an object id from one, so a zero
    // here is a request that reported success and registered nothing, and a repeat is three
    // buffers the display would scan the same memory out of.
    let mut seen = HashSet::new();
    for (slot, framebuffer) in framebuffers.iter().enumerate() {
        assert_ne!(framebuffer.id(), 0, "framebuffer {slot} was given no id");
        assert!(
            seen.insert(framebuffer.id()),
            "framebuffer {slot} carries the id an earlier one already carries"
        );
    }

    for (slot, buffer) in buffers.iter().enumerate() {
        // The driver picks out of the candidates it was given, and every candidate came from this
        // plane's own list. A layout outside it describes memory the display cannot read.
        assert!(
            scans.contains(&buffer.modifier()),
            "buffer {slot} is in {:#018x}, which plane {} does not scan out: {}",
            buffer.modifier().0,
            machine.output.pipe.plane,
            listed(&scans)
        );
        let Some(&first) = buffer.layouts().first() else {
            panic!("buffer {slot} states no memory plane at all, so nothing addresses its pixels");
        };
        assert!(
            first.stride() >= machine.output.mode.width() * 4,
            "buffer {slot} is {} wide at four bytes a pixel and its rows are {} bytes apart",
            machine.output.mode.width(),
            first.stride()
        );
        eprintln!(
            "{test}: buffer {slot} is {:#018x}, {} memory plane(s), offset {}, stride {}, \
             framebuffer {}",
            buffer.modifier().0,
            buffer.layouts().len(),
            first.offset(),
            first.stride(),
            framebuffers[slot].id()
        );
    }

    // The renderer's own rules, asked of the renderer rather than restated here. A set it refuses
    // is a display that would be set up and then never drawn to.
    let textures: Vec<wgpu::Texture> = buffers
        .iter()
        .map(|buffer| buffer.texture().clone())
        .collect();
    assert_eq!(
        Supplied::unusable(&textures),
        None,
        "the renderer refuses the textures this display was built from"
    );
    assert_eq!(textures[0].format(), FORMAT);
    assert_eq!(textures[0].width(), machine.output.mode.width());
    assert_eq!(textures[0].height(), machine.output.mode.height());

    // Nothing has been drawn or flipped, so the first buffer is the one a renderer is pointed at.
    // Acquiring it takes nothing back — the display engine has never been given it — which is the
    // one case an acquire has to do nothing in.
    let mut scanout = scanout;
    assert_eq!(
        scanout.acquire().expect("a new display owes no barrier"),
        Some(0),
        "the first frame goes into the first buffer, which the modeset then puts on the screen"
    );

    scanout.release(&machine.device);
}

#[test]
fn a_released_display_gives_back_every_descriptor_and_can_be_built_again() {
    let test = "a_released_display_gives_back_every_descriptor_and_can_be_built_again";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };

    // One set made and released first, so that whatever the two drivers open on the way to their
    // first buffer is already open when the count below is taken.
    match imported(test, &machine) {
        Some(scanout) => scanout.release(&machine.device),
        None => return,
    }
    let Some(before) = descriptors() else {
        eprintln!("{test}: /proc/self/fd cannot be read, so nothing was asserted");
        return;
    };

    let scanout = Scanout::imported(&machine.device, &machine.output, &machine.gpu, true)
        .expect("a second display is built the same way the first was");
    let held = descriptors().expect("/proc/self/fd was readable a moment ago");
    assert!(
        held >= before + BUFFERS,
        "a display of {BUFFERS} imported buffers holds {} descriptors more than the {before} \
         before it",
        held - before
    );
    let framebuffers = ids(&scanout);
    scanout.release(&machine.device);

    assert_eq!(
        descriptors().expect("/proc/self/fd was readable a moment ago"),
        before,
        "a released display left descriptors open"
    );

    // And the device hands out another display afterwards, which a driver still holding the last
    // one's handles and framebuffers would run out of room for eventually. The ids of both are
    // reported rather than asserted on: object ids are the device's, shared with whatever else is
    // driving it, so which numbers come back is not this process's to predict.
    let again = Scanout::imported(&machine.device, &machine.output, &machine.gpu, true)
        .expect("the device that released a display can build another");
    let reissued = ids(&again);
    assert_eq!(reissued.len(), BUFFERS);
    eprintln!("{test}: framebuffers {framebuffers:?} were released, and {reissued:?} came back");
    again.release(&machine.device);
}

/// Returns a set of images and the barriers over them, or the reason this machine has none.
///
/// `takes_a_fence` is what a display answers when it is asked whether a commit on it can carry a
/// sync file. Both answers are exercised here rather than only the one this machine gives, because
/// the second is what every display on the legacy interface and every plane with no `IN_FENCE_FD`
/// property gets.
///
/// The barriers come first in the pair so that they are dropped first. They reach the graphics
/// device through a handle that keeps nothing alive, and the images hold the textures that do.
fn recorded(
    test: &str,
    machine: &Machine,
    takes_a_fence: bool,
) -> Option<(Handover, Vec<Imported>)> {
    let scans = match published(&machine.device, machine.output.pipe.plane) {
        Ok(scans) if !scans.is_empty() => scans,
        _ => {
            eprintln!(
                "{test}: this display publishes no layout for {FOURCC:?}, so nothing was asserted"
            );
            return None;
        }
    };

    let buffers = match Imported::create(
        &machine.gpu,
        &scans,
        machine.output.mode.width(),
        machine.output.mode.height(),
        BUFFERS,
    ) {
        Ok(buffers) => buffers,
        Err(refusal) => {
            eprintln!("{test}: no buffer can be exported here, so nothing was asserted: {refusal}");
            return None;
        }
    };
    let handover = Handover::record(&machine.gpu, &buffers, takes_a_fence)
        .expect("a device that exported the images can record the barriers over them");
    Some((handover, buffers))
}

/// Runs one frame per buffer: take it back, draw into it, give it over.
///
/// Returns what each frame signalled. This is the real sequence: nothing waits between the frame
/// and the release, because one queue starts its submissions in order, and that is why a
/// barrier-only command buffer is enough.
fn round(
    machine: &Machine,
    buffers: &[Imported],
    handover: &mut Handover,
    round: usize,
) -> Vec<Option<OwnedFd>> {
    let mut fences = Vec::with_capacity(buffers.len());
    for (slot, buffer) in buffers.iter().enumerate() {
        handover.acquire(slot).unwrap_or_else(|refusal| {
            panic!("buffer {slot} could not be taken back on round {round}: {refusal}")
        });
        draw(&machine.gpu, buffer);
        let fence = handover.release(slot).unwrap_or_else(|refusal| {
            panic!("buffer {slot} could not be given over on round {round}: {refusal}")
        });
        fences.push(fence);
    }
    fences
}

#[test]
fn the_barriers_that_pass_a_frame_to_the_display_finish_inside_their_deadline() {
    let test = "the_barriers_that_pass_a_frame_to_the_display_finish_inside_their_deadline";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    // The shape with no fence in it, so every barrier here is something this thread waits for and
    // therefore something the deadline covers.
    let Some((mut handover, buffers)) = recorded(test, &machine, false) else {
        return;
    };

    // Twice round, so that the acquire does something. The first time round every buffer is fresh
    // and the acquire is a no-op; the second time round each one is held by the display engine and
    // a barrier really runs, which is the half a single pass would never reach.
    //
    // Each wait carries a deadline, so a device that never completes the frame is a failure with a
    // reason rather than a test binary that stops: `Gpu::wait` would poll for ever, and an image
    // bound to no memory produces exactly that.
    for pass in 0..2 {
        let fences = round(&machine, &buffers, &mut handover, pass);
        assert!(
            fences.iter().all(Option::is_none),
            "a display that can carry no fence was handed one anyway"
        );
        eprintln!(
            "{test}: round {pass}, every buffer was taken back, drawn into and given to the \
             display engine"
        );
    }

    assert!(
        !machine.gpu.loss().is_lost(),
        "the device was lost while passing the buffers it made back and forth"
    );

    // A second release with no acquire between is what a caller that drew into a buffer the
    // display engine still holds does, and it is refused by name. Without that the frame would be
    // drawn over pixels the specification calls undefined, and every ioctl would report success.
    let Err(refusal) = handover.release(0) else {
        panic!("a buffer the display engine holds was given to it a second time");
    };
    eprintln!("{test}: a buffer given over twice is refused: {refusal}");
    handover
        .acquire(0)
        .expect("and taking it back is what makes the next frame legal");

    // A slot no buffer sits at is refused by name rather than reaching the driver, which is where
    // a renderer told to draw into a set of another length would otherwise arrive.
    let Err(refusal) = handover.release(BUFFERS) else {
        panic!("a barrier was submitted for a buffer this set does not hold");
    };
    eprintln!("{test}: a slot past the end is refused: {refusal}");
    let Err(refusal) = handover.acquire(BUFFERS) else {
        panic!("a barrier was submitted for a buffer this set does not hold");
    };
    eprintln!("{test}: a slot past the end is refused: {refusal}");
}

#[test]
fn a_display_that_can_carry_no_fence_is_waited_for_before_it_is_committed() {
    let test = "a_display_that_can_carry_no_fence_is_waited_for_before_it_is_committed";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    // The unavailable case, made rather than reasoned about: a display on the legacy interface and
    // a plane with no `IN_FENCE_FD` property both answer exactly this.
    let Some((mut handover, buffers)) = recorded(test, &machine, false) else {
        return;
    };
    assert_eq!(handover.waits(), 0, "nothing has been submitted yet");

    // The first round acquires nothing — every buffer is fresh — so the waits it costs are the
    // releases alone. That is the assertion that the acquire stopped waiting: a per-frame wait
    // there would show up here as twice the number.
    let fences = round(&machine, &buffers, &mut handover, 0);
    assert!(
        fences.iter().all(Option::is_none),
        "a display that can carry no fence has nothing to hand the kernel"
    );
    assert_eq!(
        handover.waits(),
        BUFFERS,
        "one wait per frame given over, and none for taking a buffer back"
    );

    // The second round acquires for real, because the display engine holds every buffer now.
    let fences = round(&machine, &buffers, &mut handover, 1);
    assert!(fences.iter().all(Option::is_none));
    assert_eq!(
        handover.waits(),
        2 * BUFFERS,
        "an acquire that really submits a barrier still waits for nothing"
    );
    eprintln!(
        "{test}: {} frames cost {} waits",
        2 * BUFFERS,
        handover.waits()
    );

    // Teardown with barriers still running, which a frame that waits for nothing leaves behind.
    // Dropping the set has to empty the queue before it destroys the pool, and the device has to
    // be usable afterwards — a pool destroyed under a running command buffer is undefined, and
    // what it looks like is the next set failing or the device going away.
    drop(handover);
    drop(buffers);
    let Some((again, buffers)) = recorded(test, &machine, false) else {
        panic!("the device that gave a set back can record another");
    };
    assert!(
        !machine.gpu.loss().is_lost(),
        "the device was lost while the barriers were given back"
    );
    drop(again);
    drop(buffers);
}

#[test]
fn a_released_frame_hands_the_kernel_a_sync_file_this_thread_never_waits_for() {
    let test = "a_released_frame_hands_the_kernel_a_sync_file_this_thread_never_waits_for";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    let Some((mut handover, buffers)) = recorded(test, &machine, true) else {
        return;
    };

    let mut fences = round(&machine, &buffers, &mut handover, 0);
    fences.extend(round(&machine, &buffers, &mut handover, 1));

    // Whether this driver exports a sync file is a fact about the machine, so both answers are
    // taken. What is asserted is that the two halves agree: a frame that answered a descriptor is
    // one nothing here waited for, and a frame that answered none is one that was waited for.
    if fences.iter().all(Option::is_none) {
        assert_eq!(
            handover.waits(),
            fences.len(),
            "a frame the kernel was given no fence for has to have been waited for here"
        );
        eprintln!(
            "{test}: {} exports no sync file, so every frame was waited for here",
            machine.gpu.describe()
        );
        return;
    }

    assert_eq!(
        handover.waits(),
        0,
        "a frame the kernel is handed a fence for is a frame this thread does not wait for"
    );
    for (frame, fence) in fences.iter().enumerate() {
        let Some(fence) = fence else {
            panic!("frame {frame} answered no sync file while the frames around it did");
        };
        // A real descriptor rather than a number. A driver that reported success and exported
        // nothing would hand back something the kernel refuses at the far end of a commit, where
        // the only symptom is a display that stops.
        let stat = rustix::fs::fstat(fence).unwrap_or_else(|error| {
            panic!("frame {frame} answered a descriptor that is not a file: {error}")
        });
        assert_ne!(stat.st_ino, 0, "frame {frame} answered an empty descriptor");
        let named = std::fs::read_link(format!("/proc/self/fd/{}", fence.as_raw_fd()))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| format!("<unreadable: {error}>"));
        eprintln!("{test}: frame {frame} signalled {named}");
    }

    // Every descriptor closes here, as a caller does with one the kernel gave back.
    drop(fences);
    assert!(
        !machine.gpu.loss().is_lost(),
        "the device was lost while it was signalling the sync files"
    );
}

#[test]
fn a_frame_is_given_to_the_display_engine_before_anything_is_committed() {
    let test = "a_frame_is_given_to_the_display_engine_before_anything_is_committed";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    let Some(mut scanout) = imported(test, &machine) else {
        return;
    };
    let mut commit = commit::for_device(&machine.device);

    let slot = scanout
        .acquire()
        .expect("a new display owes no barrier")
        .expect("nothing is outstanding on a new display");
    draw(&machine.gpu, &scanout.buffers()[slot]);

    // A machine holding DRM master puts the frame on the screen here. A machine without it gets as
    // far as the commit and is refused there — which is the assertion: the barrier ran and
    // finished first, because a refusal from the barrier names the barrier.
    match scanout.present_drawn(&machine.device, &mut *commit) {
        Ok(shown) => {
            assert!(shown, "nothing is outstanding in front of the first frame");
            eprintln!("{test}: this process holds the device, so the frame reached the screen");
        }
        Err(refused) => {
            let stated = refused.to_string();
            for step in ["barrier", "a frame to the display", "slot"] {
                assert!(
                    !stated.contains(step),
                    "the frame was refused before the commit, by the handover itself: {stated}"
                );
            }
            eprintln!(
                "{test}: the barrier finished and the commit was refused, which is what a process \
                 that is not DRM master gets: {stated}"
            );
        }
    }

    scanout.release(&machine.device);
}

#[test]
fn a_commit_that_is_refused_keeps_no_descriptor_of_this_programs() {
    let test = "a_commit_that_is_refused_keeps_no_descriptor_of_this_programs";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    let Some(mut scanout) = imported(test, &machine) else {
        return;
    };
    let mut commit = commit::for_device(&machine.device);
    // Reported rather than asserted: whether a plane carries `IN_FENCE_FD` is a fact about a
    // driver. It says whether the frames below make a descriptor at all, and the count afterwards
    // is over those.
    let carries = commit::waits_for_a_fence(&machine.device, machine.output.pipe.plane)
        .expect("a plane this backend chose can be read");
    eprintln!(
        "{test}: plane {} {} a fence per frame",
        machine.output.pipe.plane,
        if carries { "carries" } else { "carries no" }
    );

    // The kernel reads the fence out of a sync file and gives the file back — `sync_file_get_fence`
    // takes a reference to the fence inside and never closes the descriptor — so the descriptor is
    // this program's to close on a commit that was refused and on one that succeeded alike. A
    // caller that expected the kernel to take one leaks a descriptor a frame, which on a machine
    // that is not DRM master is a descriptor per refused frame in a loop.
    //
    // The first frame is drawn before the count is taken, so that whatever the drivers open on the
    // way to their first commit is already open.
    let mut frame = |scanout: &mut Scanout| {
        let Some(slot) = scanout.acquire().expect("a display owes no barrier") else {
            panic!("nothing is outstanding, so a buffer is always named");
        };
        draw(&machine.gpu, &scanout.buffers()[slot]);
        scanout.present_drawn(&machine.device, &mut *commit)
    };
    let first = frame(&mut scanout);
    let Some(before) = descriptors() else {
        eprintln!("{test}: /proc/self/fd cannot be read, so nothing was asserted");
        scanout.release(&machine.device);
        return;
    };

    let mut refused = 0;
    for _ in 0..8 {
        if frame(&mut scanout).is_err() {
            refused += 1;
        }
        assert_eq!(
            descriptors().expect("/proc/self/fd was readable a moment ago"),
            before,
            "a frame left a descriptor open"
        );
    }

    match first {
        Ok(_) => eprintln!(
            "{test}: this process holds the device, so {} frames reached the screen and none of \
             them left a descriptor behind",
            8 - refused + 1
        ),
        Err(refusal) => eprintln!(
            "{test}: {refused} of 8 frames were refused at the commit and none of them left a \
             descriptor behind, which is what a process that is not DRM master gets: {refusal}"
        ),
    }

    scanout.release(&machine.device);
}

#[test]
fn a_display_that_composites_no_pointer_keeps_the_copied_shape() {
    let test = "a_display_that_composites_no_pointer_keeps_the_copied_shape";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };

    // Nothing can draw a software pointer into a tiled image from the processor, so the decision
    // is made before anything is allocated and it does not depend on what the two drivers could
    // otherwise agree on.
    let Err(reason) = Scanout::imported(&machine.device, &machine.output, &machine.gpu, false)
    else {
        panic!("a display whose frames have to carry the pointer was given tiled buffers");
    };
    assert!(
        matches!(reason, Copied::NoCursorPlane),
        "a display with no cursor plane was refused for another reason: {reason}"
    );
    assert!(
        !reason.to_string().is_empty(),
        "the reason a display is copied for is what the caller writes to a log"
    );
    eprintln!("{test}: {reason}");

    // And what it is driven from instead is two buffers the processor writes, with nothing for a
    // renderer to be pointed at.
    let scanout = Scanout::for_display(
        &machine.device,
        &machine.output,
        &machine.gpu,
        false,
        matches!(FORMAT, wgpu::TextureFormat::Bgra8Unorm),
    )
    .expect("every machine can allocate the buffers the copied shape uses");
    assert!(
        scanout.buffers().is_empty(),
        "a copied display holds no image a renderer draws into"
    );
    assert_eq!(
        scanout.framebuffers().len(),
        2,
        "the copied shape is two buffers"
    );
    let mut scanout = scanout;
    assert_eq!(
        scanout.acquire().expect("a copied display owes no barrier"),
        None,
        "a copied display names no buffer for a renderer to compose into"
    );
    scanout.release(&machine.device);
}

#[test]
fn a_frame_that_drew_nothing_leaves_the_next_one_the_buffer_it_took_back() {
    let test = "a_frame_that_drew_nothing_leaves_the_next_one_the_buffer_it_took_back";
    let _guard = device_lock();
    let Some(machine) = machine(test) else {
        return;
    };
    let Some(scanout) = imported(test, &machine) else {
        return;
    };

    // The display as whatever draws sees it: the images to compose into, and the two calls that
    // bracket one frame.
    let scanout = Rc::new(RefCell::new(scanout));
    let display = DrmDisplay::new(
        Arc::clone(&machine.device),
        Rc::new(RefCell::new(commit::for_device(&machine.device))),
        Rc::clone(&scanout),
        Rc::new(RefCell::new(Cursor::new(
            &machine.device,
            &machine.output,
            &mut Vec::new(),
        ))),
    );

    let textures = display.textures();
    assert_eq!(
        textures.len(),
        BUFFERS,
        "a display on this shape hands out one texture per buffer"
    );
    assert_eq!(
        Supplied::unusable(&textures),
        None,
        "the renderer refuses the textures this display hands it"
    );

    // A frame that drew nothing gives nothing over, so the buffer stays in this queue family and
    // the frame after it is pointed at the same one. Taking it back again is what has to keep
    // working: a buffer this side of the handover is acquired by doing nothing.
    assert_eq!(
        display.acquire().expect("a new display owes no barrier"),
        Some(0),
        "the first frame goes into the first buffer"
    );
    assert_eq!(
        display
            .acquire()
            .expect("a buffer nothing gave over is taken back by doing nothing"),
        Some(0),
        "a frame that drew nothing left the buffer where the next one needs it"
    );

    // And a frame that did draw gives it over. A machine holding DRM master flips here and the
    // frame after it goes to the next buffer; a machine without master is refused at the commit,
    // which happens after the barrier has run — so the display engine holds the buffer either way
    // and the acquire below is a barrier that really runs.
    draw(&machine.gpu, &scanout.borrow().buffers()[0]);
    let next = match display.present_drawn() {
        Ok(true) => {
            eprintln!("{test}: this process holds the device, so the frame reached the screen");
            1
        }
        Ok(false) => panic!("nothing is outstanding in front of the first frame"),
        Err(refused) => {
            eprintln!(
                "{test}: the barrier ran and the commit was refused, which is what a process that \
                 is not DRM master gets: {refused}"
            );
            0
        }
    };
    assert_eq!(
        display
            .acquire()
            .expect("a buffer the display engine holds is taken back before the next frame"),
        Some(next),
        "the frame after a give-over draws into the buffer the display is not reading"
    );

    // The renderer goes first, because a scanout is released by value and the display names it.
    drop(display);
    Rc::try_unwrap(scanout)
        .expect("nothing else holds this display's buffers")
        .into_inner()
        .release(&machine.device);
}
