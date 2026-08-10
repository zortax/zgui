//! Setting a mode on a real device, and flipping a buffer under it.
//!
//! This is the crate end to end: a mode, a buffer drawn into, a scanout, a second buffer, and the
//! completion the flip does not carry. It runs twice, once through each commit interface. Asking
//! for the legacy interface on an atomic device exercises the legacy path on ordinary hardware,
//! and it is the only coverage that path gets.
//!
//! # What this needs to assert anything
//!
//! A free virtual terminal, or root. Modesetting takes DRM master, and a compositor holds master
//! on any ordinary desktop. A run under a compositor reports that and returns.
//!
//! `sudo modprobe vkms` gives a device with no hardware behind it, which is a display to set a
//! mode on that nobody is looking at. `vkms` is atomic, so the legacy half of this file runs
//! against its legacy ioctls the same way it runs against a real card.

mod support;

use std::thread;
use std::time::{Duration, Instant};

use zgui_drm::buffer::DumbBuffer;
use zgui_drm::commit::{Pipe, for_device, waits_for_a_fence};
use zgui_drm::device::Interface;
use zgui_drm::format::Format;
use zgui_drm::{Device, Event};

/// The format both buffers are in.
const FORMAT: Format = Format::XRGB8888;

/// What the first buffer holds.
const FIRST: u32 = 0x00ff_0000;

/// What the second buffer holds.
///
/// Different from the first on purpose: a flip to a buffer holding the same pixels changes nothing
/// on screen and proves nothing about the flip.
const SECOND: u32 = 0x0000_00ff;

/// How long a flip is given to complete.
///
/// A 60 Hz display flips in under 17 ms. A second is long enough that a loaded machine still
/// passes, and short enough that a broken one fails while somebody is still watching.
const DEADLINE: Duration = Duration::from_secs(1);

/// How long the wait sleeps between reads.
///
/// The device is non-blocking, so the wait would otherwise spin for the whole deadline.
const POLL: Duration = Duration::from_millis(1);

#[test]
fn a_mode_is_set_and_a_buffer_flipped_through_the_atomic_interface() {
    set_a_mode_and_flip(
        "a_mode_is_set_and_a_buffer_flipped_through_the_atomic_interface",
        Interface::Preferred,
    );
}

#[test]
fn a_mode_is_set_and_a_buffer_flipped_through_the_legacy_interface() {
    set_a_mode_and_flip(
        "a_mode_is_set_and_a_buffer_flipped_through_the_legacy_interface",
        Interface::Legacy,
    );
}

#[test]
fn only_the_atomic_interface_can_be_told_to_wait_for_a_fence() {
    let test = "only_the_atomic_interface_can_be_told_to_wait_for_a_fence";
    // No master is taken here. Reading a plane's properties is a question, and the answer is what
    // decides whether a caller that draws with a graphics device makes a sync file at all.
    let Some(atomic) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !atomic.is_atomic() {
        eprintln!("{test}: this device has no atomic interface, so nothing was asserted");
        return;
    }
    let Some(plane) = support::primary_plane(&atomic, 0) else {
        eprintln!("{test}: the first CRTC has no primary plane, so nothing was asserted");
        return;
    };

    // The same plane through the same driver, asked twice. The answer differs because the
    // interface differs and for no other reason: a fence reaches the kernel as a plane property,
    // and the legacy interface commits none.
    let Some(legacy) = support::device(test, Interface::Legacy) else {
        return;
    };
    assert!(
        !waits_for_a_fence(&legacy, plane).expect("a device on the legacy interface asks nothing"),
        "a device that commits no plane property was said to carry a fence"
    );

    let carries = waits_for_a_fence(&atomic, plane).expect("a primary plane can be read");
    eprintln!(
        "{test}: plane {plane} on {} {} IN_FENCE_FD",
        atomic.path().display(),
        if carries { "carries" } else { "carries no" }
    );
}

/// Puts a display into its preferred mode, scans a buffer out, and flips to a second one.
fn set_a_mode_and_flip(test: &str, interface: Interface) {
    let Some(device) = support::device(test, interface) else {
        return;
    };

    // Asking for the legacy interface has to get the legacy interface. Without this the two tests
    // above are one test run twice, and the legacy commit path has no coverage anywhere.
    if interface == Interface::Legacy {
        assert!(
            !device.is_atomic(),
            "a device opened for the legacy interface drives the legacy path, which is the only \
             coverage that path has"
        );
    }

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

    // The first CRTC. Its place in the list counts as well as its id, because a plane states the
    // CRTCs it can drive as a mask over this list rather than by id.
    const CRTC_INDEX: usize = 0;
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
    let pipe = Pipe {
        connector: connector.id,
        crtc,
        plane,
    };
    println!(
        "{test}: {} connector {} at {}x{} on CRTC {crtc} plane {plane}",
        device.path().display(),
        connector.id,
        mode.width(),
        mode.height(),
    );

    let mut front = device
        .create_dumb_buffer(mode.width(), mode.height(), FORMAT)
        .expect("the driver allocates a dumb buffer");
    let mut back = device
        .create_dumb_buffer(mode.width(), mode.height(), FORMAT)
        .expect("the driver allocates a dumb buffer");
    fill(&mut front, &device, FIRST);
    fill(&mut back, &device, SECOND);

    let shown = device
        .add_framebuffer(&front, FORMAT)
        .expect("the driver accepts the first buffer for scanout");
    let next = device
        .add_framebuffer(&back, FORMAT)
        .expect("the driver accepts the second buffer for scanout");

    let mut commit = for_device(&device);
    assert_eq!(
        commit.can_test(),
        device.is_atomic(),
        "only the atomic interface validates a configuration before it applies it"
    );

    commit
        // No fence: the buffers here are written by the processor, so the picture is already in
        // them when the commit is issued.
        .modeset(&device, pipe, &mode, shown, None)
        .expect("the device takes the mode");
    commit
        .flip(&device, pipe, next, None)
        .expect("the device takes the flip");

    // The flip returned at once, and it has not happened until the device says so. Waiting for it
    // paces a frame loop with no compositor under it, so the test waits the same way. A flip that
    // never completes stalls such a loop forever, so the deadline fails the test here rather than
    // hanging.
    let completion = first_completion(&device).unwrap_or_else(|| {
        panic!(
            "the flip completes within {DEADLINE:?}; a buffer that never comes back stalls a \
                frame loop"
        )
    });
    let Event::FlipComplete {
        crtc: flipped, at, ..
    } = completion
    else {
        panic!("a page flip reports that it completed")
    };
    assert_eq!(flipped, crtc, "the completion names the CRTC that flipped");
    println!("{test}: CRTC {flipped} flipped at {at:?}");

    device
        .remove_framebuffer(next)
        .expect("a framebuffer is released");
    device
        .remove_framebuffer(shown)
        .expect("a framebuffer is released");
    device
        .destroy_dumb_buffer(back)
        .expect("a dumb buffer is released");
    device
        .destroy_dumb_buffer(front)
        .expect("a dumb buffer is released");
    device.drop_master().expect("master is given up");
}

/// Returns the first completion the device reports, or nothing once [`DEADLINE`] passes.
fn first_completion(device: &Device) -> Option<Event> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        if let Some(event) = device
            .poll_events()
            .expect("the device reports what happened")
            .into_iter()
            .next()
        {
            return Some(event);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL);
    }
}

/// Writes `colour` over every pixel of `buffer`.
///
/// Rows step by the stride rather than by the width. A driver rounds a row up for its own reasons,
/// and a fill that stepped by the width would write a diagonal.
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
