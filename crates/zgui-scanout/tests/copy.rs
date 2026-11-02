//! A copy between two real scanout buffers, on a real card.
//!
//! Two things are asked, and the second is why this is a test rather than a benchmark. **Did the
//! pixels arrive?** — read back out of the buffer itself, because a copy a device cannot really do
//! reports success, runs at an impossible speed and leaves the memory as it was. **What did it
//! cost?** — against the link it exists to avoid crossing.
//!
//! The test skips itself, loudly, on a machine with no card, no libgbm or no EGL.
//! `ZGUI_SCANOUT_NODE` names the node to use; without it every `card*` under `/dev/dri` is tried in
//! turn.

// The subject is a Linux graphics interface, so there is nothing to ask elsewhere.
#![cfg(target_os = "linux")]
#![allow(
    unsafe_code,
    reason = "the test maps a buffer through its descriptor to fill it and to count what moved"
)]

use std::os::fd::{AsFd, BorrowedFd};
use std::time::Instant;

use zgui_scanout::{Buffer, Copier, Rect, egl::Egl};

/// `XR24`: eight bits a channel, blue first, the fourth byte ignored.
const FOURCC: u32 = u32::from_le_bytes(*b"XR24");

/// A screen's worth, as the machine this was written for drives.
const WIDTH: u32 = 1280;
/// The same.
const HEIGHT: u32 = 1024;

/// One block's damage, near enough: `tty_motion` damages about this much in about this shape.
const RECT: (i32, i32) = (130, 98);
/// How many of them a frame owes, measured on that scene.
const RECTS: usize = 24;

/// What the source is filled with, and what a copied byte therefore reads as.
const MOVED: u8 = 0x5a;
/// What the target is filled with, and what an untouched byte still reads as.
const KEPT: u8 = 0x21;

/// The same `area` of pixels cut several ways, plus the whole screen for the ceiling.
///
/// A rectangle's cost has two parts and they cannot be told apart from one shape: the pixels it
/// moves, and whatever the device pays to begin one at all. Holding the area still and changing the
/// count is what separates them.
fn shapes(area: usize) -> Vec<(&'static str, Vec<Rect>)> {
    let one = |w: i32, h: i32| {
        vec![Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }]
    };
    let tiled = |side: i32| {
        let across = WIDTH as i32 / side;
        let wanted = area as i32 / (side * side);
        (0..wanted)
            .map(|which| Rect {
                x: (which % across) * side,
                y: (which / across) * side,
                width: side,
                height: side,
            })
            .filter(|rect| rect.y + rect.height <= HEIGHT as i32)
            .collect::<Vec<_>>()
    };
    let wide = area as i32 / WIDTH as i32;
    let square = (area as f64).sqrt() as i32;
    vec![
        ("one wide band", one(WIDTH as i32, wide)),
        ("one square", one(square, square)),
        ("a frame's damage", damage()),
        ("64-pixel tiles", tiled(64)),
        ("32-pixel tiles", tiled(32)),
        ("the whole screen", one(WIDTH as i32, HEIGHT as i32)),
    ]
}

/// Writes `value` into every byte a buffer holds, through its descriptor.
///
/// Through the dma-buf rather than through `gbm_bo_map`, which faults inside the driver on the
/// hardware this was written for. A uniform fill needs no layout: every byte is the same one
/// whether the pixels are laid out in rows or in tiles.
fn fill(descriptor: BorrowedFd<'_>, span: usize, value: u8) -> bool {
    // SAFETY: the length is the buffer's own, and the descriptor is a dma-buf the allocation keeps
    // alive across this call.
    let address = unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut(),
            span,
            rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
            rustix::mm::MapFlags::SHARED,
            descriptor,
            0,
        )
    };
    let Ok(address) = address else {
        return false;
    };
    // SAFETY: the mapping is `span` bytes of writable memory, and it is released here.
    unsafe {
        std::slice::from_raw_parts_mut(address.cast::<u8>(), span).fill(value);
        let _ = rustix::mm::munmap(address, span);
    }
    true
}

