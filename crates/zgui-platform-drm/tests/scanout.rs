//! Putting a rendered frame on a real display: allocate, modeset, flip, wait for the completion.
//!
//! This is the one thing in this backend that no unit test can stand in for. A blit that steps the
//! wrong stride, a fourcc whose channels are the other way round and a flip whose completion never
//! arrives all pass every test that runs without hardware, and all three show up here as a picture
//! that is wrong or a wait that times out.
//!
//! It needs DRM master. A compositor holding the device is refused with `EPERM`, so this looks for
//! master, says on standard error that it did not get it, and returns — the shape `cargo xtask
//! ledger ignored` prescribes for a test that cannot be switched off. Run it on a free virtual
//! terminal to make it assert anything.

use std::time::{Duration, Instant};

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_drm::commit;
use zgui_drm::device::Interface;
use zgui_drm::{Device, Event};
use zgui_geom::{Device as DeviceSpace, DevicePx, Point, Rect, Scale, Size};
use zgui_platform_drm::{Output, Scanout};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, Pixels, wgpu};
use zgui_scene::{Paint, Quad, Scene};

/// How long a flip is waited for before the wait is called a failure.
///
/// A display at 60 Hz completes in under 17 milliseconds, and one at 24 in under 42. Two seconds
/// is a hundred frames of slack, so a wait that runs out is a flip that is never coming rather
/// than a slow display.
const DEADLINE: Duration = Duration::from_secs(2);

/// How long the wait sleeps between reads of the device.
const POLL: Duration = Duration::from_millis(2);

/// Returns a device this process is DRM master of, or nothing.
///
/// Two ways to get nothing, and each says which it was: no device at all, and a device somebody
/// else is driving.
fn master(test: &str) -> Option<Device> {
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
    if let Err(error) = device.become_master() {
        eprintln!(
            "{test}: this process is not DRM master, so nothing was asserted: {error}\n\
             something else is driving {} — run this on a free virtual terminal, with no \
             compositor holding the device",
            device.path().display()
        );
        return None;
    }
    Some(device)
}

/// Returns one solid rectangle covering `width` x `height`, drawn and read back the way a frame
/// loop does.
///
/// Read back rather than invented, so that what reaches the display has been through the renderer
/// and carries whatever channel order the renderer's own surface format gives it. That is the half
/// of the format decision no unit test can see.
fn frame(test: &str, width: u32, height: u32) -> Option<Pixels> {
    let size: Size<i32, DeviceSpace> = Size::new(width as i32, height as i32);
    let target = RenderTarget::new(size, Scale::new(1.0));
    let mut renderer =
        match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
            Ok(renderer) => renderer,
            Err(failure) => {
                eprintln!("{test}: no usable graphics device, so nothing was asserted: {failure}");
                return None;
            }
        };

    let mut scene = Scene::new();
    scene.begin_frame(size);
    // A colour nothing else on a console produces, so a person watching can tell this frame from
    // whatever the terminal left behind.
    let paint = scene
        .paints
        .add(Paint::Solid(Color::srgb_u8(0, 120, 200, 255)));
    scene.push_quad(Quad::filled(
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width as f32), DevicePx(height as f32)),
        ),
        paint,
    ));
    scene.finish(&DamageSet::full());

    let outcome = renderer.draw(&scene, &DamageSet::full());
    assert!(
        outcome.stats().is_some(),
        "a frame composed into a texture always reaches it, and this one reported {outcome:?}"
    );
    renderer.read_presented()
}

/// Waits for a flip on `crtc`, reporting whether it arrived inside [`DEADLINE`].
///
/// The device is read once per turn and the whole slice is handed to the scanout, which is the
/// shape the frame loop reads events in: one read carries every CRTC that finished.
fn await_flip(device: &Device, scanout: &mut Scanout, crtc: u32) -> bool {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        let events = device.poll_events().expect("the device can be read");
        scanout.drain(&events);
        if events
            .iter()
            .any(|event| matches!(event, Event::FlipComplete { crtc: done, .. } if *done == crtc))
        {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

#[test]
fn a_rendered_frame_reaches_a_display_and_the_flip_reports_back() {
    let test = "a_rendered_frame_reaches_a_display_and_the_flip_reports_back";
    let Some(device) = master(test) else {
        return;
    };

    let outputs = Output::discover(&device).expect("the device is readable");
    let Some(output) = outputs.first() else {
        eprintln!("{test}: no display is plugged in, so nothing was asserted");
        drop(device.drop_master());
        return;
    };
    let (width, height) = (output.mode.width(), output.mode.height());
    let Some(pixels) = frame(test, width, height) else {
        drop(device.drop_master());
        return;
    };

    println!(
        "connector {} crtc {} plane {} at {width}x{height}, frame read back {}",
        output.pipe.connector,
        output.pipe.crtc,
        output.pipe.plane,
        if pixels.is_bgra() { "BGRA" } else { "RGBA" },
    );

    let mut scanout =
        Scanout::new(&device, output, pixels.is_bgra()).expect("the mode is set on this display");
    let mut commit = commit::for_device(&device);

    assert!(
        scanout
            .present(&device, &mut *commit, &pixels)
            .expect("the driver accepts the first flip"),
        "nothing is outstanding in front of the first frame"
    );
    assert!(
        !scanout
            .present(&device, &mut *commit, &pixels)
            .expect("a refused frame is not an error"),
        "a second frame before the completion is declined rather than written over the buffer \
         that is still on screen"
    );
    assert!(
        await_flip(&device, &mut scanout, output.pipe.crtc),
        "a flip on a display running at {} mHz completes well inside {DEADLINE:?}",
        output.mode.refresh_rate_millihertz()
    );

    // The buffer the first frame was written into is now the one on screen, and the other is free.
    // A second frame proves the pair really rotates rather than the first one having worked once.
    assert!(
        scanout
            .present(&device, &mut *commit, &pixels)
            .expect("the driver accepts the second flip"),
        "the completion freed the other buffer"
    );
    assert!(
        await_flip(&device, &mut scanout, output.pipe.crtc),
        "and the second flip completes too"
    );

    println!(
        "two frames reached crtc {} and both flips completed",
        output.pipe.crtc
    );
    scanout.release(&device);
    drop(device.drop_master());
}
