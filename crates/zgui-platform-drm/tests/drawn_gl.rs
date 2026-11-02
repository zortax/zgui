//! Composing a frame straight into a buffer a display scans out of, on a machine with no Vulkan.
//!
//! The copied path reads a whole frame back to system memory and copies it into the display's
//! buffer. This is the path that does neither, built on OpenGL rather than on Vulkan: `libgbm`
//! allocates a buffer the display already owns, EGL imports the descriptor as a texture, and the
//! renderer composes into it where it lies.
//!
//! **The check is the buffer's own mapping, never the graphics API.** A copy into memory the device
//! cannot reach reports success, runs at an impossible speed and leaves the memory untouched; asking
//! GL what it drew answers with what it was told. Reading the allocation is the only question whose
//! answer cannot be a polite fiction.
//!
//! It needs no DRM master — nothing here sets a mode or flips — so it asserts on any machine with a
//! card, an OpenGL adapter and `libgbm`. A machine missing one of the three says so on standard
//! error and returns, the shape `cargo xtask ledger ignored` prescribes for a test that cannot be
//! switched off.

#![allow(
    unsafe_code,
    reason = "the timing below reaches wgpu's own GL context through its hal, which is unsafe to \
              take and safe to read through — the same access `import::gl` makes"
)]

use std::path::PathBuf;
use std::time::Instant;

use zgui_platform_drm::import::gbm;
use zgui_platform_drm::import::gl;
use zgui_render_wgpu::{Gpu, SharedGraphics, wgpu};

/// Small, because what is being asserted is that the pixels arrive rather than how many.
const WIDTH: u32 = 256;
const HEIGHT: u32 = 128;

/// The colour a frame is composed in, and the bytes it has to reach memory as.
///
/// `XR24` is blue first, so a fully blue frame is `[0xFF, 0x00, 0x00, _]` — the fourth byte is what
/// the scanout ignores, and nothing here asserts on it.
const BLUE: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

fn main() {
    let Some(gpu) = graphics() else {
        return;
    };
    if gpu.adapter().get_info().backend != wgpu::Backend::Gl {
        eprintln!(
            "this adapter is {:?} and this path is the OpenGL one, so there is nothing to assert",
            gpu.adapter().get_info().backend
        );
        return;
    }
    let Some(card) = card() else {
        return;
    };
    let library = match gbm::Library::load() {
        Ok(library) => library,
        Err(why) => {
            eprintln!("no libgbm on this machine, so nothing here can allocate: {why}");
            return;
        }
    };
    let node = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&card)
    {
        Ok(node) => node,
        Err(error) => {
            eprintln!("{} cannot be opened for writing: {error}", card.display());
            return;
        }
    };
    let allocator = match gbm::Device::new(&library, std::os::fd::AsFd::as_fd(&node)) {
        Ok(allocator) => allocator,
        Err(why) => {
            eprintln!("{} gives no allocator: {why}", card.display());
            return;
        }
    };
    what_the_chosen_allocator_gives(&gpu, &allocator, &card);
    what_naming_the_layout_does(&gpu, &allocator);
    what_a_dumb_buffer_does_instead(&gpu, &card);
    whether_a_padded_buffer_is_merely_padded(&gpu, &allocator);
    what_each_allocator_costs_to_fill(&gpu, &allocator, &card);
    what_one_call_into_this_driver_costs(&gpu);
    a_frame_composed_into_an_imported_buffer_reaches_the_buffer(&gpu, &allocator);
    a_frame_reaches_the_last_row_of_a_padded_buffer(&gpu, &allocator);
    a_frame_drawn_in_two_colours_reaches_memory_in_two_colours(&gpu, &allocator);
    every_buffer_of_a_set_is_its_own_memory(&gpu, &allocator);
    the_tier_that_signals_a_frame_is_one_this_device_has(&gpu);
    println!("drawn_gl: every assertion held on {}", allocator.backend());
}

/// The whole of it: allocate, import, compose, and read the memory back.
fn a_frame_composed_into_an_imported_buffer_reaches_the_buffer(gpu: &Gpu, allocator: &gbm::Device) {
    let mut buffers = match gl::create(gpu, allocator, WIDTH, HEIGHT, 1) {
        Ok(buffers) => buffers,
        Err(why) => {
            eprintln!("this machine cannot draw into a scanout buffer: {why}");
            std::process::exit(0);
        }
    };
    let drawn = buffers.pop().expect("one was asked for");

    // Nothing about the colour is read back through GL, so it cannot answer with what it was told.
    let before = drawn.peek().expect("a gbm buffer maps");
    assert_ne!(
        before[0], 0xFF,
        "the buffer already held the colour this is about to draw, so nothing would be proved"
    );
    a_scanout_buffer_answers_what_is_writing_it(&drawn);
    compose(gpu, drawn.texture(), BLUE);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let after = drawn.peek().expect("a gbm buffer maps");
    assert_eq!(
        after[0], 0xFF,
        "a blue frame reaches XR24 memory blue first, and the buffer holds {after:?}"
    );
    assert_eq!(
        [after[1], after[2]],
        [0x00, 0x00],
        "and nothing else, so the channel order agrees end to end: {after:?}"
    );
}