/// Counts the bytes of a buffer that hold `value`, through its descriptor.
///
/// **A count needs no layout.** Where each pixel physically sits depends on whether the driver laid
/// the buffer out in rows or in tiles, and this test knows neither; a count over the whole buffer is
/// the same number either way, because a layout permutes bytes and keeps them. So an exact count is
/// an exact statement about how much was copied, on hardware whose arrangement is never asked.
fn count(descriptor: BorrowedFd<'_>, span: usize, value: u8) -> Option<usize> {
    // SAFETY: as `fill`.
    let address = unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut(),
            span,
            rustix::mm::ProtFlags::READ,
            rustix::mm::MapFlags::SHARED,
            descriptor,
            0,
        )
    };
    let address = address.ok()?;
    // SAFETY: the mapping is `span` bytes of readable memory, released here.
    let held = unsafe {
        let read = std::slice::from_raw_parts(address.cast::<u8>(), span);
        let held = read.iter().filter(|byte| **byte == value).count();
        let _ = rustix::mm::munmap(address, span);
        held
    };
    Some(held)
}

/// The rectangles a frame owes, spread over the screen as moving blocks are.
///
/// They do not overlap, which is what makes the count below exact arithmetic rather than a bound.
fn damage() -> Vec<Rect> {
    (0..RECTS)
        .map(|which| {
            let columns = 6;
            let (column, row) = (which % columns, which / columns);
            Rect {
                x: 24 + i32::try_from(column).expect("six columns") * 200,
                y: 24 + i32::try_from(row).expect("four rows") * 220,
                width: RECT.0,
                height: RECT.1,
            }
        })
        .collect()
}

/// Opens the first node that answers, and says which it was.
fn node() -> Option<(std::fs::File, String)> {
    let named = std::env::var("ZGUI_SCANOUT_NODE").ok();
    let candidates: Vec<String> = match named {
        Some(one) => vec![one],
        None => (0..4)
            .map(|which| format!("/dev/dri/card{which}"))
            .collect(),
    };
    candidates
        .into_iter()
        .find_map(|path| std::fs::File::open(&path).ok().map(|file| (file, path)))
}

/// Seconds of processor this process has spent, for telling device work from driver work.
fn processor_time() -> f64 {
    let held = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    held.tv_sec as f64 + held.tv_nsec as f64 / 1e9
}

