//! Carrying a buffer from one card to another, which is what a machine with two of them needs.
//!
//! `tests/prime.rs` exports and imports on one device, which is the round trip a graphics API makes
//! against the card it already owns. This is the other shape: a machine that **draws on one card
//! and displays on another**, where the buffer has to cross between two drivers that share nothing
//! but the kernel's dma-buf interface.
//!
//! It is the question a split machine turns on. A renderer that cannot hand its frame to the
//! display device has to read every frame back and copy it through the processor, which is the
//! shape `zgui-platform-drm` calls the copied one. What this file asserts is whether the other
//! shape is reachable at all, per direction, on the hardware it is run against.
//!
//! # What this needs to assert anything
//!
//! **The crossing.** Two cards that open. Exporting and importing take no DRM master, so it runs
//! under a compositor and needs no free terminal. A machine with one card says so and asserts
//! nothing.
//!
//! **The scanout.** The same two cards, a display plugged into one of them, and DRM master on that
//! one. `ADDFB2` accepting an imported handle says the kernel checked the format, the stride and
//! the size; it does not say the display engine can fetch from memory another card allocated. Only
//! putting it on the screen says that, so the second test does.
//!
//! # Why the outcome is reported rather than required
//!
//! Two drivers sharing memory is a property of the pair, not a bug in either one. A display engine
//! reaches the memory it can reach: one that scans out of system memory takes an imported buffer,
//! and one that scans out of its own video memory does not. So each direction is attempted and
//! named, and what is *asserted* is the part that would be a fault — that an import which succeeded
//! produces a handle the same device will build a framebuffer from.

mod support;

use std::os::fd::AsFd;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use zgui_drm::buffer::DumbBuffer;
use zgui_drm::commit::{Pipe, for_device};
use zgui_drm::device::Interface;
use zgui_drm::format::{Format, Modifier};
use zgui_drm::{Device, Event};

/// How long a flip is waited for before the wait is called a failure.
const DEADLINE: Duration = Duration::from_secs(2);

/// How long the wait sleeps between reads of the device.
const POLL: Duration = Duration::from_millis(2);

/// How long the crossed frame is left on the screen.
///
/// Nothing here can read the display back, so whether the picture is *right* is a thing only a
/// person watching can answer. Two seconds is long enough to see one solid colour replace another,
/// which is what tells a correct stride from one that shears the image.
const HOLD: Duration = Duration::from_secs(2);

/// The colour the mode is set with: green.
const FIRST: u32 = 0x0000_ff00;

/// The colour flipped to: blue.
const SECOND: u32 = 0x0000_00ff;

/// How wide the buffer under test is.
///
/// A scanout-sized buffer rather than a token one. A display engine states what it can scan out in
/// its stride and its alignment, and a sixty-four pixel buffer can meet those where a real frame
/// does not.
const WIDTH: u32 = 1280;

/// How tall the buffer under test is.
const HEIGHT: u32 = 1024;

/// Returns every card that opens, with the path it opened at.
fn cards_that_open(test: &str) -> Vec<(PathBuf, Device)> {
    let Ok(paths) = zgui_drm::cards() else {
        eprintln!("{test}: /dev/dri cannot be read, so nothing was asserted");
        return Vec::new();
    };

    paths
        .into_iter()
        .filter_map(|path| {
            let device = Device::open_with(&path, Interface::Preferred).ok()?;
            Some((path, device))
        })
        .collect()
}

/// What one direction of the crossing did.
enum Crossing {
    /// The importing card built a framebuffer from the buffer the exporting card allocated.
    Scanout,
    /// The importing card took the descriptor and refused it for scanout.
    ImportOnly(String),
    /// The importing card refused the descriptor.
    Refused(String),
}

