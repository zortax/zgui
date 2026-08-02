//! Whether anything was actually drawn inside a rectangle.
//!
//! The measurement a component gallery kept getting wrong: a drawing that reaches the document
//! produces a box of exactly the right size whether or not a single pixel of it is painted, so
//! every assertion about the box passes while the window shows empty space. What follows counts
//! *pixels that differ from the surface around them*, which is the only thing that separates the
//! two.

use zgui::geom::{Device, DevicePx, Rect};
use zgui_render_wgpu::Pixels;

/// How far apart two colours have to be to count as different, per channel.
///
/// Not zero: an antialiased edge against a background is a real difference, but so is the last bit
/// of a colour that went through a non-sRGB target and back. Eight is far below the contrast of any
/// ink a person can see and far above that.
const TOLERANCE: i32 = 8;

/// What fraction of the pixels inside `rect` differ from the colour that rectangle is mostly made
/// of, from zero to one.
///
/// The rectangle's own most common colour is the background rather than a colour passed in, because
/// a drawing sits on whatever is behind it and a fixture that named the background would be
/// asserting against a stylesheet instead of against the picture.
///
/// Zero for a rectangle outside the surface, and for one that is entirely flat — which is what a
/// drawing that was planned, counted, composited from an unwritten scratch and never rasterised
/// looks like.
pub fn fraction(pixels: &Pixels, rect: Rect<DevicePx, Device>) -> f32 {
    let Some(bounds) = clamped(pixels, rect) else {
        return 0.0;
    };
    let mut counts: rustc_hash::FxHashMap<[u8; 4], u32> = rustc_hash::FxHashMap::default();
    for (x, y) in every(bounds) {
        *counts.entry(pixels.rgba(x, y)).or_default() += 1;
    }
    let Some((&background, _)) = counts.iter().max_by_key(|(_, count)| **count) else {
        return 0.0;
    };
    let mut different = 0_u32;
    let mut total = 0_u32;
    for (x, y) in every(bounds) {
        total += 1;
        if unlike(pixels.rgba(x, y), background) {
            different += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    different as f32 / total as f32
}

/// Whether two colours are far enough apart to be told apart.
fn unlike(left: [u8; 4], right: [u8; 4]) -> bool {
    left.iter()
        .zip(right.iter())
        .any(|(a, b)| (i32::from(*a) - i32::from(*b)).abs() > TOLERANCE)
}

/// `rect` in whole pixels, cut down to what was read back, or nothing if none of it was.
fn clamped(pixels: &Pixels, rect: Rect<DevicePx, Device>) -> Option<(i32, i32, i32, i32)> {
    let size = pixels.size();
    let left = rect.origin.x.0.floor() as i32;
    let top = rect.origin.y.0.floor() as i32;
    let right = (rect.origin.x.0 + rect.size.width.0).ceil() as i32;
    let bottom = (rect.origin.y.0 + rect.size.height.0).ceil() as i32;
    let bounds = (
        left.max(0),
        top.max(0),
        right.min(size.width),
        bottom.min(size.height),
    );
    (bounds.2 > bounds.0 && bounds.3 > bounds.1).then_some(bounds)
}

/// Every pixel coordinate inside a clamped rectangle.
fn every(bounds: (i32, i32, i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    (bounds.1..bounds.3).flat_map(move |y| (bounds.0..bounds.2).map(move |x| (x, y)))
}