/// A frame has to reach every row of a buffer whose pitch the driver padded.
///
/// **Every other check here is at a width where the padding does nothing.** 256 pixels is 1024
/// bytes, which needs no rounding, so the buffer's rows are exactly `width x 4` apart and a
/// renderer that assumed as much is indistinguishable from one that read the stride. 1280 pixels is
/// 5120 bytes and i915 rounds a scanout pitch up to a power of two, which is 8192 — and there the
/// two answers differ by 3072 bytes a row.
///
/// The corner cannot see it: a renderer laying rows out at 5120 into a buffer whose rows are 8192
/// apart writes the first row exactly where it belongs. The last row is 1023 rows further down by
/// one reckoning and 1023 rows further down by another, and those are 3 MB apart. So this composes
/// the whole surface and reads a pixel out of the **last** row, which is only blue if both ends
/// laid the rows out the same way.
///
/// A driver that pads nothing says so and returns, because then there is no disagreement to find.
fn a_frame_reaches_the_last_row_of_a_padded_buffer(gpu: &Gpu, allocator: &gbm::Device) {
    // Wide enough that `width x 4` is not already a power of two, and short enough that the
    // allocation stays small.
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;

    let Ok(mut buffers) = gl::create(gpu, allocator, WIDE, TALL, 1) else {
        eprintln!("this machine cannot draw into a {WIDE}x{TALL} scanout buffer");
        return;
    };
    let drawn = buffers.pop().expect("one was asked for");
    if drawn.stride() == WIDE * 4 {
        println!(
            "drawn_gl: this driver padded nothing for a row of {WIDE}, so there is no stride to \
             disagree about"
        );
        return;
    }
    println!(
        "drawn_gl: a row of {WIDE} needs {} bytes and this driver gave {}",
        WIDE * 4,
        drawn.stride()
    );

    compose(gpu, drawn.texture(), BLUE);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let corner = drawn.peek_at(0, 0).expect("a gbm buffer maps");
    assert_eq!(
        corner[0], 0xFF,
        "the first row is where both answers agree, and even that did not arrive: {corner:?}"
    );
    let last = drawn.peek_at(0, TALL - 1).expect("a gbm buffer maps");
    assert_eq!(
        last[0],
        0xFF,
        "the frame reached the first row and not the last, so the renderer laid its rows out at \
         {} bytes and the buffer's are {} apart: {last:?}",
        WIDE * 4,
        drawn.stride()
    );
}

/// A frame drawn in two colours has to reach memory as two colours, in the right places.
///
/// **Every check above composes one flat colour, and a flat colour is invariant under every layout
/// error there is.** A buffer that is tiled, sheared or offset holds the same byte at every address
/// either way, so a uniform blue frame arrives "correctly" however the two ends disagree. This is
/// the check that can tell: it clears the surface blue, draws red into a rectangle whose corner is
/// nowhere near the origin, and then asks the memory where the red went.
///
/// Three reads settle it. Inside the rectangle has to be red; a pixel one column to its left and a
/// pixel one row above it have to still be blue. A tiled buffer read as linear puts the red in a
/// regular pattern of blocks elsewhere, and a stride the two ends disagree about puts it on the
/// wrong row — and either shows up as blue where red belongs.
fn a_frame_drawn_in_two_colours_reaches_memory_in_two_colours(gpu: &Gpu, allocator: &gbm::Device) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;
    // A corner off both axes, and off any power of two, so that an error in either direction moves
    // it somewhere this notices.
    const AT: (u32, u32) = (517, 21);

    let Ok(mut buffers) = gl::create(gpu, allocator, WIDE, TALL, 1) else {
        eprintln!("this machine cannot draw into a {WIDE}x{TALL} scanout buffer");
        return;
    };
    let drawn = buffers.pop().expect("one was asked for");

    compose(gpu, drawn.texture(), BLUE);
    paint_red_rectangle(gpu, drawn.texture(), AT.0, AT.1, 64, 16);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let inside = drawn
        .peek_at(AT.0 + 8, AT.1 + 4)
        .expect("a gbm buffer maps");
    let left = drawn
        .peek_at(AT.0 - 1, AT.1 + 4)
        .expect("a gbm buffer maps");
    let above = drawn
        .peek_at(AT.0 + 8, AT.1 - 1)
        .expect("a gbm buffer maps");

    println!(
        "drawn_gl: stride {}, inside {inside:?}, left {left:?}, above {above:?}",
        drawn.stride()
    );
    if inside[2] != 0xFF {
        report_what_reaches_the_buffer(gpu, allocator, WIDE, TALL);
    }
    // This allocator is the plain one, and whether its pitch agrees with the renderer's is the
    // machine's answer rather than this crate's: where a driver pads a row, the two ends address
    // different memory and no call along the way says so. `create_agreed` exists precisely because
    // that cannot be asserted away, and the check above it is the one that holds the product to
    // account. So where the pitch disagrees, this says what it found and stops asserting — a
    // machine where it agrees still gets the full check.
    let agrees = drawn.stride() == WIDE * 4;
    if !agrees {
        println!(
            "drawn_gl: this allocator pads a row of {WIDE} to {} bytes, so the renderer and the \
             buffer address different memory and the reads below say nothing about either; \
             `create_agreed` is what a display uses, and it is checked above",
            drawn.stride()
        );
        return;
    }
    assert_eq!(
        [inside[0], inside[2]],
        [0x00, 0xFF],
        "the rectangle was drawn red and the memory under it is {inside:?}, so what the renderer \
         wrote and what the buffer holds are laid out differently"
    );
    assert_eq!(
        [left[0], left[2]],
        [0xFF, 0x00],
        "the pixel beside the rectangle should still be blue and is {left:?}"
    );
    assert_eq!(
        [above[0], above[2]],
        [0xFF, 0x00],
        "the pixel above the rectangle should still be blue and is {above:?}"
    );
}

