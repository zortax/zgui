//! Repeated outlines in a path: the subpaths of a scatter of triangles, crosses or stars.
//!
//! Pure geometry with no scene access. Each subpath is written relative to its own move point,
//! mapped by the device's linear map and rounded to 1/256 of a device pixel. Two subpaths with the
//! same words are one outline, so float noise below a 256th of a pixel keeps one outline, and a
//! translated copy of a subpath is the same outline at another anchor.
//!
//! The words serve both as the outline's identity and as what is rasterised, so no two subpaths
//! that share an identity draw different pixels.

use core::hash::Hasher;
use core::ops::Range;

use rustc_hash::FxHasher;
use zgui_scene::kurbo::{BezPath, PathEl, Point};

/// The fewest subpaths a path needs before its outlines are looked for.
pub(crate) const MIN_SUBPATHS: usize = 8;

/// The most distinct outlines one path may have.
pub(crate) const MAX_GEOMETRIES: usize = 64;

/// How many subpaths are read before the first test of the distinct outlines.
const FIRST_READ: usize = 64;

/// The most distinct outlines among the first [`FIRST_READ`] subpaths.
const FIRST_DISTINCT: usize = 16;

/// Words per device pixel.
pub(crate) const UNITS: f64 = 256.0;

/// The farthest a point may lie from its anchor, in words: 64 device pixels.
const REACH: f64 = 64.0 * UNITS;

/// The word that starts a move, followed by two coordinates.
pub(crate) const MOVE: i32 = 0;
/// The word that starts a line, followed by two coordinates.
pub(crate) const LINE: i32 = 1;
/// The word that starts a quadratic, followed by four coordinates.
pub(crate) const QUAD: i32 = 2;
/// The word that starts a cubic, followed by six coordinates.
pub(crate) const CUBIC: i32 = 3;
/// The word of a close.
pub(crate) const CLOSE: i32 = 4;

/// One distinct outline, in device pixels from its anchor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// A tag, then the tag's coordinates in 1/256 device pixels: [`MOVE`], [`LINE`], [`QUAD`],
    /// [`CUBIC`] and [`CLOSE`].
    pub(crate) commands: Box<[i32]>,
    /// The control box in device pixels from the anchor, as `[x0, y0, x1, y1]`, before any
    /// stroke reach.
    pub(crate) bounds: [f32; 4],
    /// The signed area of the control polygon, in device pixels squared.
    pub(crate) area: f64,
}

/// A path as anchored copies of a few distinct outlines.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Split {
    /// The distinct outlines, at most [`MAX_GEOMETRIES`].
    pub(crate) geometries: Box<[Geometry]>,
    /// The move point of each subpath, in the path's own units.
    pub(crate) anchors: Box<[[f32; 2]]>,
    /// The outline of each anchor, as an index into `geometries`.
    pub(crate) of: Box<[u8]>,
}

/// Why a path is no set of repeated outlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitDeclined {
    /// It has fewer than [`MIN_SUBPATHS`] subpaths.
    Few,
    /// It has too many distinct outlines, found after reading this many subpaths.
    Distinct {
        /// The subpaths read.
        read: usize,
    },
    /// A point lies more than 64 device pixels from its anchor.
    Large,
    /// A point is not finite, or the path does not start with a move.
    Shape,
}

/// The subpaths of `path` as anchored outlines under the linear map `linear`, `[a, b, c, d]` as
/// kurbo orders them, or why it is none.
///
/// The moves are counted first, so a path of fewer than [`MIN_SUBPATHS`] subpaths declines with no
/// other work. More than 16 distinct outlines among the first 64 subpaths decline, and so do more
/// than `min(64, N / 8)` after them. A repeat costs no allocation.
pub(crate) fn split(path: &BezPath, linear: [f64; 4]) -> Result<Split, SplitDeclined> {
    let elements = path.elements();
    let moves = elements
        .iter()
        .filter(|element| matches!(element, PathEl::MoveTo(_)))
        .count();
    if moves < MIN_SUBPATHS {
        return Err(SplitDeclined::Few);
    }
    if !matches!(elements.first(), Some(PathEl::MoveTo(_))) {
        return Err(SplitDeclined::Shape);
    }
    let first = FIRST_READ.min(moves);
    let later = MAX_GEOMETRIES.min(moves / 8);
    let mut encoded: Vec<i32> = Vec::new();
    let mut store: Vec<i32> = Vec::new();
    let mut distinct: Vec<(u64, Range<usize>)> = Vec::new();
    let mut geometries: Vec<Geometry> = Vec::new();
    let mut anchors = Vec::with_capacity(moves);
    let mut of = Vec::with_capacity(moves);
    let mut start = 0;
    for read in 1..=moves {
        let end = elements[start + 1..]
            .iter()
            .position(|element| matches!(element, PathEl::MoveTo(_)))
            .map_or(elements.len(), |at| start + 1 + at);
        let subpath = &elements[start..end];
        let PathEl::MoveTo(anchor) = subpath[0] else {
            return Err(SplitDeclined::Shape);
        };
        encoded.clear();
        encode(subpath, anchor, linear, &mut encoded)?;
        let hash = hash(&encoded);
        let found = distinct
            .iter()
            .position(|(held, range)| *held == hash && store[range.clone()] == encoded[..]);
        let index = match found {
            Some(index) => index,
            None => {
                let at = store.len();
                store.extend_from_slice(&encoded);
                distinct.push((hash, at..store.len()));
                geometries.push(geometry(&encoded));
                distinct.len() - 1
            }
        };
        let allowed = if read <= first { FIRST_DISTINCT } else { later };
        if distinct.len() > allowed {
            return Err(SplitDeclined::Distinct { read });
        }
        let (x, y) = (anchor.x as f32, anchor.y as f32);
        if !(x.is_finite() && y.is_finite()) {
            return Err(SplitDeclined::Shape);
        }
        anchors.push([x, y]);
        of.push(index as u8);
        start = end;
    }
    Ok(Split {
        geometries: geometries.into_boxed_slice(),
        anchors: anchors.into_boxed_slice(),
        of: of.into_boxed_slice(),
    })
}

