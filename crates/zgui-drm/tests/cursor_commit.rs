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
use zgui_drm::commit::{AtomicCommit, Commit, LegacyCommit, Pipe, for_device};
use zgui_drm::cursor::{CursorImage, CursorPlane};
use zgui_drm::device::Interface;
use zgui_drm::format::Format;
use zgui_drm::{Device, Error};

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
    if !support::master(test, &device) {
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
        let Some(plane) = support::primary_plane(&device, CRTC_INDEX) else {
            eprintln!("{test}: CRTC {crtc} has no primary plane, so nothing was asserted");
            return;
        };
        plane
    } else {
        // The legacy interface addresses the CRTC, so there is no plane object to name.
        0
    };

    // The atomic interface names the cursor by its plane. The legacy one names the CRTC and reads
    // no plane at all, which is why `None` is a working target there rather than a reason to stop.
    let id = device
        .cursor_plane(CRTC_INDEX, &[])
        .expect("the device answers what cursor plane it has");
    if device.is_atomic() && id.is_none() {
        eprintln!("{test}: CRTC {crtc} has no cursor plane, so nothing was asserted");
        return;
    }
    let cursor = CursorPlane { crtc, id };

    let size = device.cursor_size();
    println!(
        "{test}: {} connector {} at {}x{} on CRTC {crtc} plane {plane}, cursor {}x{} on plane {}",
        device.path().display(),
        connector.id,
        mode.width(),
        mode.height(),
        size.width,
        size.height,
        cursor.id.unwrap_or_default(),
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

    // The legacy interface reads a cursor buffer with a format and a stride of its own, and this
    // is where a driver that rounded the row up is caught. Reporting it here names both strides,
    // where a failure inside `set_cursor` would name neither.
    if u64::from(image.stride()) != CursorImage::legacy_stride(size.width) {
        eprintln!(
            "{test}: this driver gives a {}x{} cursor buffer a stride of {}, and the legacy \
             interface reads {} — so nothing was asserted",
            size.width,
            size.height,
            image.stride(),
            CursorImage::legacy_stride(size.width),
        );
        return;
    }

    // Both names of the same buffer. Which one is read is the interface's decision: the atomic
    // path sets `FB_ID` from the framebuffer, and `DRM_IOCTL_MODE_CURSOR2` names the GEM handle.
    let image_id = CursorImage {
        framebuffer: Some(pointer),
        handle: image.handle(),
        width: size.width,
        height: size.height,
        stride: image.stride(),
        format: CURSOR,
        // The tip of an arrow is its top left corner, and this test draws a square.
        hotspot_x: 0,
        hotspot_y: 0,
    };

    let mut commit = for_device(&device);
    commit
        .modeset(&device, pipe(connector.id, crtc, plane), &mode, shown)
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

/// Returns an image with no buffer behind it.
///
/// Every refusal below happens before a commit is built or a request is issued, so none of them
/// reads this, and every one of them asserts under a compositor holding master.
fn unbacked() -> CursorImage {
    CursorImage {
        framebuffer: None,
        handle: 0,
        width: 64,
        height: 64,
        stride: 256,
        format: CURSOR,
        hotspot_x: 0,
        hotspot_y: 0,
    }
}

#[test]
fn an_atomic_commit_refuses_a_cursor_it_cannot_name() {
    let Some(device) = support::device(
        "an_atomic_commit_refuses_a_cursor_it_cannot_name",
        Interface::Preferred,
    ) else {
        return;
    };

    let mut commit = AtomicCommit::new();

    // `Device::cursor_plane` answers `None` where the device offers no cursor plane, and the
    // atomic interface has no other way to name a cursor. A caller that gets this composites the
    // pointer into the frame instead.
    let nowhere = CursorPlane { crtc: 1, id: None };
    for refused in [
        commit.set_cursor(&device, nowhere, unbacked(), 0, 0),
        commit.move_cursor(&device, nowhere, 0, 0),
        commit.hide_cursor(&device, nowhere),
    ] {
        let error = refused.expect_err("an atomic commit cannot name a cursor with no plane");
        assert!(
            matches!(&error, Error::Unusable(what) if what.contains("cursor plane")),
            "the refusal says what is missing rather than reporting an ioctl: {error}"
        );
    }

    // A plane and no framebuffer. `FB_ID` is the only way a plane names an image, and a caller
    // that registered none is one written for the legacy interface, where none is needed.
    let somewhere = CursorPlane {
        crtc: 1,
        id: Some(1),
    };
    let error = commit
        .set_cursor(&device, somewhere, unbacked(), 0, 0)
        .expect_err("an atomic commit cannot put an image it cannot name on a plane");
    assert!(
        matches!(&error, Error::Unusable(what) if what.contains("framebuffer")),
        "the refusal names the framebuffer rather than reporting an ioctl: {error}"
    );
}

#[test]
fn the_legacy_interface_refuses_a_cursor_it_would_read_as_something_else() {
    let Some(device) = support::device(
        "the_legacy_interface_refuses_a_cursor_it_would_read_as_something_else",
        Interface::Legacy,
    ) else {
        return;
    };

    let mut commit = LegacyCommit::new();
    let target = CursorPlane { crtc: 1, id: None };

    // `XRGB8888` is what everything else in this crate scans out, so it is the mistake a caller
    // makes. The kernel reads a cursor buffer as `ARGB8888` whatever it is told, so the unused
    // byte becomes the alpha channel: the cursor is invisible and every call reports success.
    // That is the one failure nothing downstream can see, and this is where it becomes an error.
    let error = commit
        .set_cursor(
            &device,
            target,
            CursorImage {
                format: Format::XRGB8888,
                ..unbacked()
            },
            0,
            0,
        )
        .expect_err("a format this interface would reinterpret is refused");
    assert!(
        matches!(&error, Error::Unusable(what) if what.contains("reinterpreted")),
        "the refusal says what the kernel would do with it: {error}"
    );

    // A driver rounds a dumb buffer's rows up for its own reasons, and this interface reads four
    // bytes a pixel with no rounding.
    let error = commit
        .set_cursor(
            &device,
            target,
            CursorImage {
                stride: 512,
                ..unbacked()
            },
            0,
            0,
        )
        .expect_err("a stride this interface would read past is refused");
    assert!(
        matches!(&error, Error::Unusable(what) if what.contains("sheared")),
        "the refusal says what the kernel would do with it: {error}"
    );
}

/// Returns the pipe a modeset is applied to.
fn pipe(connector: u32, crtc: u32, plane: u32) -> Pipe {
    Pipe {
        connector,
        crtc,
        plane,
    }
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