/// How fast a frame can be written into each kind of buffer, which is where the memory is.
///
/// **Not a clear.** This card fast-clears, and a repeated clear of one surface can be elided
/// outright — a measurement of one reported 32 GB/s on a part whose memory does 6.4. So this draws
/// a shaded full-surface quad, whose every pixel is written by the fragment stage, and the colour
/// is read back afterwards to prove the writes happened at all.
///
/// What the number says is which side of the link the buffer is on. Video memory on this card is
/// thousands of megabytes a second; across one PCIe lane it is under two hundred.
fn what_each_allocator_costs_to_fill(gpu: &Gpu, allocator: &gbm::Device, card: &std::path::Path) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 1024;
    const ROUNDS: u32 = 20;

    let megabytes = f64::from(WIDE) * f64::from(TALL) * 4.0 * f64::from(ROUNDS) / 1_048_576.0;
    let time_it = |what: &str, texture: &wgpu::Texture| {
        let pipeline = red_pipeline(gpu, texture.format());
        // One round outside the clock, so that whatever the driver does once — faulting the pages
        // in, warming the first submission — is not counted as bandwidth.
        paint_with(gpu, &pipeline, texture, 0, 0, WIDE, TALL);
        gl::finish(gpu, gl::signal(gpu, false, false), None);
        let started = Instant::now();
        for _ in 0..ROUNDS {
            paint_with(gpu, &pipeline, texture, 0, 0, WIDE, TALL);
        }
        gl::finish(gpu, gl::signal(gpu, false, false), None);
        let taken = started.elapsed().as_secs_f64();
        println!(
            "drawn_gl: filling {what} — {:.1} MB/s, {:.2} ms a frame",
            megabytes / taken,
            taken * 1000.0 / f64::from(ROUNDS)
        );
    };

    if let Ok(mut buffers) = gl::create(gpu, allocator, WIDE, TALL, 1) {
        let drawn = buffers.pop().expect("one was asked for");
        time_it("a gbm buffer", drawn.texture());
        let pixel = drawn.peek_at(0, 0).unwrap_or_default();
        println!("drawn_gl:   and its first pixel is {pixel:?}");
    }

    let Ok(device) = zgui_drm::Device::open(card) else {
        return;
    };
    let Ok(mut buffer) =
        device.create_dumb_buffer(WIDE, TALL, zgui_drm::format::Format(gl::FOURCC))
    else {
        return;
    };
    let Ok(descriptor) = device.export_buffer(&buffer) else {
        return;
    };
    let stride = buffer.stride();
    if let Ok(texture) = gl::import_descriptor(
        gpu,
        std::os::fd::AsFd::as_fd(&descriptor),
        WIDE,
        TALL,
        stride,
        0,
        gbm::IMPLICIT,
    ) {
        time_it("a dumb buffer", &texture);
        if let Ok(bytes) = buffer.bytes(&device) {
            println!(
                "drawn_gl:   and its first pixel is {:?}",
                &bytes[..4.min(bytes.len())]
            );
        }
    }
}