/// The whole of `path` as one outline anchored at its origin, under the linear map `linear`.
///
/// What a marker is: one outline of any number of subpaths, placed by its origin on each point.
pub(crate) fn geometry_of(path: &BezPath, linear: [f64; 4]) -> Result<Geometry, SplitDeclined> {
    let elements = path.elements();
    if !matches!(elements.first(), Some(PathEl::MoveTo(_))) {
        return Err(SplitDeclined::Shape);
    }
    let mut encoded = Vec::new();
    encode(elements, Point::ORIGIN, linear, &mut encoded)?;
    Ok(geometry(&encoded))
}

/// The pixel a device coordinate is drawn from and the quarter-pixel phase it is rasterised at.
///
/// Quantised first and split after, as a pen position is, so the two halves agree about the pixel.
/// `pixel + phase / 4` is within 1/8 of a pixel of `device`. The shader splits with the same
/// formula, in the same precision.
pub(crate) fn phase_of(device: f32) -> (i32, u8) {
    let quantised = (4.0 * device + 0.5).floor();
    let phase = quantised.rem_euclid(4.0);
    (((quantised - phase) / 4.0) as i32, phase as u8)
}

/// Writes `elements` relative to `anchor`, mapped by `linear` and in words, into `out`.
fn encode(
    elements: &[PathEl],
    anchor: Point,
    linear: [f64; 4],
    out: &mut Vec<i32>,
) -> Result<(), SplitDeclined> {
    let [a, b, c, d] = linear;
    let word = |value: f64| -> Result<i32, SplitDeclined> {
        let scaled = (value * UNITS).round();
        if !scaled.is_finite() {
            return Err(SplitDeclined::Shape);
        }
        if scaled.abs() > REACH {
            return Err(SplitDeclined::Large);
        }
        Ok(scaled as i32)
    };
    let point = |out: &mut Vec<i32>, p: Point| -> Result<(), SplitDeclined> {
        let (x, y) = (p.x - anchor.x, p.y - anchor.y);
        out.push(word(a * x + c * y)?);
        out.push(word(b * x + d * y)?);
        Ok(())
    };
    for element in elements {
        match *element {
            PathEl::MoveTo(p) => {
                out.push(MOVE);
                point(out, p)?;
            }
            PathEl::LineTo(p) => {
                out.push(LINE);
                point(out, p)?;
            }
            PathEl::QuadTo(p1, p2) => {
                out.push(QUAD);
                point(out, p1)?;
                point(out, p2)?;
            }
            PathEl::CurveTo(p1, p2, p3) => {
                out.push(CUBIC);
                point(out, p1)?;
                point(out, p2)?;
                point(out, p3)?;
            }
            PathEl::ClosePath => out.push(CLOSE),
        }
    }
    Ok(())
}

/// The hash two subpaths are first compared by.
fn hash(words: &[i32]) -> u64 {
    let mut hasher = FxHasher::default();
    for &word in words {
        hasher.write_i32(word);
    }
    hasher.finish()
}

/// The outline of `words`, with its control box and the signed area of its control polygon.
fn geometry(words: &[i32]) -> Geometry {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut twice_area = 0.0f64;
    let mut first: Option<(f64, f64)> = None;
    let mut last = (0.0f64, 0.0f64);
    each_point(words, |x, y| {
        let (x, y) = (f64::from(x) / UNITS, f64::from(y) / UNITS);
        bounds = [
            bounds[0].min(x as f32),
            bounds[1].min(y as f32),
            bounds[2].max(x as f32),
            bounds[3].max(y as f32),
        ];
        if first.is_some() {
            twice_area += last.0 * y - x * last.1;
        }
        first.get_or_insert((x, y));
        last = (x, y);
    });
    if let Some(first) = first {
        twice_area += last.0 * first.1 - first.0 * last.1;
    }
    if !bounds[0].is_finite() {
        bounds = [0.0; 4];
    }
    Geometry {
        commands: words.into(),
        bounds,
        area: twice_area / 2.0,
    }
}

/// Calls `visit` with every point of `words`, control points included, in words.
pub(crate) fn each_point(words: &[i32], mut visit: impl FnMut(i32, i32)) {
    let mut at = 0;
    while let Some(&tag) = words.get(at) {
        let count = match tag {
            MOVE | LINE => 1,
            QUAD => 2,
            CUBIC => 3,
            _ => 0,
        };
        for index in 0..count {
            visit(words[at + 1 + 2 * index], words[at + 2 + 2 * index]);
        }
        at += 1 + 2 * count;
    }
}

#[cfg(test)]
mod tests;