#[test]
fn a_copy_between_two_scanout_buffers_moves_the_pixels_and_says_what_it_cost() {
    let Some((card, path)) = node() else {
        eprintln!("no card under /dev/dri opened, so nothing about a copy was checked");
        return;
    };
    let Ok(library) = zgui_gbm::Library::load() else {
        eprintln!("this machine has no libgbm, so nothing about a copy was checked");
        return;
    };
    let Ok(allocator) = zgui_gbm::Device::new(&library, card.as_fd()) else {
        eprintln!("{path}: no allocator over this node; nothing was checked");
        return;
    };
    let mut allocations = Vec::new();
    for _ in 0..2 {
        match allocator.create(WIDTH, HEIGHT, FOURCC, zgui_gbm::Layout::Driver) {
            Ok(allocation) => allocations.push(allocation),
            Err(reason) => {
                eprintln!("{path}: no scanout buffer of that size ({reason}); nothing checked");
                return;
            }
        }
    }

    let described: Vec<(u32, u32)> = allocations
        .iter()
        .map(|allocation| (allocation.stride(), allocation.offset()))
        .collect();
    let mut exported = Vec::new();
    for allocation in &mut allocations {
        match allocation.descriptor() {
            Ok(descriptor) => exported.push(descriptor),
            Err(reason) => {
                eprintln!("{path}: a buffer would not export ({reason}); nothing checked");
                return;
            }
        }
    }

    // Two values that cannot be confused, so a byte says which buffer it came from.
    for (which, (descriptor, (stride, _))) in exported.iter().zip(&described).enumerate() {
        let span = *stride as usize * HEIGHT as usize;
        let value = if which == 0 { MOVED } else { KEPT };
        assert!(
            fill(*descriptor, span, value),
            "a buffer could not be filled"
        );
    }

    let buffers: Vec<Buffer<'_>> = exported
        .iter()
        .zip(&described)
        .map(|(descriptor, (stride, offset))| Buffer {
            descriptor: *descriptor,
            width: WIDTH,
            height: HEIGHT,
            fourcc: FOURCC,
            stride: *stride,
            offset: *offset,
            // Implicit throughout, which is what the allocation above asked for.
            modifier: None,
        })
        .collect();

    let mut copier = match Egl::open(card.as_fd(), &buffers) {
        Ok(copier) => copier,
        Err(reason) => {
            eprintln!("{path}: no copier here ({reason}); nothing about a copy was checked");
            return;
        }
    };
    assert_eq!(copier.len(), 2, "both buffers were taken");

    // **A second copier, holding the same buffers the other way round.** EGL binds a context to a
    // thread, and opening this one takes the thread from the copier above — which is exactly what a
    // renderer on the same thread does, because it has a context of its own and makes it current to
    // draw. A copier that does not take the thread back issues its copy against whatever context
    // holds it, and every call is accepted.
    //
    // The reversal is what makes that visible. Two contexts share no objects, but each names its
    // textures 1 and 2 in the order it imported them — so a copy misdirected into a context holding
    // the *same* buffers in the same order does the right thing by luck, and the test passes with
    // the bug in place. Reversed, the same names mean the opposite buffers, and a misdirected copy
    // writes the wrong one.
    let mut reversed = buffers.clone();
    reversed.reverse();
    let displacing = Egl::open(card.as_fd(), &reversed);
    assert!(
        displacing.is_ok(),
        "a second copier over the same buffers was refused"
    );

    let rects = damage();
    copier.copy(0, 1, &rects).expect("the copy was refused");
    println!("{path}: {} rectangles, {copier:?}", rects.len());

    // Did they arrive? Exactly the rectangles asked for, and nothing beyond them.
    let area: usize = rects
        .iter()
        .map(|rect| {
            usize::try_from(rect.width).expect("a positive width")
                * usize::try_from(rect.height).expect("a positive height")
        })
        .sum();
    let span = described[1].0 as usize * HEIGHT as usize;
    let moved = count(exported[1], span, MOVED).expect("the target would not map");
    assert_eq!(
        moved,
        area * 4,
        "the copy moved {moved} bytes where {} were asked for",
        area * 4,
    );

    // **What limits it**, which decides how much of a frame a display device can be asked to do.
    // The same area in different shapes: a rate that holds is a memory bus, and one that falls with
    // the rectangle count is a cost paid per rectangle rather than per pixel. The whole screen is
    // there for the ceiling, because that is what a repair costs in the worst case.
    println!("  --- the same {area} pixels, in different shapes ---");
    for (what, shape) in shapes(area) {
        // Twice, and the second is the one reported: the first pays for whatever the driver
        // validates once per shape.
        for round in 0..2 {
            let wall = Instant::now();
            copier.copy(0, 1, &shape).expect("the copy was refused");
            let took = wall.elapsed();
            if round == 1 {
                let bytes: usize = shape
                    .iter()
                    .map(|rect| rect.width as usize * rect.height as usize * 4)
                    .sum();
                println!(
                    "  {what:<22} {:>3} rects  {:>6.2} ms  {:>7.1} MB/s",
                    shape.len(),
                    took.as_secs_f64() * 1e3,
                    bytes as f64 / took.as_secs_f64() / 1e6,
                );
            }
        }
    }

    // What did it cost, and **who spent it**? Wall time beside processor time: a copy the device
    // makes leaves the processor idle and the two diverge, and one the driver makes on the
    // processor has them equal. On a machine with one core that is the difference between work
    // that can overlap a frame and work that cannot.
    for round in 0..4 {
        let (wall, cpu) = (Instant::now(), processor_time());
        copier.copy(0, 1, &rects).expect("the copy was refused");
        let (took, spent) = (wall.elapsed(), processor_time() - cpu);
        let bytes = area * 4;
        println!(
            "  round {round}: {:>6.2} ms wall, {:>6.2} ms processor, {:>5.2} MiB = {:>7.1} MB/s",
            took.as_secs_f64() * 1e3,
            spent * 1e3,
            bytes as f64 / (1024.0 * 1024.0),
            bytes as f64 / took.as_secs_f64() / 1e6,
        );
    }
}