/// Whether a buffer whose pitch was padded is **linear with padding** or tiled.
///
/// The difference decides whether the padding can be worked around at all. If the buffer is linear
/// and merely wider than it was asked for, then allocating at `stride / 4` pixels makes the
/// renderer's own `width x 4` equal the real pitch and the two ends agree by construction. If it is
/// tiled, no width makes a linear renderer correct and the arrangement has to be abandoned rather
/// than adjusted.
///
/// So this allocates at the padded width, draws a band that only a partial write can misplace, and
/// reads the memory back. Correct here means linear.
fn whether_a_padded_buffer_is_merely_padded(gpu: &Gpu, allocator: &gbm::Device) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;

    let Ok(mut narrow) = gl::create(gpu, allocator, WIDE, TALL, 1) else {
        return;
    };
    let stride = narrow.pop().expect("one was asked for").stride();
    if stride == WIDE * 4 {
        println!("drawn_gl: nothing was padded, so there is nothing to widen");
        return;
    }
    let widened = stride / 4;
    let Ok(mut buffers) = gl::create(gpu, allocator, widened, TALL, 1) else {
        println!("drawn_gl: this driver would not allocate {widened} pixels wide");
        return;
    };
    let drawn = buffers.pop().expect("one was asked for");
    println!(
        "drawn_gl: asked for {widened} wide to match a stride of {stride}, and got stride {}",
        drawn.stride()
    );

    compose(gpu, drawn.texture(), BLUE);
    paint_red_rectangle(gpu, drawn.texture(), 0, 21, widened, 16);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let say = |x: u32, y: u32| -> &str {
        let pixel = drawn.peek_at(x, y).unwrap_or_default();
        if pixel[2] == 0xFF { "red" } else { "blue" }
    };
    println!(
        "drawn_gl: a band at y=21..37 in the widened buffer: (0,0) {}, (10,10) {}, (517,21) {}, \
         (100,30) {}, (0,63) {}",
        say(0, 0),
        say(10, 10),
        say(517, 21),
        say(100, 30),
        say(0, 63)
    );
}

/// A partial write into the buffers the backend actually chooses has to land where it was drawn.
///
/// The whole point of [`gl::create_agreed`]: whichever allocator it settles on, the renderer and
/// the display have to lay a row out the same way. A flat fill cannot check that — it is invariant
/// under every layout error there is — so this draws a band and reads the rows above, inside and
/// below it.
///
/// This is the case that fails on a machine where the two ends disagree, and it is the one that
/// would have caught the fault from the beginning.
fn what_the_chosen_allocator_gives(gpu: &Gpu, allocator: &gbm::Device, card: &std::path::Path) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;

    let Ok(device) = zgui_drm::Device::open(card) else {
        return;
    };
    let mut buffers = match gl::create_agreed(gpu, allocator, &device, WIDE, TALL, 1) {
        Ok(buffers) => buffers,
        Err(why) => {
            println!("drawn_gl: no allocator on this machine answers an agreed layout: {why}");
            return;
        }
    };
    let drawn = buffers.pop().expect("one was asked for");
    let stride = drawn.stride();
    println!("drawn_gl: the chosen allocator laid a row of {WIDE} out in {stride} bytes");

    compose(gpu, drawn.texture(), BLUE);
    paint_red_rectangle(gpu, drawn.texture(), 0, 21, WIDE, 16);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    // Read through the buffer's own memory, whichever allocator made it.
    let mut drawn = drawn;
    let mut say = |x: u32, y: u32| -> &'static str {
        match drawn.read_pixel(&device, x, y) {
            Ok(pixel) if pixel[2] == 0xFF => "red",
            Ok(_) => "blue",
            Err(_) => "unread",
        }
    };
    let seen = [
        say(0, 0),
        say(10, 10),
        say(517, 21),
        say(100, 30),
        say(0, 63),
    ];
    println!(
        "drawn_gl: a band at y=21..37 in the chosen buffer: (0,0) {}, (10,10) {}, (517,21) {}, \
         (100,30) {}, (0,63) {}",
        seen[0], seen[1], seen[2], seen[3], seen[4]
    );
    assert_eq!(
        seen,
        ["blue", "blue", "red", "red", "blue"],
        "a band drawn at y=21..37 has to be red inside and blue outside; the allocator that was \
         chosen lays a row out in {stride} bytes"
    );
    drawn.release(&device);
}

