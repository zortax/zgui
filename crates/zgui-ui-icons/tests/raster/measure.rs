//! Reading a rendering: where the ink is, what colour it is, and how its edges are shaded.

use zgui::geom::{Device, Rect};
use zgui_render_wgpu::Pixels;

/// Whether the pixel at `(x, y)` is exactly `color`.
pub fn is(pixels: &Pixels, x: i32, y: i32, color: [u8; 4]) -> bool {
    pixels.rgba(x, y) == color
}

/// The tightest rectangle containing every pixel that is not `background`.
///
/// `None` when nothing differs from the background, which is the reading a rasteriser that wrote
/// nothing produces and the one every assertion here has to be able to tell apart from a pass.
pub fn ink_bounds(pixels: &Pixels, background: [u8; 4]) -> Option<Rect<i32, Device>> {
    let flat = background;
    let (mut left, mut top) = (i32::MAX, i32::MAX);
    let (mut right, mut bottom) = (i32::MIN, i32::MIN);
    for y in 0..pixels.size().height {
        for x in 0..pixels.size().width {
            if pixels.rgba(x, y) == flat {
                continue;
            }
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if left > right {
        return None;
    }
    Some(Rect::new(
        zgui::geom::Point::new(left, top),
        zgui::geom::Size::new(right - left, bottom - top),
    ))
}

/// How far `(x, y)`'s centre is from `centre`, in device pixels.
pub fn radius(centre: (f64, f64), x: i32, y: i32) -> f64 {
    let dx = f64::from(x) + 0.5 - centre.0;
    let dy = f64::from(y) + 0.5 - centre.1;
    dx.hypot(dy)
}

/// What a pixel is, relative to two colours it should be between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Exactly the first colour.
    Near,
    /// Exactly the second.
    Far,
    /// Somewhere between the two on every channel, which is what a coverage value looks like.
    Between,
    /// Neither, and not between them either.
    Other,
}

/// Classifies the pixel at `(x, y)` against `ink` over `background`.
pub fn level(pixels: &Pixels, x: i32, y: i32, ink: [u8; 4], background: [u8; 4]) -> Level {
    let (found, near, far) = (pixels.rgba(x, y), ink, background);
    if found == near {
        return Level::Near;
    }
    if found == far {
        return Level::Far;
    }
    let between = (0..3).all(|channel| {
        let (low, high) = (
            near[channel].min(far[channel]),
            near[channel].max(far[channel]),
        );
        found[channel] > low && found[channel] < high
    });
    if between {
        Level::Between
    } else {
        Level::Other
    }
}

/// Every pixel of `pixels` that is a partial coverage of `ink` over `background`.
///
/// Returned as coordinates rather than a count so that a caller can ask *where* they are, which is
/// what separates an antialiased edge from a blurred one: an edge's partial pixels lie in a band
/// one or two pixels wide along the outline, and nowhere else.
pub fn partial(pixels: &Pixels, ink: [u8; 4], background: [u8; 4]) -> Vec<(i32, i32)> {
    let mut found = Vec::new();
    for y in 0..pixels.size().height {
        for x in 0..pixels.size().width {
            if level(pixels, x, y, ink, background) == Level::Between {
                found.push((x, y));
            }
        }
    }
    found
}

/// How many distinct values the given channel takes over `at`.
pub fn distinct(pixels: &Pixels, at: &[(i32, i32)], channel: usize) -> usize {
    let mut seen: Vec<u8> = at
        .iter()
        .map(|&(x, y)| pixels.rgba(x, y)[channel])
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// The first pixel inside `region` that is not `color`, with what it was instead.
pub fn first_unlike(
    pixels: &Pixels,
    region: impl Fn(i32, i32) -> bool,
    color: [u8; 4],
) -> Option<(i32, i32, [u8; 4])> {
    let wanted = color;
    for y in 0..pixels.size().height {
        for x in 0..pixels.size().width {
            if region(x, y) && pixels.rgba(x, y) != wanted {
                return Some((x, y, pixels.rgba(x, y)));
            }
        }
    }
    None
}

/// How many pixels inside `region` there are, so that an emptiness check cannot pass over nothing.
pub fn count(pixels: &Pixels, region: impl Fn(i32, i32) -> bool) -> u32 {
    let mut total = 0;
    for y in 0..pixels.size().height {
        for x in 0..pixels.size().width {
            if region(x, y) {
                total += 1;
            }
        }
    }
    total
}

/// The first pixel at which two readbacks differ, with both values.
pub fn first_difference(
    left: &Pixels,
    right: &Pixels,
    region: impl Fn(i32, i32) -> bool,
) -> Option<(i32, i32, [u8; 4], [u8; 4])> {
    assert_eq!(
        left.size(),
        right.size(),
        "two readbacks of different extents were compared, which would compare a prefix"
    );
    for y in 0..left.size().height {
        for x in 0..left.size().width {
            if region(x, y) && left.rgba(x, y) != right.rgba(x, y) {
                return Some((x, y, left.rgba(x, y), right.rgba(x, y)));
            }
        }
    }
    None
}