/// Carries one buffer from `from` to `to` and reports how far it got.
fn cross(from: &Device, to: &Device) -> Crossing {
    let buffer = match from.create_dumb_buffer(WIDTH, HEIGHT, Format::XRGB8888) {
        Ok(buffer) => buffer,
        Err(error) => return Crossing::Refused(format!("the buffer was not allocated: {error}")),
    };
    let stride = buffer.stride();

    let descriptor = match from.export_buffer(&buffer) {
        Ok(descriptor) => descriptor,
        Err(error) => return Crossing::Refused(format!("the buffer was not exported: {error}")),
    };

    let imported = match to.import_buffer(descriptor.as_fd()) {
        Ok(imported) => imported,
        Err(error) => {
            return Crossing::Refused(format!("the descriptor was not imported: {error}"));
        }
    };

    // The layout a dumb buffer has. Where the importing driver takes no modifiers the request
    // states none, which is the same buffer described the older way.
    let modifier = to.supports_format_modifiers().then_some(Modifier::LINEAR);
    let outcome = match to.add_framebuffer_from_handles(
        WIDTH,
        HEIGHT,
        Format::XRGB8888,
        [imported.handle(), 0, 0, 0],
        [stride, 0, 0, 0],
        [0; 4],
        modifier,
    ) {
        Ok(framebuffer) => {
            assert_ne!(framebuffer.id(), 0, "an accepted framebuffer has an id");
            let _ = to.remove_framebuffer(framebuffer);
            Crossing::Scanout
        }
        Err(error) => Crossing::ImportOnly(error.to_string()),
    };

    let _ = to.release_imported(imported);
    drop(descriptor);
    let _ = from.destroy_dumb_buffer(buffer);

    outcome
}

#[test]
fn a_buffer_crosses_between_two_cards_or_says_why_it_cannot() {
    let test = "a_buffer_crosses_between_two_cards_or_says_why_it_cannot";
    let cards = cards_that_open(test);
    if cards.len() < 2 {
        eprintln!(
            "{test}: this machine has {} card(s) that open, so there is nothing to cross between \
             and nothing was asserted\n\
             add a second one with `sudo modprobe vkms` to run it",
            cards.len()
        );
        return;
    }

    let mut reached_scanout = 0;
    for (from_path, from) in &cards {
        for (to_path, to) in &cards {
            if from_path == to_path {
                continue;
            }
            let from_name = from_path.display();
            let to_name = to_path.display();
            match cross(from, to) {
                Crossing::Scanout => {
                    reached_scanout += 1;
                    eprintln!(
                        "{test}: {from_name} -> {to_name}: imported and accepted for scanout, so \
                         a frame drawn on {from_name} can be displayed by {to_name} with no copy"
                    );
                }
                Crossing::ImportOnly(why) => eprintln!(
                    "{test}: {from_name} -> {to_name}: imported, and refused for scanout: {why}"
                ),
                Crossing::Refused(why) => {
                    eprintln!("{test}: {from_name} -> {to_name}: {why}");
                }
            }
        }
    }

    eprintln!(
        "{test}: {reached_scanout} of {} direction(s) reached scanout",
        cards.len() * (cards.len() - 1)
    );
}

/// Writes one colour over every pixel of `buffer`, which `device` owns.
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

/// Waits for the first event `device` reports, or gives up.
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