/// What happens when the importer is **told** the layout instead of left to assume one.
///
/// gbm answers `DRM_FORMAT_MOD_INVALID` for every buffer it makes on this display, so the import
/// passes no modifier at all and the importing driver falls back to whatever it assumes — which on
/// the evidence is `width x 4`, not the pitch it was handed. Nothing has ever tried simply saying
/// `DRM_FORMAT_MOD_LINEAR`.
///
/// This allocates through gbm exactly as the real path does, exports the same descriptor, and
/// imports it twice: once naming no layout, and once naming linear. Same buffer, same pitch, same
/// draw. If naming it is enough, the second is correct and the first is not, and the whole problem
/// is that nobody was told.
fn what_naming_the_layout_does(gpu: &Gpu, allocator: &gbm::Device) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;

    for (label, modifier) in [
        ("no layout named", gbm::IMPLICIT),
        ("linear named", gbm::LINEAR),
    ] {
        let mut allocation = match allocator.create(WIDE, TALL, gl::FOURCC, gbm::Layout::Driver) {
            Ok(allocation) => allocation,
            Err(why) => {
                println!("drawn_gl: no buffer to try {label} with: {why}");
                return;
            }
        };
        let stride = allocation.stride();
        let Ok(descriptor) = allocation.descriptor() else {
            return;
        };
        let texture = match gl::import_descriptor(gpu, descriptor, WIDE, TALL, stride, 0, modifier)
        {
            Ok(texture) => texture,
            Err(why) => {
                println!("drawn_gl: importing with {label} was refused: {why}");
                continue;
            }
        };

        compose(gpu, &texture, BLUE);
        paint_red_rectangle(gpu, &texture, 0, 21, WIDE, 16);
        gl::finish(gpu, gl::signal(gpu, false, false), None);

        drop(texture);
        let points = [(0_u32, 0_u32), (10, 10), (517, 21), (100, 30), (0, 63)];
        let Ok((mapped, pixels)) = allocation.read_pixels(WIDE, TALL, &points) else {
            println!("drawn_gl: {label}: the buffer would not map");
            continue;
        };
        let seen: Vec<&str> = pixels
            .iter()
            .map(|pixel| if pixel[2] == 0xFF { "red" } else { "blue" })
            .collect();
        println!(
            "drawn_gl: {label}: allocated stride {stride}, mapping stride {mapped}, band at \
             y=21..37 reads {seen:?} at {points:?}"
        );
    }
}

/// The same probes over a buffer the **kernel** allocated rather than gbm.
///
/// A dumb buffer is linear by construction and its pitch is `width x 4` rounded to 64 bytes, which
/// for 1280 pixels is 5120 — the number a renderer would compute for itself. gbm on this display
/// answers 8192 and names no layout, so the two allocators differ in exactly the thing the two
/// drivers have to agree about. If the probes come out right here and wrong there, the disagreement
/// is the padded pitch and nothing else.
///
/// It draws through the same import and the same pipeline, and reads the memory through the
/// buffer's own mapping rather than through the graphics interface, for the reason at the head of
/// this file.
fn what_a_dumb_buffer_does_instead(gpu: &Gpu, card: &std::path::Path) {
    const WIDE: u32 = 1280;
    const TALL: u32 = 64;

    let Ok(device) = zgui_drm::Device::open(card) else {
        eprintln!("drawn_gl: {} did not open as a DRM device", card.display());
        return;
    };
    if !device.supports_dumb_buffers() {
        println!("drawn_gl: this card has no dumb buffers, so there is nothing to compare");
        return;
    }
    let mut buffer =
        match device.create_dumb_buffer(WIDE, TALL, zgui_drm::format::Format(gl::FOURCC)) {
            Ok(buffer) => buffer,
            Err(error) => {
                println!("drawn_gl: this card would not make a dumb buffer: {error}");
                return;
            }
        };
    println!(
        "drawn_gl: a dumb buffer of {WIDE}x{TALL} has stride {} where gbm answered 8192",
        buffer.stride()
    );
    let descriptor = match device.export_buffer(&buffer) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            println!("drawn_gl: the dumb buffer would not export: {error}");
            return;
        }
    };
    let stride = buffer.stride();
    let texture = match gl::import_descriptor(
        gpu,
        std::os::fd::AsFd::as_fd(&descriptor),
        WIDE,
        TALL,
        stride,
        0,
        gbm::IMPLICIT,
    ) {
        Ok(texture) => texture,
        Err(why) => {
            println!("drawn_gl: a dumb buffer does not import into this driver: {why}");
            return;
        }
    };

    compose(gpu, &texture, BLUE);
    paint_red_rectangle(gpu, &texture, 0, 21, WIDE, 16);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let Ok(bytes) = buffer.bytes(&device) else {
        println!("drawn_gl: the dumb buffer would not map");
        return;
    };
    let read = |x: u32, y: u32| -> &str {
        let at = (y * stride + x * 4) as usize;
        match (bytes.get(at), bytes.get(at + 2)) {
            (Some(0xFF), Some(0x00)) => "blue",
            (Some(0x00), Some(0xFF)) => "red",
            _ => "neither",
        }
    };
    println!(
        "drawn_gl: a band drawn at y=21..37 over a dumb buffer: (0,0) {}, (10,10) {}, (517,21) {}, \
         (100,30) {}, (0,63) {}",
        read(0, 0),
        read(10, 10),
        read(517, 21),
        read(100, 30),
        read(0, 63)
    );
}

