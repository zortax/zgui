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
use zgui_scene::kurbo::{self, BezPath, PathEl, Point};

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

/// How far a flattened outline may lie from its curves in the winding test, in device pixels.
const FLATTEN: f64 = 1.0 / 16.0;

/// One distinct outline, in device pixels from its anchor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// A tag, then the tag's coordinates in 1/256 device pixels: [`MOVE`], [`LINE`], [`QUAD`],
    /// [`CUBIC`] and [`CLOSE`].
    pub(crate) commands: Box<[i32]>,
    /// The control box in device pixels from the anchor, as `[x0, y0, x1, y1]`, before any
    /// stroke reach.
    pub(crate) bounds: [f32; 4],
    /// The signs its winding number takes under the nonzero rule.
    pub(crate) winding: Winding,
}

/// The signs the winding number of an outline takes where it is not zero.
///
/// Copies of outlines that all wind one way paint the union of their coverage under the nonzero
/// rule. A copy that winds the other way over another cancels it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Winding {
    /// Positive somewhere: clockwise on the screen.
    pub(crate) positive: bool,
    /// Negative somewhere.
    pub(crate) negative: bool,
}

impl Winding {
    /// Both signs.
    const MIXED: Self = Self {
        positive: true,
        negative: true,
    };

    /// The signs of either.
    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            positive: self.positive || other.positive,
            negative: self.negative || other.negative,
        }
    }

    /// Whether both signs occur.
    pub(crate) fn mixed(self) -> bool {
        self.positive && self.negative
    }
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

/// The outline of `words`, with its control box and its winding.
fn geometry(words: &[i32]) -> Geometry {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    each_point(words, |x, y| {
        let (x, y) = ((f64::from(x) / UNITS) as f32, (f64::from(y) / UNITS) as f32);
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x),
            bounds[3].max(y),
        ];
    });
    if !bounds[0].is_finite() {
        bounds = [0.0; 4];
    }
    Geometry {
        commands: words.into(),
        bounds,
        winding: winding(words),
    }
}

/// The signs the winding number of `words` takes, from its outline flattened to within
/// [`FLATTEN`].
///
/// An outline that crosses or touches itself, or whose subpaths cross or touch, counts as both
/// signs. This is exact for a figure-eight and too strict for a pentagram, whose winding is one
/// and two. Otherwise each subpath is a simple loop with no other on its boundary, so the winding
/// just inside a loop is its own sign plus the winding of the others at any of its points, and
/// every region lies just inside one loop.
fn winding(words: &[i32]) -> Winding {
    let mut loops: Vec<Vec<Point>> = Vec::new();
    kurbo::flatten(elements(words), FLATTEN, |element| match element {
        PathEl::MoveTo(p) => loops.push(vec![p]),
        PathEl::LineTo(p) => {
            if let Some(points) = loops.last_mut()
                && points.last() != Some(&p)
            {
                points.push(p);
            }
        }
        _ => {}
    });
    for points in &mut loops {
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
    }
    // Two points or fewer enclose nothing.
    loops.retain(|points| points.len() >= 3);
    if crosses(&loops) {
        return Winding::MIXED;
    }
    let mut found = Winding::default();
    for (index, points) in loops.iter().enumerate() {
        let twice_area: f64 = edges(points).map(|(a, b)| a.x * b.y - b.x * a.y).sum();
        let own = if twice_area > 0.0 {
            1
        } else if twice_area < 0.0 {
            -1
        } else {
            continue;
        };
        let inside = own
            + loops
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .map(|(_, others)| winding_at(others, points[0]))
                .sum::<i32>();
        found.positive |= inside > 0;
        found.negative |= inside < 0;
    }
    found
}

/// The edges of a closed loop of `points`, the closing edge last.
fn edges(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
}

/// Whether two edges of `loops` that do not follow each other in one loop meet.
///
/// The edges are swept by their left end, so only edges whose spans overlap on both axes are
/// tested.
fn crosses(loops: &[Vec<Point>]) -> bool {
    struct Edge {
        a: Point,
        b: Point,
        x: [f64; 2],
        y: [f64; 2],
        of: usize,
        at: usize,
        len: usize,
    }
    let mut all: Vec<Edge> = Vec::new();
    for (of, points) in loops.iter().enumerate() {
        for (at, (a, b)) in edges(points).enumerate() {
            all.push(Edge {
                a,
                b,
                x: [a.x.min(b.x), a.x.max(b.x)],
                y: [a.y.min(b.y), a.y.max(b.y)],
                of,
                at,
                len: points.len(),
            });
        }
    }
    all.sort_unstable_by(|first, second| first.x[0].total_cmp(&second.x[0]));
    for (index, first) in all.iter().enumerate() {
        for second in &all[index + 1..] {
            if second.x[0] > first.x[1] {
                break;
            }
            if second.y[0] > first.y[1] || second.y[1] < first.y[0] {
                continue;
            }
            let next = |edge: &Edge, other: &Edge| (edge.at + 1) % edge.len == other.at;
            if first.of == second.of && (next(first, second) || next(second, first)) {
                continue;
            }
            if meet(first.a, first.b, second.a, second.b) {
                return true;
            }
        }
    }
    false
}

/// Twice the signed area of the triangle `o`, `a`, `b`.
fn turn(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether the segments `a`–`b` and `c`–`d` share a point.
fn meet(a: Point, b: Point, c: Point, d: Point) -> bool {
    let within = |a: Point, b: Point, p: Point| {
        p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
    };
    let [d1, d2, d3, d4] = [turn(c, d, a), turn(c, d, b), turn(a, b, c), turn(a, b, d)];
    let apart = |p: f64, q: f64| (p > 0.0 && q < 0.0) || (p < 0.0 && q > 0.0);
    (apart(d1, d2) && apart(d3, d4))
        || (d1 == 0.0 && within(c, d, a))
        || (d2 == 0.0 && within(c, d, b))
        || (d3 == 0.0 && within(a, b, c))
        || (d4 == 0.0 && within(a, b, d))
}

/// The winding number of the loop `points` about `p`, which is on no edge.
fn winding_at(points: &[Point], p: Point) -> i32 {
    let mut winding = 0;
    for (a, b) in edges(points) {
        if a.y <= p.y {
            if b.y > p.y && turn(a, b, p) > 0.0 {
                winding += 1;
            }
        } else if b.y <= p.y && turn(a, b, p) < 0.0 {
            winding -= 1;
        }
    }
    winding
}

/// The path elements of `words`, in device pixels.
fn elements(words: &[i32]) -> impl Iterator<Item = PathEl> + '_ {
    let point = move |at: usize| {
        Point::new(
            f64::from(words[at]) / UNITS,
            f64::from(words[at + 1]) / UNITS,
        )
    };
    let mut at = 0;
    core::iter::from_fn(move || {
        let tag = *words.get(at)?;
        let element = match tag {
            MOVE => PathEl::MoveTo(point(at + 1)),
            LINE => PathEl::LineTo(point(at + 1)),
            QUAD => PathEl::QuadTo(point(at + 1), point(at + 3)),
            CUBIC => PathEl::CurveTo(point(at + 1), point(at + 3), point(at + 5)),
            _ => PathEl::ClosePath,
        };
        at += 1 + match tag {
            MOVE | LINE => 2,
            QUAD => 4,
            CUBIC => 6,
            _ => 0,
        };
        Some(element)
    })
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