#[test]
fn a_frame_allocated_on_one_card_is_scanned_out_by_another() {
    let test = "a_frame_allocated_on_one_card_is_scanned_out_by_another";
    let cards = cards_that_open(test);
    if cards.len() < 2 {
        eprintln!(
            "{test}: this machine has {} card(s) that open, so there is nothing to cross between \
             and nothing was asserted",
            cards.len()
        );
        return;
    }

    // The display card is the one with a screen on it. The other is the one that would draw, and
    // on this machine that is the arrangement rather than a choice: the card with the screen is
    // not the card that can render.
    let Some((display_path, display)) = cards.iter().find(|(_, device)| {
        device
            .has_a_display()
            .expect("a device that enumerates answers what is plugged into it")
    }) else {
        eprintln!("{test}: no card has a display plugged in, so nothing was asserted");
        return;
    };
    let Some((render_path, render)) = cards.iter().find(|(path, _)| path != display_path) else {
        eprintln!("{test}: only the card with the display opens, so nothing was asserted");
        return;
    };

    if !support::master(test, display) {
        return;
    }
    if !render.supports_dumb_buffers() {
        eprintln!(
            "{test}: {} has no dumb buffers, so there is nothing to allocate on it",
            render_path.display()
        );
        return;
    }

    let resources = display.resources().expect("the device enumerates");
    let Some(connector) = resources
        .connectors
        .iter()
        .filter_map(|id| display.connector(*id).ok())
        .find(|connector| connector.is_connected() && connector.preferred_mode().is_some())
    else {
        eprintln!("{test}: no display is plugged in, so nothing was asserted");
        return;
    };
    let mode = *connector
        .preferred_mode()
        .expect("a connector kept for its preferred mode has one");

    const CRTC_INDEX: usize = 0;
    let crtc = *resources
        .crtcs
        .get(CRTC_INDEX)
        .expect("a modesetting device has a CRTC");
    let plane = if display.is_atomic() {
        let Some(plane) = support::primary_plane(display, CRTC_INDEX) else {
            eprintln!("{test}: CRTC {crtc} has no primary plane, so nothing was asserted");
            return;
        };
        plane
    } else {
        0
    };
    let pipe = Pipe {
        connector: connector.id,
        crtc,
        plane,
    };

    println!(
        "{test}: drawing on {}, displaying on {} — connector {} at {}x{} on CRTC {crtc}",
        render_path.display(),
        display_path.display(),
        connector.id,
        mode.width(),
        mode.height(),
    );

    // Both buffers belong to the *render* card. Nothing is allocated on the display card at all,
    // which is the whole point: what it scans out is memory another driver owns.
    let mut front = render
        .create_dumb_buffer(mode.width(), mode.height(), Format::XRGB8888)
        .expect("the render card allocates a dumb buffer");
    let mut back = render
        .create_dumb_buffer(mode.width(), mode.height(), Format::XRGB8888)
        .expect("the render card allocates a dumb buffer");
    // Written by the processor here because this test is about the crossing rather than about
    // drawing. A renderer would write these with the render card's own engine.
    fill(&mut front, render, FIRST);
    fill(&mut back, render, SECOND);

    let front_stride = front.stride();
    let back_stride = back.stride();
    let front_fd = render
        .export_buffer(&front)
        .expect("the render card exports a buffer");
    let back_fd = render
        .export_buffer(&back)
        .expect("the render card exports a buffer");

    let front_imported = display
        .import_buffer(front_fd.as_fd())
        .expect("the display card imports the descriptor");
    let back_imported = display
        .import_buffer(back_fd.as_fd())
        .expect("the display card imports the descriptor");

    let modifier = display
        .supports_format_modifiers()
        .then_some(Modifier::LINEAR);
    let shown = display
        .add_framebuffer_from_handles(
            mode.width(),
            mode.height(),
            Format::XRGB8888,
            [front_imported.handle(), 0, 0, 0],
            [front_stride, 0, 0, 0],
            [0; 4],
            modifier,
        )
        .expect("the display card accepts an imported buffer for scanout");
    let next = display
        .add_framebuffer_from_handles(
            mode.width(),
            mode.height(),
            Format::XRGB8888,
            [back_imported.handle(), 0, 0, 0],
            [back_stride, 0, 0, 0],
            [0; 4],
            modifier,
        )
        .expect("the display card accepts an imported buffer for scanout");

    let mut commit = for_device(display);
    commit
        .modeset(display, pipe, &mode, shown, None)
        .expect("the display card takes the mode with a buffer another card allocated");
    println!(
        "{test}: the mode is set on a buffer {} allocated",
        render_path.display()
    );
    thread::sleep(HOLD);

    commit
        .flip(display, pipe, next, None)
        .expect("the display card flips to a second buffer another card allocated");
    let completion = first_completion(display).unwrap_or_else(|| {
        panic!("the flip completes within {DEADLINE:?}; a buffer that never comes back stalls a frame loop")
    });
    let Event::FlipComplete {
        crtc: flipped, at, ..
    } = completion
    else {
        panic!("a page flip reports that it completed")
    };
    assert_eq!(flipped, crtc, "the completion names the CRTC that flipped");
    println!("{test}: CRTC {flipped} flipped at {at:?} — a crossed buffer reached the screen");
    thread::sleep(HOLD);

    display
        .remove_framebuffer(next)
        .expect("a framebuffer is released");
    display
        .remove_framebuffer(shown)
        .expect("a framebuffer is released");
    let _ = display.release_imported(back_imported);
    let _ = display.release_imported(front_imported);
    drop(back_fd);
    drop(front_fd);
    let _ = render.destroy_dumb_buffer(back);
    let _ = render.destroy_dumb_buffer(front);
    display.drop_master().expect("master is given up");
}