/// Asks, in order, which of three ways of writing a buffer actually reaches its memory.
///
/// A clear and a draw are different paths through the driver, and a scissored draw is a third. The
/// rectangle above says only that something did not arrive; this says which of the three did.
fn report_what_reaches_the_buffer(gpu: &Gpu, allocator: &gbm::Device, width: u32, height: u32) {
    let Ok(mut buffers) = gl::create(gpu, allocator, width, height, 3) else {
        return;
    };
    let scissored = buffers.pop().expect("three were asked for");
    let whole = buffers.pop().expect("three were asked for");
    let cleared = buffers.pop().expect("three were asked for");

    let fresh = cleared.peek_at(0, 0).unwrap_or_default();
    compose(gpu, cleared.texture(), BLUE);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    // Red over the whole surface, through the same pipeline the scissored draw uses.
    compose(gpu, whole.texture(), BLUE);
    paint_red_rectangle(gpu, whole.texture(), 0, 0, width, height);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    compose(gpu, scissored.texture(), BLUE);
    paint_red_rectangle(gpu, scissored.texture(), 0, 21, width, 1);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    let say = |what: &str, pixel: [u8; 4]| {
        let colour = match (pixel[0], pixel[2]) {
            (0xFF, 0x00) => "blue",
            (0x00, 0xFF) => "red",
            _ => "neither",
        };
        println!("drawn_gl:   {what}: {pixel:?} ({colour})");
    };
    println!("drawn_gl: what reaches a {width}x{height} buffer:");
    say("a fresh buffer at (0,0)", fresh);
    say(
        "cleared blue, at (0,0)",
        cleared.peek_at(0, 0).unwrap_or_default(),
    );
    say(
        "cleared blue, at the last row",
        cleared.peek_at(0, height - 1).unwrap_or_default(),
    );
    say(
        "drawn red over the whole surface, at (0,0)",
        whole.peek_at(0, 0).unwrap_or_default(),
    );
    say(
        "drawn red over the whole surface, at (517,21)",
        whole.peek_at(517, 21).unwrap_or_default(),
    );
    say(
        "drawn red in one scissored row, at (517,21)",
        scissored.peek_at(517, 21).unwrap_or_default(),
    );

    // Which part of a scissor is refused. Several probes per rectangle rather than one: a single
    // point cannot tell "the rectangle landed somewhere else" from "nothing was drawn at all", and
    // both look the same at whichever point is asked.
    let probes = [
        (0_u32, 0_u32),
        (10, 10),
        (517, 21),
        (100, 40),
        (0, height - 1),
    ];
    println!("drawn_gl: which scissors reach the buffer, probed at {probes:?}:");
    for (label, rect) in [
        ("the whole target", (0, 0, width, height)),
        ("the top row", (0, 0, width, 1)),
        ("the left half", (0, 0, width / 2, height)),
        ("the top-left corner", (0, 0, 64, 16)),
        ("a row part way down", (0, 21, width, 1)),
        ("a band part way down", (0, 21, width, 16)),
    ] {
        let Ok(mut one) = gl::create(gpu, allocator, width, height, 1) else {
            return;
        };
        let buffer = one.pop().expect("one was asked for");
        compose(gpu, buffer.texture(), BLUE);
        paint_red_rectangle(gpu, buffer.texture(), rect.0, rect.1, rect.2, rect.3);
        gl::finish(gpu, gl::signal(gpu, false, false), None);
        let seen: Vec<&str> = probes
            .iter()
            .map(|(x, y)| {
                let pixel = buffer.peek_at(*x, *y).unwrap_or_default();
                if pixel[2] == 0xFF { "red" } else { "blue" }
            })
            .collect();
        println!("drawn_gl:   {label} {rect:?}: {seen:?}");
    }
}

/// Draws a red rectangle into `texture`, leaving everything outside it as it was.
///
/// A scissored draw rather than a clear: a render pass clears its whole attachment, and what is
/// being asked here is where a *part* of a frame lands.
fn paint_red_rectangle(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let pipeline = red_pipeline(gpu, texture.format());
    paint_with(gpu, &pipeline, texture, x, y, width, height);
}

