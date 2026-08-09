//! Putting a cursor on a display, moving it, and taking it away.
//!
//! It runs twice, once through each commit interface. Asking for the legacy interface on an atomic
//! device exercises `DRM_IOCTL_MODE_CURSOR2` on ordinary hardware, and it is the only coverage
//! that request gets.
//!
//! # What this needs to assert anything
//!
//! A free virtual terminal, or root. A cursor goes on a CRTC that is already showing a mode, so
//! this sets one first, and modesetting takes DRM master. A run under a compositor reports that and
//! returns.
//!
//! `sudo modprobe vkms` gives a device with no hardware behind it, which is a display to put a
//! cursor on that nobody is looking at.

mod support;

use zgui_drm::buffer::DumbBuffer;
use zgui_drm::commit::{Commit, Pipe, for_device};
use zgui_drm::cursor::{CursorImage, CursorPlane};
use zgui_drm::device::Interface;
use zgui_drm::format::Format;
use zgui_drm::property::ObjectKind;
use zgui_drm::{Device, Error};

/// What the kernel numbers `DRM_PLANE_TYPE_PRIMARY`.
///
/// Named here because `sys` is private: a test reaches this crate the way any other caller does.
const PRIMARY: u64 = 1;

/// The format the display scans out.
const SCANOUT: Format = Format::XRGB8888;

/// The format a cursor is in.
///
/// A cursor is composited over the frame, so it needs the alpha channel that a scanout format
/// leaves unused.
const CURSOR: Format = Format::ARGB8888;

/// What the display holds under the cursor.
const BACKGROUND: u32 = 0x0000_0080;

/// What the cursor holds: opaque white.
const FOREGROUND: u32 = 0xffff_ffff;

/// The first CRTC.
///
/// Its place in the resource list counts as well as its id, because a plane states the CRTCs it
/// can drive as a mask over that list.
const CRTC_INDEX: usize = 0;

#[test]
fn a_cursor_is_put_on_a_display_moved_and_taken_away_through_the_atomic_interface() {
    drive_a_cursor(
        "a_cursor_is_put_on_a_display_moved_and_taken_away_through_the_atomic_interface",
        Interface::Preferred,
    );
}

#[test]
fn a_cursor_is_put_on_a_display_moved_and_taken_away_through_the_legacy_interface() {
    drive_a_cursor(
        "a_cursor_is_put_on_a_display_moved_and_taken_away_through_the_legacy_interface",
        Interface::Legacy,
    );
}