/// The pipeline [`paint_red_rectangle`] draws with, built separately so a timing loop can hoist it.
///
/// Compiling a shader and building a pipeline are tens of milliseconds on this driver, which is the
/// same order as the frame being measured. A loop that built one per round would report that cost
/// as though it were memory bandwidth.
fn red_pipeline(gpu: &Gpu, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = gpu
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("drawn_gl red"),
            source: wgpu::ShaderSource::Wgsl(
                r"
                @vertex
                fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                    // Two triangles making a quad over the whole target. A single big triangle
                    // puts its hypotenuse through the corner of the viewport, where coverage is a
                    // rasterisation rule rather than a certainty — which is exactly the ambiguity
                    // a check on where pixels land must not have.
                    var corners = array<vec2<f32>, 6>(
                        vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0, -1.0), vec2<f32>(-1.0,  1.0),
                        vec2<f32>(-1.0,  1.0), vec2<f32>( 1.0, -1.0), vec2<f32>( 1.0,  1.0),
                    );
                    return vec4<f32>(corners[index], 0.0, 1.0);
                }

                @fragment
                fn fragment() -> @location(0) vec4<f32> {
                    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
                }
                "
                .into(),
            ),
        });
    gpu.device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("drawn_gl red"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
}

/// Draws the red rectangle with a pipeline the caller already has.
fn paint_with(
    gpu: &Gpu,
    pipeline: &wgpu::RenderPipeline,
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("drawn_gl red"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_scissor_rect(x, y, width, height);
        pass.draw(0..6, 0..1);
    }
    gpu.queue().submit([encoder.finish()]);
    // Waited for here as well as by the caller's `gl::finish`: this is a submission of its own, and
    // a check that read the buffer before it landed would report a scissor fault that is really a
    // race in the check.
    let _ = gpu.device().poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
}

/// Two buffers of one set have to be two allocations, or a flip shows the frame being drawn.
fn every_buffer_of_a_set_is_its_own_memory(gpu: &Gpu, allocator: &gbm::Device) {
    let Ok(buffers) = gl::create(gpu, allocator, WIDTH, HEIGHT, 2) else {
        return;
    };
    // Both are drawn into below and only one is drawn blue, which is what says they are two
    // allocations rather than two names for one.
    let [first, second] = &buffers[..] else {
        panic!("two were asked for and {} came back", buffers.len());
    };

    compose(gpu, first.texture(), BLUE);
    gl::finish(gpu, gl::signal(gpu, false, false), None);

    assert_eq!(
        first.peek().expect("a gbm buffer maps")[0],
        0xFF,
        "the first buffer holds the frame"
    );
    assert_ne!(
        second.peek().expect("a gbm buffer maps")[0],
        0xFF,
        "and the second one does not, so they are separate memory"
    );
}

/// Whichever tier a device lands on, it has to be one it can actually perform.
fn the_tier_that_signals_a_frame_is_one_this_device_has(gpu: &Gpu) {
    // `Signal::Kernel` needs the display to take an in-fence, because the flip is what carries
    // the descriptor and there is nowhere else to put it. `Signal::Written` does not: the
    // descriptor comes from the buffer rather than from the driver, and a commit that cannot take
    // one is a descriptor the frame loop parks on in its own poll set instead. Either way nothing
    // in this process waits for the card, which is the whole point of the tiers.
    for buffers in [false, true] {
        let without = gl::signal(gpu, false, buffers);
        assert!(
            without != gl::Signal::Kernel,
            "a display that takes no in-fence has nowhere for the flip to carry a descriptor, so \
             the kernel cannot be the one that waits, and it was told it could: {without:?}"
        );
    }
    let without = gl::signal(gpu, false, false);
    let with = gl::signal(gpu, true, false);
    assert!(
        with == without || with == gl::Signal::Kernel,
        "offering an in-fence may raise the tier and may change nothing, and it did neither: \
         {without:?} became {with:?}"
    );
    // A device whose driver exports a sync file itself keeps that tier; one whose driver does not
    // takes the buffer's, which is the whole point of the tier.
    let buffered = gl::signal(gpu, true, true);
    assert!(
        matches!(buffered, gl::Signal::Kernel | gl::Signal::Written),
        "a display that takes an in-fence and buffers that say what writes them leave the waiting \
         to the kernel, and this device answered {buffered:?}"
    );
    eprintln!(
        "drawn_gl: this device signals a frame by {with:?} where the display takes a fence, and \
         by {buffered:?} where its buffers also answer"
    );
}

/// A buffer these tests allocated says what is still writing it, or says the kernel cannot.
fn a_scanout_buffer_answers_what_is_writing_it(buffer: &gl::Drawn) {
    let Some(descriptor) = buffer.exported() else {
        eprintln!("drawn_gl: this buffer was never exported, so nothing was asserted");
        return;
    };
    match zgui_drm::sync::writers_of(descriptor) {
        // Nothing has been drawn into it here, so what comes back is a descriptor that is already
        // signalled. That it comes back at all is the fact the tier is chosen on.
        Ok(Some(fence)) => eprintln!(
            "drawn_gl: this kernel says what writes a scanout buffer, so a frame is waited for by \
             the kernel (descriptor {fence:?})"
        ),
        Ok(None) => eprintln!(
            "drawn_gl: this kernel does not serve the request, so a frame is waited for by the \
             program"
        ),
        Err(refusal) => panic!(
            "a dma-buf either answers a descriptor or reports that this kernel has no such \
             request, and this one refused: {refusal}"
        ),
    }
}

/// What a single call into this graphics driver costs, which is what a frame's submit is made of.
///
/// On this backend a submit is not a wait — it is the recorded command stream replayed as real
/// calls, one after another, on the frame loop's own thread. So what a frame spends there is the
/// call count times this, and knowing one without the other says nothing about which to attack.
///
/// Three kinds, because they are not the same cost. A **query** answers out of the driver's own
/// state and touches nothing. A **setter** marks state dirty and is what most of a command stream
/// is. A **draw** is where a driver validates everything that was marked, so it carries the cost of
/// every setter before it — which is why counting calls alone is misleading.
fn what_one_call_into_this_driver_costs(gpu: &Gpu) {
    use std::time::Instant;
    // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed. The guard is read
    // through and dropped, which its own documentation permits at any time.
    let Some(adapter) = (unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }) else {
        eprintln!("drawn_gl: this adapter is not the OpenGL one, so no call was timed");
        return;
    };
    let gl = adapter.adapter_context().lock();
    // SAFETY: the context is current for as long as `gl` lives, and every call below is a state
    // query or a state setter with arguments the enum types make valid.
    unsafe {
        use glow::HasContext as _;
        const ROUNDS: u32 = 20_000;
        // Warm whatever the loader resolves lazily, so the first call is not counted as the cost of
        // every call.
        let _ = gl.is_enabled(glow::BLEND);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        let at = Instant::now();
        for _ in 0..ROUNDS {
            let _ = gl.is_enabled(glow::BLEND);
        }
        let query = at.elapsed().as_secs_f64() / f64::from(ROUNDS);

        let at = Instant::now();
        for _ in 0..ROUNDS {
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
        let setter = at.elapsed().as_secs_f64() / f64::from(ROUNDS);

        let at = Instant::now();
        for _ in 0..ROUNDS {
            gl.scissor(0, 0, 16, 16);
        }
        let scissor = at.elapsed().as_secs_f64() / f64::from(ROUNDS);

        eprintln!(
            "drawn_gl: one call into this driver — query {:.3} us, bind {:.3} us, scissor {:.3} us",
            query * 1e6,
            setter * 1e6,
            scissor * 1e6,
        );
    }
    what_one_system_call_costs();
}

/// What a bare system call costs here, which is what a per-draw kernel submission is measured
/// against.
///
/// A draw on this machine reaches the kernel — adding draws adds system time and almost no user
/// time — so the question is whether the cost is the crossing itself or the work the kernel does
/// once it is there. A call that fails immediately answers the first half: whatever a draw costs
/// beyond this is the kernel validating buffers and queueing a command stream, not the boundary.
fn what_one_system_call_costs() {
    use std::time::Instant;
    const ROUNDS: u32 = 20_000;
    // A read of nothing from a descriptor that refuses it: a full crossing into the kernel and
    // back, with nothing done in between.
    let Ok(node) = std::fs::File::open("/dev/null") else {
        eprintln!("drawn_gl: no /dev/null, so no system call was timed");
        return;
    };
    let mut nothing = [0_u8; 0];
    let at = Instant::now();
    for _ in 0..ROUNDS {
        let _ = rustix::io::read(std::os::fd::AsFd::as_fd(&node), &mut nothing);
    }
    let each = at.elapsed().as_secs_f64() / f64::from(ROUNDS);
    eprintln!("drawn_gl: one system call — {:.3} us", each * 1e6);
}

/// Composes a frame of one colour into `texture`, through the device the renderer draws on.
fn compose(gpu: &Gpu, texture: &wgpu::Texture, colour: wgpu::Color) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("drawn_gl"),
        });
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("drawn_gl"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(colour),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    gpu.queue().submit([encoder.finish()]);
    // Waited for here as well as by the caller's `gl::finish`: this is a submission of its own, and
    // a check that read the buffer before it landed would report a scissor fault that is really a
    // race in the check.
    let _ = gpu.device().poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
}

/// A graphics device, or a word on standard error about why there is none.
fn graphics() -> Option<std::sync::Arc<Gpu>> {
    match SharedGraphics::new().open_gpu() {
        Ok(gpu) => Some(gpu),
        Err(error) => {
            eprintln!("no graphics device on this machine: {error}");
            None
        }
    }
}

/// The first card, or a word about why there is none.
fn card() -> Option<PathBuf> {
    if let Ok(named) = std::env::var("ZGUI_DRM_DEVICE") {
        return Some(PathBuf::from(named));
    }
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("card"))
        })
        .collect();
    cards.sort();
    if cards.is_empty() {
        eprintln!("no card in /dev/dri, so there is no display to allocate for");
    }
    cards.pop()
}