/// Sets a mode, puts a cursor on the CRTC, moves it, and takes it off again.
fn drive_a_cursor(test: &str, interface: Interface) {
    let Some(device) = support::device(test, interface) else {
        return;
    };

    // Modesetting takes master, and a compositor holds it. Saying so is the honest outcome, and it
    // is what `cargo xtask ledger ignored` asks for in place of switching the test off.
    if let Err(error) = device.become_master() {
        eprintln!(
            "{test}: cannot take DRM master on {}, so nothing was asserted: {error}\n\
             run this from a free virtual terminal, or as root",
            device.path().display()
        );
        return;
    }
    if !device.supports_dumb_buffers() {
        eprintln!("{test}: this device has no dumb buffers, so nothing was asserted");
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    let Some(connector) = resources
        .connectors
        .iter()
        .filter_map(|id| device.connector(*id).ok())
        .find(|connector| connector.is_connected() && connector.preferred_mode().is_some())
    else {
        eprintln!("{test}: no display is plugged in, so nothing was asserted");
        return;
    };
    let mode = *connector
        .preferred_mode()
        .expect("a connector kept for its preferred mode has one");
    let crtc = *resources
        .crtcs
        .get(CRTC_INDEX)
        .expect("a modesetting device has a CRTC");

    let plane = if device.is_atomic() {
        let Some(plane) = primary_plane(&device, CRTC_INDEX) else {
            eprintln!("{test}: CRTC {crtc} has no primary plane, so nothing was asserted");
            return;
        };
        plane
    } else {
        // The legacy interface addresses the CRTC, so there is no plane object to name.
        0
    };

    // The atomic interface names the cursor by its plane. The legacy one names the CRTC and reads
    // no plane at all, which is why this is zero there rather than a reason to stop.
    let cursor = CursorPlane {
        crtc,
        id: if device.is_atomic() {
            match device.cursor_plane(CRTC_INDEX) {
                Ok(Some(id)) => id,
                Ok(None) => {
                    eprintln!("{test}: CRTC {crtc} has no cursor plane, so nothing was asserted");
                    return;
                }
                Err(error) => panic!("the device answers what cursor plane it has: {error}"),
            }
        } else {
            0
        },
    };

    let size = device.cursor_size();
    println!(
        "{test}: {} connector {} at {}x{} on CRTC {crtc} plane {plane}, cursor {}x{} on plane {}",
        device.path().display(),
        connector.id,
        mode.width(),
        mode.height(),
        size.width,
        size.height,
        cursor.id,
    );

    let mut frame = device
        .create_dumb_buffer(mode.width(), mode.height(), SCANOUT)
        .expect("the driver allocates a dumb buffer");
    let mut image = device
        .create_dumb_buffer(size.width, size.height, CURSOR)
        .expect("the driver allocates a cursor-sized dumb buffer");
    fill(&mut frame, &device, BACKGROUND);
    fill(&mut image, &device, FOREGROUND);

    let shown = device
        .add_framebuffer(&frame, SCANOUT)
        .expect("the driver accepts the frame for scanout");
    let pointer = device
        .add_framebuffer(&image, CURSOR)
        .expect("the driver accepts the cursor image for scanout");

    // Both names of the same buffer. Which one is read is the interface's decision: the atomic
    // path sets `FB_ID` from the framebuffer, and `DRM_IOCTL_MODE_CURSOR2` names the GEM handle.
    let image_id = CursorImage {
        framebuffer: pointer,
        handle: image.handle(),
        width: size.width,
        height: size.height,
        // The tip of an arrow is its top left corner, and this test draws a square.
        hotspot_x: 0,
        hotspot_y: 0,
    };

    let mut commit = for_device(&device);
    commit
        .modeset(&device, pipe(&connector.id, crtc, plane), &mode, shown)
        .expect("the device takes the mode");

    // A cursor goes on somewhere inside the display, so that a driver clipping it against the CRTC
    // cannot be what makes this pass.
    commit
        .set_cursor(&device, cursor, image_id, 100, 100)
        .expect("the device takes the cursor image");
    // A move touches no buffer. It is the operation a hardware cursor exists for.
    commit
        .move_cursor(&device, cursor, 400, 300)
        .expect("the device moves the cursor");
    // The position is signed, and a pointer near the left or the top edge sits at a negative one
    // once its hotspot is taken off. A conversion that widened 32 bits rather than sign-extending
    // 64 would put the cursor four thousand million pixels to the right, and the kernel refuses
    // that on the atomic path.
    commit
        .move_cursor(&device, cursor, -16, -16)
        .expect("the device takes a cursor hanging off the top left corner");
    commit
        .hide_cursor(&device, cursor)
        .expect("the device takes the cursor away");

    device
        .remove_framebuffer(pointer)
        .expect("a framebuffer is released");
    device
        .remove_framebuffer(shown)
        .expect("a framebuffer is released");
    device
        .destroy_dumb_buffer(image)
        .expect("a dumb buffer is released");
    device
        .destroy_dumb_buffer(frame)
        .expect("a dumb buffer is released");
    device.drop_master().expect("master is given up");
}

#[test]
fn an_atomic_commit_refuses_a_cursor_on_a_crtc_that_has_no_cursor_plane() {
    let Some(device) = support::device(
        "an_atomic_commit_refuses_a_cursor_on_a_crtc_that_has_no_cursor_plane",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.supports_dumb_buffers() {
        eprintln!("this device has no dumb buffers, so there is no image to refuse");
        return;
    }
    if !device.is_atomic() {
        eprintln!("this device is not atomic, so the atomic interface is not the one it gets");
        return;
    }

    // A real image, because a `CursorImage` names a framebuffer the device issued. Nothing below
    // reaches the kernel with it: the refusal happens before the commit is built, so this asserts
    // under a compositor holding master.
    let buffer = device
        .create_dumb_buffer(64, 64, CURSOR)
        .expect("the driver allocates a dumb buffer");
    let framebuffer = device
        .add_framebuffer(&buffer, CURSOR)
        .expect("the driver accepts a cursor image for scanout");
    let image = CursorImage {
        framebuffer,
        handle: buffer.handle(),
        width: 64,
        height: 64,
        hotspot_x: 0,
        hotspot_y: 0,
    };

    // Zero is not an object id: the kernel's allocator starts at one. So it is what a device with
    // no cursor plane reaches a commit as, and the atomic interface has no other way to name a
    // cursor. A caller that gets this composites the pointer into the frame instead.
    let mut commit = zgui_drm::commit::AtomicCommit::new();
    let crtc = CursorPlane { crtc: 1, id: 0 };
    for refused in [
        commit.set_cursor(&device, crtc, image, 0, 0),
        commit.move_cursor(&device, crtc, 0, 0),
        commit.hide_cursor(&device, crtc),
    ] {
        let error = refused.expect_err("an atomic commit cannot name a cursor with no plane");
        assert!(
            matches!(&error, Error::Unusable(what) if what.contains("cursor plane")),
            "the refusal says what is missing rather than reporting an ioctl: {error}"
        );
    }

    device
        .remove_framebuffer(framebuffer)
        .expect("a framebuffer is released");
    device
        .destroy_dumb_buffer(buffer)
        .expect("a dumb buffer is released");
}

/// Returns the pipe a modeset is applied to.
fn pipe(connector: &u32, crtc: u32, plane: u32) -> Pipe {
    Pipe {
        connector: *connector,
        crtc,
        plane,
    }
}

/// The primary plane that can drive the CRTC at `crtc_index`, where the device has one.
fn primary_plane(device: &Device, crtc_index: usize) -> Option<u32> {
    device.planes().ok()?.into_iter().find(|id| {
        let Ok(plane) = device.plane(*id) else {
            return false;
        };
        plane.drives(crtc_index)
            && device
                .properties(*id, ObjectKind::Plane)
                .ok()
                .and_then(|properties| properties.value("type"))
                == Some(PRIMARY)
    })
}

/// Writes `colour` over every pixel of `buffer`.
///
/// Rows step by the stride rather than by the width, because a driver rounds a row up for its own
/// reasons and a fill that stepped by the width would write a diagonal.
fn fill(buffer: &mut DumbBuffer, device: &Device, colour: u32) {
    let width = buffer.width() as usize;
    let height = buffer.height() as usize;
    let stride = buffer.stride() as usize;
    let pixel = colour.to_ne_bytes();
    let bytes = buffer.bytes(device).expect("a dumb buffer maps");

    for row in bytes.chunks_mut(stride).take(height) {
        for target in row.chunks_exact_mut(pixel.len()).take(width) {
            target.copy_from_slice(&pixel);
        }
    }
}
