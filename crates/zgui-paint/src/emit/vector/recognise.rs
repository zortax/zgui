//! Analytic shapes in a path: circles, axis-aligned ellipses, rectangles, rounded rectangles and
//! simple strokes.
//!
//! Pure geometry with no scene access, computed in `f64` and stored in `f32`. Recognition is all or
//! nothing: one subpath that is no accepted shape declines the whole path. Every distance is
//! compared against `tau`, one sixteenth of a device pixel in the path's own units, so a shape
//! drawn from the result lies within a sixteenth of a pixel of the outline it replaces.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_scene::kurbo::{self, BezPath, CubicBez, ParamCurve, PathEl, Point, Vec2};
use zgui_scene::peniko;

#[cfg(test)]
mod tests;

/// One part of a shape: its interior or its outline.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Part<'a> {
    /// The interior under a fill rule.
    ///
    /// The rule changes nothing here. Recognised subpaths are simple, and the caller requires
    /// them to be apart before it draws them one by one.
    #[allow(
        dead_code,
        reason = "the rule is part of what a part is, and nothing reads it yet"
    )]
    Fill(peniko::Fill),
    /// The outline a stroke style produces.
    Stroke(&'a kurbo::Stroke),
}

/// How far recognition may go.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    /// The largest error allowed, in the path's own units.
    pub(crate) tau: f64,
    /// The most primitives one path may become.
    pub(crate) max_prims: usize,
}

/// A rounded, bordered rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoxPrim {
    /// The outer edge, as `[x0, y0, x1, y1]`.
    pub(crate) rect: [f32; 4],
    /// The elliptical corner radii in quad order: top left, top right, bottom right, bottom left,
    /// each as `x, y`.
    pub(crate) radii: [f32; 8],
    /// The superellipse exponent of the corners: 2 is round, 1 is a bevel.
    pub(crate) exponent: f32,
    /// The border width inside the outer edge: 0 for a fill, the stroke width for a stroke.
    pub(crate) border: f32,
}

/// The direction the closed subpaths of a path turn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Orientation {
    /// All of them turn with a positive cross product.
    Positive,
    /// All of them turn with a negative cross product.
    Negative,
    /// Some turn each way.
    Mixed,
}

/// The primitives one part of a path is made of, in its own space.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Decomposition {
    /// Discs and rings, as `[cx, cy, outer radius, inner radius]`. A solid disc has an inner
    /// radius of 0.
    pub(crate) discs: Vec<[f32; 4]>,
    /// Rectangles, rounded rectangles and axis-aligned ellipses.
    pub(crate) boxes: Vec<BoxPrim>,
    /// Stroked line segments, as `[x0, y0, x1, y1]`.
    pub(crate) capsules: Vec<[f32; 4]>,
    /// One entry per capsule: the start cap in bits 0 and 1, the end cap in bits 2 and 3. See
    /// [`BUTT`], [`SQUARE`] and [`ROUND`].
    pub(crate) caps: Vec<u8>,
    /// Half the stroke width, or 0 for a fill.
    pub(crate) half_width: f32,
    /// The union of the primitives' outer bounds, as `[x0, y0, x1, y1]`.
    pub(crate) ink: [f32; 4],
    /// The turning direction of the closed subpaths.
    pub(crate) orientation: Orientation,
    /// How many primitives there are.
    pub(crate) count: usize,
    /// The longest side of any primitive's bounds.
    pub(crate) max_extent: f32,
}

/// A cap that stops at the end of the segment.
pub(crate) const BUTT: u8 = 0;
/// A cap that continues the segment by half the stroke width.
pub(crate) const SQUARE: u8 = 1;
/// A half disc of half the stroke width.
pub(crate) const ROUND: u8 = 2;

/// The fewest and the most cubics a circle or an ellipse may have.
const ARCS: core::ops::RangeInclusive<usize> = 4..=64;

/// The most cubics one rounded corner may have.
const CORNER_CUBICS: usize = 16;

/// The most segments one subpath may have before any are merged.
const SEGMENTS: usize = 256;

/// The largest cosine between an arc's end tangent and its radius.
const PERPENDICULAR: f64 = 0.02;

/// The largest cosine between an axis-aligned tangent and the other axis.
const AXIAL: f64 = 1.0e-3;

/// How far the turns of a closed curve may sum from a full turn, in radians.
const FULL_TURN: f64 = 1.0e-3;

/// How many rectangles one grid cell may hold before separation is not proven.
const CROWDED: usize = 16;

/// One sixteenth of a device pixel in local units, or `None` for a degenerate or non-finite
/// matrix.
///
/// Measured along the direction the matrix stretches most, so the bound holds in every direction.
pub(crate) fn tau(affine: &zgui_geom::Affine2) -> Option<f64> {
    let [a, b, c, d] = [affine.a, affine.b, affine.c, affine.d].map(f64::from);
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    // The largest singular value: the root of the larger eigenvalue of AᵀA.
    let p = a * a + b * b;
    let q = c * c + d * d;
    let r = a * c + b * d;
    let larger = (p + q) / 2.0 + (((p - q) / 2.0).powi(2) + r * r).sqrt();
    let sigma = larger.sqrt();
    (sigma.is_finite() && sigma > 0.0).then(|| 1.0 / 16.0 / sigma)
}

/// The primitives `part` of `path` is made of, or `None` when one subpath is no accepted shape.
#[cfg(test)]
pub(crate) fn recognise(path: &BezPath, part: Part<'_>, limits: Limits) -> Option<Decomposition> {
    recognise_or_decline(path, part, limits).ok()
}

/// Why a path is no decomposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Declined {
    /// It has more subpaths or prims than the limit allows. A higher limit may accept it.
    Limit,
    /// One of its subpaths is no accepted shape, or the part cannot be drawn. No limit accepts it.
    Shape,
}

/// The primitives `part` of `path` is made of, or why it is not made of any.
pub(crate) fn recognise_or_decline(
    path: &BezPath,
    part: Part<'_>,
    limits: Limits,
) -> Result<Decomposition, Declined> {
    let tau = limits.tau;
    if !tau.is_finite() || tau <= 0.0 {
        return Err(Declined::Shape);
    }
    let elements = path.elements();
    if let Part::Stroke(style) = part {
        let width = style.width;
        if !style.dash_pattern.is_empty() || !width.is_finite() || width <= 0.0 {
            return Err(Declined::Shape);
        }
        if !style.miter_limit.is_finite() {
            return Err(Declined::Shape);
        }
    }
    let found = if elements.len() >= PARALLEL_ELEMENTS && threads() > 1 {
        recognise_in_parallel(elements, part, tau, limits.max_prims, threads())?
    } else {
        let moves = elements
            .iter()
            .filter(|element| matches!(element, PathEl::MoveTo(_)))
            .count();
        if moves > limits.max_prims {
            return Err(Declined::Limit);
        }
        recognise_run(elements, part, tau).ok_or(Declined::Shape)?
    };
    found.finish(limits.max_prims).ok_or(Declined::Limit)
}

/// How many path elements make a path worth recognising on several threads: about 16 384
/// circles of a move, four cubics and a close.
pub(crate) const PARALLEL_ELEMENTS: usize = 16_384 * 6;

/// The most threads one recognition uses.
const MAX_THREADS: usize = 8;

/// How many threads a large recognition is split across.
///
/// Asked once: the answer reads the process's scheduling limits, which costs more than
/// recognising a small path does.
fn threads() -> usize {
    static THREADS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *THREADS.get_or_init(|| {
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(MAX_THREADS)
    })
}

/// What the subpaths of `elements` are, or `None` when one of them is no accepted shape.
fn recognise_run(elements: &[PathEl], part: Part<'_>, tau: f64) -> Option<Found> {
    let mut found = Found::default();
    let mut scratch = Segments::new();
    let mut template = Template::default();
    let tolerance = tau * REPEAT;
    match part {
        Part::Fill(_) => {
            each_subpath(elements, tau, |subpath| {
                found.subpaths += 1;
                if template.repeat(subpath, tolerance, &mut found) {
                    return Some(());
                }
                let before = found.counts();
                fill(subpath, tau, &mut found, &mut scratch)?;
                template.learn(subpath, before, &found);
                Some(())
            })?;
        }
        Part::Stroke(style) => {
            found.half_width = style.width / 2.0;
            each_subpath(elements, tau, |subpath| {
                found.subpaths += 1;
                if subpath.closed && template.repeat(subpath, tolerance, &mut found) {
                    return Some(());
                }
                let before = found.counts();
                stroke(subpath, style, tau, &mut found, &mut scratch)?;
                if subpath.closed {
                    template.learn(subpath, before, &found);
                }
                Some(())
            })?;
        }
    }
    Some(found)
}

/// How far, as a fraction of tau, a subpath may lie from the last recognised one and repeat it.
///
/// A plot draws every marker with the same commands at a different place, and the place moves the
/// rounding of every coordinate. A repeat is a translated copy within this much, and takes the
/// prim the first copy became, moved: its error is the first copy's plus at most this fraction of
/// tau.
const REPEAT: f64 = 1.0 / 1024.0;

/// The last closed subpath that became exactly one disc or box, moved to start at the origin.
#[derive(Debug, Default)]
struct Template {
    /// Its segments, moved. Empty while there is no template.
    segments: Segments,
    /// Where it ended, moved.
    end: Point,
    /// Whether it ended with a close.
    closed: bool,
    /// What it became, moved the same way.
    prim: Option<Repeated>,
    /// The turning sign of the subpath.
    sign: f64,
}

/// The one prim a template became, relative to the template's start.
#[derive(Clone, Copy, Debug)]
enum Repeated {
    /// A disc or a ring: centre, outer radius and inner radius.
    Disc([f64; 4]),
    /// A box: its outer edge, radii, exponent and border.
    Box([f64; 4], [f32; 8], f32, f32),
}

impl Template {
    /// Adds the template's prim at `subpath`'s start when `subpath` repeats it within
    /// `tolerance`, and reports whether it did.
    fn repeat(&self, subpath: &Subpath, tolerance: f64, found: &mut Found) -> bool {
        let Some(prim) = self.prim else {
            return false;
        };
        let start = subpath.start;
        let near = |point: Point, held: Point| {
            let moved = point - start;
            (moved.x - held.x).abs() <= tolerance && (moved.y - held.y).abs() <= tolerance
        };
        let same = subpath.closed == self.closed
            && subpath.segments.len() == self.segments.len()
            && near(subpath.end, self.end)
            && subpath
                .segments
                .iter()
                .zip(&self.segments)
                .all(|pair| match pair {
                    (Segment::Line(_, to), Segment::Line(_, held)) => near(*to, *held),
                    (Segment::Cubic(cubic), Segment::Cubic(held)) => {
                        near(cubic.p1, held.p1)
                            && near(cubic.p2, held.p2)
                            && near(cubic.p3, held.p3)
                    }
                    _ => false,
                });
        if !same {
            return false;
        }
        found.turned(self.sign);
        match prim {
            Repeated::Disc([x, y, outer, inner]) => {
                found.disc([x + start.x, y + start.y, outer, inner]);
            }
            Repeated::Box([x0, y0, x1, y1], radii, exponent, border) => {
                let rect = [x0 + start.x, y0 + start.y, x1 + start.x, y1 + start.y];
                found.boxed(rect, radii, exponent, border);
            }
        }
        true
    }

    /// Keeps `subpath` as the template when it became exactly one disc or box.
    fn learn(&mut self, subpath: &Subpath, before: [usize; 3], found: &Found) {
        let after = found.counts();
        let start = subpath.start.to_vec2();
        let prim = match [
            after[0] - before[0],
            after[1] - before[1],
            after[2] - before[2],
        ] {
            [1, 0, 0] => {
                let [x, y, outer, inner] = found.last;
                Repeated::Disc([x - start.x, y - start.y, outer, inner])
            }
            [0, 1, 0] => {
                let held = &found.boxes[after[1] - 1];
                let [x0, y0, x1, y1] = found.last;
                Repeated::Box(
                    [x0 - start.x, y0 - start.y, x1 - start.x, y1 - start.y],
                    held.radii,
                    held.exponent,
                    held.border,
                )
            }
            _ => {
                self.prim = None;
                return;
            }
        };
        self.segments.clear();
        self.segments
            .extend(subpath.segments.iter().map(|segment| match *segment {
                Segment::Line(from, to) => Segment::Line(from - start, to - start),
                Segment::Cubic(cubic) => Segment::Cubic(CubicBez::new(
                    cubic.p0 - start,
                    cubic.p1 - start,
                    cubic.p2 - start,
                    cubic.p3 - start,
                )),
            }));
        self.end = subpath.end - start;
        self.closed = subpath.closed;
        self.sign = found.last_sign;
        self.prim = Some(prim);
    }
}

/// The same, with the elements split into contiguous runs at moves, one run per thread, and the
/// runs joined in order.
///
/// Each run starts at the first move at or after an even share of the elements, so finding the
/// runs reads a few elements rather than the whole path. More than `max_prims` subpaths answer
/// `None`, as one run would.
fn recognise_in_parallel(
    elements: &[PathEl],
    part: Part<'_>,
    tau: f64,
    max_prims: usize,
    threads: usize,
) -> Result<Found, Declined> {
    let mut starts = Vec::with_capacity(threads + 1);
    starts.push(0);
    for share in 1..threads {
        let from = (elements.len() * share / threads).max(*starts.last().unwrap_or(&0));
        let at = elements[from..]
            .iter()
            .position(|element| matches!(element, PathEl::MoveTo(_)))
            .map_or(elements.len(), |offset| from + offset);
        starts.push(at);
    }
    starts.push(elements.len());
    let runs: Vec<Result<Found, Declined>> = std::thread::scope(|scope| {
        let handles: Vec<_> = starts
            .windows(2)
            .filter(|bounds| bounds[0] < bounds[1])
            .map(|bounds| {
                let run = &elements[bounds[0]..bounds[1]];
                scope.spawn(move || {
                    let found = recognise_run(run, part, tau).ok_or(Declined::Shape)?;
                    if found.subpaths > max_prims {
                        return Err(Declined::Limit);
                    }
                    Ok(found)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Err(Declined::Shape)))
            .collect()
    });
    let mut joined: Option<Found> = None;
    for run in runs {
        let run = run?;
        joined = Some(match joined {
            None => run,
            Some(mut held) => {
                held.join(run);
                held
            }
        });
    }
    match joined {
        Some(found) if found.subpaths <= max_prims => Ok(found),
        Some(_) => Err(Declined::Limit),
        None => Err(Declined::Shape),
    }
}

/// Whether the device rectangles `rects`, each as `[x0, y0, x1, y1]` and inflated by `margin` on
/// every side, are pairwise disjoint.
///
/// Touching edges count as disjoint. A grid cell crowded past a fixed count answers false, which
/// means "not proven" and keeps the cost linear.
pub(crate) fn separated(rects: &[[f32; 4]], margin: f32) -> bool {
    let largest = rects
        .iter()
        .map(|rect| (rect[2] - rect[0]).max(rect[3] - rect[1]))
        .fold(0.0f32, f32::max);
    separated_with(rects.len(), largest, margin, |index| rects[index])
}

/// [`separated`] over `count` rectangles made one at a time by `rect`, none of whose sides is
/// longer than `largest`.
///
/// The rectangles are made only as far as the first overlap, so a path of many overlapping prims
/// answers after a few of them.
pub(crate) fn separated_with(
    count: usize,
    largest: f32,
    margin: f32,
    rect: impl Fn(usize) -> [f32; 4],
) -> bool {
    if count < 2 {
        return true;
    }
    // The cell is as large as the largest rectangle, so each one meets at most four cells.
    let cell = largest + 2.0 * margin;
    if !cell.is_finite() || cell <= 0.0 {
        return false;
    }
    let mut inflated: Vec<[f32; 4]> = Vec::new();
    let mut grid: FxHashMap<(i32, i32), SmallVec<[u32; 4]>> = FxHashMap::default();
    for index in 0..count {
        let [x0, y0, x1, y1] = rect(index);
        let rect = [x0 - margin, y0 - margin, x1 + margin, y1 + margin];
        if rect.iter().any(|value| !value.is_finite()) {
            return false;
        }
        let cells =
            |low: f32, high: f32| ((low / cell).floor() as i32)..=((high / cell).floor() as i32);
        for x in cells(rect[0], rect[2]) {
            for y in cells(rect[1], rect[3]) {
                let entries = grid.entry((x, y)).or_default();
                let overlaps = entries.iter().any(|&other| {
                    let other = inflated[other as usize];
                    rect[0] < other[2]
                        && other[0] < rect[2]
                        && rect[1] < other[3]
                        && other[1] < rect[3]
                });
                if overlaps {
                    return false;
                }
                entries.push(index as u32);
                if entries.len() > CROWDED {
                    return false;
                }
            }
        }
        inflated.push(rect);
    }
    true
}

/// One segment of a subpath.
#[derive(Clone, Copy, Debug)]
enum Segment {
    /// A straight line from the first point to the second.
    Line(Point, Point),
    /// A cubic Bézier.
    Cubic(CubicBez),
}

impl Segment {
    /// Where it starts.
    fn start(&self) -> Point {
        match self {
            Self::Line(start, _) => *start,
            Self::Cubic(cubic) => cubic.p0,
        }
    }

    /// Where it ends.
    fn end(&self) -> Point {
        match self {
            Self::Line(_, end) => *end,
            Self::Cubic(cubic) => cubic.p3,
        }
    }
}

/// The segments of one subpath, held inline while there are few of them.
type Segments = SmallVec<[Segment; 8]>;

/// One subpath as written, with its degenerate segments dropped.
#[derive(Debug, Default)]
struct Subpath {
    /// The segments.
    segments: Segments,
    /// The point its move went to.
    start: Point,
    /// The point it ended at.
    end: Point,
    /// Whether it ended with a close.
    closed: bool,
    /// Whether anything followed its move.
    drawn: bool,
}

/// Calls `visit` with each subpath of `path`, and answers `None` as soon as one call does.
///
/// Also `None` for a quadratic, a non-finite point, a segment before any move, or too many
/// segments in one subpath. One subpath is held at a time, in storage reused for the next.
fn each_subpath(
    elements: &[PathEl],
    tau: f64,
    mut visit: impl FnMut(&Subpath) -> Option<()>,
) -> Option<()> {
    let short = tau / 8.0;
    let finite = |point: Point| point.is_finite().then_some(point);
    let mut subpath = Subpath::default();
    let mut open = false;
    for element in elements {
        match *element {
            PathEl::MoveTo(point) => {
                let point = finite(point)?;
                if open {
                    visit(&subpath)?;
                }
                subpath.segments.clear();
                subpath.start = point;
                subpath.end = point;
                subpath.closed = false;
                subpath.drawn = false;
                open = true;
            }
            PathEl::LineTo(point) => {
                let point = finite(point)?;
                if !open {
                    return None;
                }
                subpath.drawn = true;
                if (point - subpath.end).hypot2() >= short * short {
                    subpath.segments.push(Segment::Line(subpath.end, point));
                    subpath.end = point;
                }
            }
            PathEl::CurveTo(first, second, point) => {
                let [first, second, point] = [finite(first)?, finite(second)?, finite(point)?];
                if !open {
                    return None;
                }
                subpath.drawn = true;
                let points = [subpath.end, first, second, point];
                if spread_squared(&points) >= short * short {
                    subpath.segments.push(Segment::Cubic(CubicBez::new(
                        subpath.end,
                        first,
                        second,
                        point,
                    )));
                    subpath.end = point;
                }
            }
            PathEl::QuadTo(..) => return None,
            PathEl::ClosePath => {
                if !open {
                    return None;
                }
                subpath.closed = true;
                subpath.drawn = true;
                visit(&subpath)?;
                open = false;
            }
        }
        if subpath.segments.len() > SEGMENTS {
            return None;
        }
    }
    if open {
        visit(&subpath)?;
    }
    Some(())
}

/// The square of the largest distance between two of `points`.
fn spread_squared(points: &[Point]) -> f64 {
    let mut largest = 0.0f64;
    for (at, first) in points.iter().enumerate() {
        for second in &points[at + 1..] {
            largest = largest.max((*first - *second).hypot2());
        }
    }
    largest
}

/// Writes the segments of `subpath` into `out`, closed by a line back to its start when `close`
/// and the gap is longer than `gap`, with collinear lines merged.
fn normalise(subpath: &Subpath, close: bool, gap: f64, tau: f64, out: &mut Segments) {
    let closing = (close && (subpath.start - subpath.end).hypot() >= gap)
        .then_some(Segment::Line(subpath.end, subpath.start));
    merge(
        subpath.segments.iter().copied().chain(closing),
        close,
        tau / 8.0,
        out,
    );
}

/// Writes `segments` into `out` with each run of collinear lines that go one way merged into one
/// line.
///
/// Every point inside a run lies within `tolerance` of the merged line. When `cyclic`, a run may
/// continue from the last segment into the first.
fn merge(
    segments: impl Iterator<Item = Segment>,
    cyclic: bool,
    tolerance: f64,
    out: &mut Segments,
) {
    out.clear();
    // The points inside the run the last output line stands for.
    let mut inside: SmallVec<[Point; 4]> = SmallVec::new();
    for segment in segments {
        if let (Some(Segment::Line(start, joint)), Segment::Line(_, end)) =
            (out.last().copied(), segment)
            && run_holds(start, joint, end, &inside, tolerance)
        {
            inside.push(joint);
            *out.last_mut().expect("a line is there") = Segment::Line(start, end);
            continue;
        }
        inside.clear();
        out.push(segment);
    }
    if cyclic
        && out.len() > 2
        && let (Segment::Line(start, joint), Segment::Line(_, end)) = (out[out.len() - 1], out[0])
        && run_holds(start, joint, end, &inside, tolerance)
    {
        out[0] = Segment::Line(start, end);
        out.pop();
    }
}
/// Whether the line from `start` to `end` stands for a run through `joint` and `inside` that goes
/// one way.
fn run_holds(start: Point, joint: Point, end: Point, inside: &[Point], tolerance: f64) -> bool {
    let chord = end - start;
    let length = chord.hypot();
    if length == 0.0 || (joint - start).dot(end - joint) <= 0.0 {
        return false;
    }
    let off = |point: Point| (chord.cross(point - start) / length).abs();
    off(joint) <= tolerance && inside.iter().all(|point| off(*point) <= tolerance)
}

/// What the subpaths of one part became so far.
#[derive(Debug)]
struct Found {
    /// Discs and rings.
    discs: Vec<[f32; 4]>,
    /// Boxes.
    boxes: Vec<BoxPrim>,
    /// Capsules.
    capsules: Vec<[f32; 4]>,
    /// Their caps.
    caps: Vec<u8>,
    /// Half the stroke width.
    half_width: f64,
    /// Whether a closed subpath turned each way.
    turned: [bool; 2],
    /// The union of the bounds so far.
    ink: [f64; 4],
    /// The longest side of any bounds so far.
    max_extent: f64,
    /// The last disc as recognised, or the last box's outer edge, before either is stored in
    /// `f32`.
    last: [f64; 4],
    /// The turning sign of the last closed subpath.
    last_sign: f64,
    /// How many subpaths were visited.
    subpaths: usize,
}

impl Default for Found {
    fn default() -> Self {
        Self {
            discs: Vec::new(),
            boxes: Vec::new(),
            capsules: Vec::new(),
            caps: Vec::new(),
            half_width: 0.0,
            turned: [false; 2],
            ink: [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
            max_extent: 0.0,
            last: [0.0; 4],
            last_sign: 0.0,
            subpaths: 0,
        }
    }
}

impl Found {
    /// Appends what a later run of subpaths became.
    fn join(&mut self, later: Self) {
        self.discs.extend(later.discs);
        self.boxes.extend(later.boxes);
        self.capsules.extend(later.capsules);
        self.caps.extend(later.caps);
        self.turned = [
            self.turned[0] || later.turned[0],
            self.turned[1] || later.turned[1],
        ];
        self.ink = [
            self.ink[0].min(later.ink[0]),
            self.ink[1].min(later.ink[1]),
            self.ink[2].max(later.ink[2]),
            self.ink[3].max(later.ink[3]),
        ];
        self.max_extent = self.max_extent.max(later.max_extent);
        self.subpaths += later.subpaths;
    }

    /// Notes the turning sign of one closed subpath.
    fn turned(&mut self, sign: f64) {
        self.turned[usize::from(sign < 0.0)] = true;
        self.last_sign = sign;
    }

    /// How many discs, boxes and capsules there are so far.
    fn counts(&self) -> [usize; 3] {
        [self.discs.len(), self.boxes.len(), self.capsules.len()]
    }

    /// Takes the bounds of one more primitive into the ink.
    fn bounded(&mut self, bounds: [f64; 4]) {
        self.ink = [
            self.ink[0].min(bounds[0]),
            self.ink[1].min(bounds[1]),
            self.ink[2].max(bounds[2]),
            self.ink[3].max(bounds[3]),
        ];
        self.max_extent = self
            .max_extent
            .max(bounds[2] - bounds[0])
            .max(bounds[3] - bounds[1]);
    }

    /// Adds a disc or a ring.
    fn disc(&mut self, [cx, cy, outer, inner]: [f64; 4]) {
        self.last = [cx, cy, outer, inner];
        self.bounded([cx - outer, cy - outer, cx + outer, cy + outer]);
        self.discs
            .push([cx, cy, outer, inner].map(|value| value as f32));
    }

    /// Adds a box whose outer edge is `rect`.
    fn boxed(&mut self, rect: [f64; 4], radii: [f32; 8], exponent: f32, border: f32) {
        self.last = rect;
        self.bounded(rect);
        self.boxes.push(BoxPrim {
            rect: rect.map(|value| value as f32),
            radii,
            exponent,
            border,
        });
    }

    /// Adds a capsule from `start` to `end` with `caps`.
    fn capsule(&mut self, start: Point, end: Point, caps: u8) {
        let capsule = [start.x, start.y, end.x, end.y];
        self.bounded(capsule_bounds(capsule, caps, self.half_width));
        self.capsules.push(capsule.map(|value| value as f32));
        self.caps.push(caps);
    }

    /// The decomposition, or `None` past `max_prims`.
    fn finish(self, max_prims: usize) -> Option<Decomposition> {
        let count = self.discs.len() + self.boxes.len() + self.capsules.len();
        if count > max_prims {
            return None;
        }
        let orientation = match self.turned {
            [_, false] => Orientation::Positive,
            [false, true] => Orientation::Negative,
            [true, true] => Orientation::Mixed,
        };
        let ink = if count == 0 {
            [0.0; 4]
        } else {
            self.ink.map(|value| value as f32)
        };
        Some(Decomposition {
            discs: self.discs,
            boxes: self.boxes,
            capsules: self.capsules,
            caps: self.caps,
            half_width: self.half_width as f32,
            ink,
            orientation,
            count,
            max_extent: self.max_extent as f32,
        })
    }
}
/// The bounds a capsule paints, with its caps and its width.
fn capsule_bounds(capsule: [f64; 4], caps: u8, half: f64) -> [f64; 4] {
    let start = Point::new(capsule[0], capsule[1]);
    let end = Point::new(capsule[2], capsule[3]);
    let along = end - start;
    let length = along.hypot();
    if length == 0.0 {
        return [
            start.x - half,
            start.y - half,
            start.x + half,
            start.y + half,
        ];
    }
    let unit = along / length;
    let normal = Vec2::new(-unit.y, unit.x) * half;
    // A round end reaches as far as a square one along the segment and less beside it, so the
    // square end bounds both.
    let reach = |cap: u8| if cap == BUTT { 0.0 } else { half };
    let first = start - unit * reach(caps & 3);
    let last = end + unit * reach(caps >> 2);
    let corners = [first + normal, first - normal, last + normal, last - normal];
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for corner in corners {
        bounds = [
            bounds[0].min(corner.x),
            bounds[1].min(corner.y),
            bounds[2].max(corner.x),
            bounds[3].max(corner.y),
        ];
    }
    // A round end is a half disc, which never reaches past the square end's corners but does
    // reach past a diagonal one's bounding box at the end point itself.
    for (cap, point) in [(caps & 3, start), (caps >> 2, end)] {
        if cap == ROUND {
            bounds = [
                bounds[0].min(point.x - half),
                bounds[1].min(point.y - half),
                bounds[2].max(point.x + half),
                bounds[3].max(point.y + half),
            ];
        }
    }
    bounds
}

/// Adds the primitive one filled subpath is, or `None` when it is no accepted shape.
///
/// A subpath that encloses no area adds nothing.
fn fill(subpath: &Subpath, tau: f64, found: &mut Found, scratch: &mut Segments) -> Option<()> {
    // A fill closes every subpath. One that already ends within tau of its start is a ring.
    normalise(subpath, true, tau, tau, scratch);
    let segments = &scratch[..];
    if segments.is_empty() || flat(segments, tau / 4.0) {
        return Some(());
    }
    if let Some(ellipse) = ellipse(segments, tau) {
        found.turned(ellipse.sign);
        let [cx, cy] = [ellipse.centre.x, ellipse.centre.y];
        if (ellipse.rx - ellipse.ry).abs() <= tau {
            found.disc([cx, cy, (ellipse.rx + ellipse.ry) / 2.0, 0.0]);
        } else {
            let (rx, ry) = (ellipse.rx as f32, ellipse.ry as f32);
            found.boxed(
                [
                    cx - ellipse.rx,
                    cy - ellipse.ry,
                    cx + ellipse.rx,
                    cy + ellipse.ry,
                ],
                [rx, ry, rx, ry, rx, ry, rx, ry],
                2.0,
                0.0,
            );
        }
        return Some(());
    }
    let shape = rounded_box(segments, tau)?;
    found.turned(shape.sign);
    let mut radii = [0.0f32; 8];
    for (at, corner) in shape.corners.iter().enumerate() {
        if let Corner::Arc { rx, ry } = *corner {
            radii[at * 2] = rx as f32;
            radii[at * 2 + 1] = ry as f32;
        }
    }
    found.boxed(shape.rect, radii, 2.0, 0.0);
    Some(())
}

/// Whether every point of a polygon lies within `tolerance` of one line, so it encloses no area.
fn flat(segments: &[Segment], tolerance: f64) -> bool {
    if segments
        .iter()
        .any(|segment| matches!(segment, Segment::Cubic(_)))
    {
        return false;
    }
    let first = segments[0].start();
    let Some(far) = segments
        .iter()
        .map(Segment::end)
        .max_by(|one, two| (*one - first).hypot().total_cmp(&(*two - first).hypot()))
    else {
        return true;
    };
    let chord = far - first;
    let length = chord.hypot();
    length == 0.0
        || segments
            .iter()
            .all(|segment| (chord.cross(segment.end() - first) / length).abs() <= tolerance)
}

/// Adds the primitives one stroked subpath is, or `None` when it is no accepted shape.
fn stroke(
    subpath: &Subpath,
    style: &kurbo::Stroke,
    tau: f64,
    found: &mut Found,
    scratch: &mut Segments,
) -> Option<()> {
    let half = style.width / 2.0;
    normalise(subpath, subpath.closed, tau / 8.0, tau, scratch);
    let segments = &scratch[..];
    if segments.is_empty() {
        if !subpath.drawn {
            return Some(());
        }
        if subpath.closed {
            return None;
        }
        // A zero-length segment: its caps meet, so a round pair is a dot and a butt pair is
        // nothing.
        return match (style.start_cap, style.end_cap) {
            (kurbo::Cap::Round, kurbo::Cap::Round) => {
                found.capsule(subpath.start, subpath.start, ROUND | (ROUND << 2));
                Some(())
            }
            (kurbo::Cap::Butt, kurbo::Cap::Butt) => Some(()),
            _ => None,
        };
    }
    if !subpath.closed {
        return polyline(segments, style, tau, found);
    }
    if let Some(ellipse) = ellipse(segments, tau) {
        let radius = (ellipse.rx + ellipse.ry) / 2.0;
        if (ellipse.rx - ellipse.ry).abs() > tau || radius <= half {
            return None;
        }
        found.turned(ellipse.sign);
        found.disc([
            ellipse.centre.x,
            ellipse.centre.y,
            radius + half,
            radius - half,
        ]);
        return Some(());
    }
    let shape = rounded_box(segments, tau)?;
    found.turned(shape.sign);
    let join = join_of(style);
    let arcs = shape
        .corners
        .iter()
        .any(|corner| matches!(corner, Corner::Arc { .. }));
    let mut radii = [0.0f32; 8];
    let mut exponent = 2.0;
    for (at, corner) in shape.corners.iter().enumerate() {
        let radius = match *corner {
            Corner::Arc { rx, ry } => {
                if (rx - ry).abs() > tau {
                    return None;
                }
                (rx + ry) / 2.0 + half
            }
            Corner::Sharp => match join {
                kurbo::Join::Miter => 0.0,
                kurbo::Join::Round => half,
                kurbo::Join::Bevel => {
                    // One exponent cuts every corner, and an arc corner needs the round one.
                    if arcs {
                        return None;
                    }
                    exponent = 1.0;
                    half
                }
            },
        };
        radii[at * 2] = radius as f32;
        radii[at * 2 + 1] = radius as f32;
    }
    let [x0, y0, x1, y1] = shape.rect;
    found.boxed(
        [x0 - half, y0 - half, x1 + half, y1 + half],
        radii,
        exponent,
        style.width as f32,
    );
    Some(())
}

/// The join a stroke draws at a right angle.
///
/// A miter whose limit is below √2, the ratio a right angle needs, is drawn as a bevel there.
fn join_of(style: &kurbo::Stroke) -> kurbo::Join {
    match style.join {
        kurbo::Join::Miter if style.miter_limit < core::f64::consts::SQRT_2 => kurbo::Join::Bevel,
        join => join,
    }
}

/// Adds one capsule per segment of an open polyline, or `None` for a curve or a join no capsule
/// end can draw.
fn polyline(
    segments: &[Segment],
    style: &kurbo::Stroke,
    tau: f64,
    found: &mut Found,
) -> Option<()> {
    let line = |at: usize| match segments[at] {
        Segment::Line(start, end) => Some((start, end)),
        Segment::Cubic(_) => None,
    };
    let join = join_of(style);
    // The cap an inner end takes so the union of two capsules draws the join between them.
    let inner = |one: (Point, Point), two: (Point, Point)| match join {
        kurbo::Join::Round => Some(ROUND),
        kurbo::Join::Miter => {
            let first = axis_of_line(one.1 - one.0, tau)?;
            let second = axis_of_line(two.1 - two.0, tau)?;
            (first.dot(second) == 0.0).then_some(SQUARE)
        }
        kurbo::Join::Bevel => None,
    };
    let last = segments.len() - 1;
    for at in 0..segments.len() {
        let here = line(at)?;
        let start = if at == 0 {
            cap_code(style.start_cap)
        } else {
            inner(line(at - 1)?, here)?
        };
        let end = if at == last {
            cap_code(style.end_cap)
        } else {
            inner(here, line(at + 1)?)?
        };
        found.capsule(here.0, here.1, start | (end << 2));
    }
    Some(())
}

/// The code a cap is stored as.
fn cap_code(cap: kurbo::Cap) -> u8 {
    match cap {
        kurbo::Cap::Butt => BUTT,
        kurbo::Cap::Square => SQUARE,
        kurbo::Cap::Round => ROUND,
    }
}

/// The unit step along the axis a line runs on, or `None` when it leaves that axis by more than
/// a quarter of tau.
fn axis_of_line(along: Vec2, tau: f64) -> Option<Vec2> {
    if along.x.abs() >= along.y.abs() {
        (along.y.abs() <= tau / 4.0 && along.x != 0.0).then(|| Vec2::new(along.x.signum(), 0.0))
    } else {
        (along.x.abs() <= tau / 4.0).then(|| Vec2::new(0.0, along.y.signum()))
    }
}

/// The unit step along the axis a tangent runs on, or `None` when it is not axis-aligned.
fn axis_of_tangent(tangent: Vec2) -> Option<Vec2> {
    let length = tangent.hypot();
    if length == 0.0 || !length.is_finite() {
        return None;
    }
    if tangent.y.abs() <= AXIAL * length {
        Some(Vec2::new(tangent.x.signum(), 0.0))
    } else if tangent.x.abs() <= AXIAL * length {
        Some(Vec2::new(0.0, tangent.y.signum()))
    } else {
        None
    }
}

/// The direction a cubic leaves its start in, from the first control point distinct from it.
fn start_tangent(cubic: &CubicBez) -> Vec2 {
    [cubic.p1, cubic.p2, cubic.p3]
        .into_iter()
        .map(|point| point - cubic.p0)
        .find(|step| step.hypot2() > 0.0)
        .unwrap_or_default()
}

/// The direction a cubic arrives at its end in, from the last control point distinct from it.
fn end_tangent(cubic: &CubicBez) -> Vec2 {
    [cubic.p2, cubic.p1, cubic.p0]
        .into_iter()
        .map(|point| cubic.p3 - point)
        .find(|step| step.hypot2() > 0.0)
        .unwrap_or_default()
}

/// An axis-aligned ellipse a closed run of cubics follows.
#[derive(Clone, Copy, Debug)]
struct Ellipse {
    /// The centre.
    centre: Point,
    /// The horizontal radius.
    rx: f64,
    /// The vertical radius.
    ry: f64,
    /// The sign of its turning.
    sign: f64,
}

/// The axis-aligned ellipse `segments` follow within `tau`, or `None`.
fn ellipse(segments: &[Segment], tau: f64) -> Option<Ellipse> {
    if !ARCS.contains(&segments.len()) {
        return None;
    }
    let cubics: SmallVec<[CubicBez; 8]> = segments
        .iter()
        .map(|segment| match segment {
            Segment::Cubic(cubic) => Some(*cubic),
            Segment::Line(..) => None,
        })
        .collect::<Option<_>>()?;
    let count = cubics.len() as f64;
    let centre = cubics
        .iter()
        .fold(Point::ZERO, |sum, cubic| sum + cubic.p0.to_vec2());
    let centre = Point::new(centre.x / count, centre.y / count);
    // Least squares for 1/rx² and 1/ry² over the start points: A·dx² + B·dy² = 1.
    let (mut xxxx, mut xxyy, mut yyyy, mut xx, mut yy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for cubic in &cubics {
        let dx2 = (cubic.p0.x - centre.x).powi(2);
        let dy2 = (cubic.p0.y - centre.y).powi(2);
        xxxx += dx2 * dx2;
        xxyy += dx2 * dy2;
        yyyy += dy2 * dy2;
        xx += dx2;
        yy += dy2;
    }
    let determinant = xxxx * yyyy - xxyy * xxyy;
    let a = (xx * yyyy - yy * xxyy) / determinant;
    let b = (xxxx * yy - xxyy * xx) / determinant;
    if !(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0) {
        return None;
    }
    let (rx, ry) = (a.sqrt().recip(), b.sqrt().recip());
    if !cubics
        .iter()
        .all(|cubic| follows(cubic, centre, rx, ry, tau))
    {
        return None;
    }
    // Each step between endpoints turns one way by less than a half turn, and the steps add up to
    // one full turn. A step turns by less than a half turn exactly when its cross product is not
    // zero, and its sign says which way. The steps of a closed run add up to a whole number of
    // turns, which pseudo-angles count with no trigonometry.
    let normal = |point: Point| Vec2::new((point.x - centre.x) / rx, (point.y - centre.y) / ry);
    let mut total = 0.0;
    let mut sign = 0.0;
    for cubic in &cubics {
        let (from, to) = (normal(cubic.p0), normal(cubic.p3));
        let cross = from.cross(to);
        if cross == 0.0 || (sign != 0.0 && cross.signum() != sign) {
            return None;
        }
        sign = cross.signum();
        let step = (pseudo_angle(to) - pseudo_angle(from)) * sign;
        total += if step < 0.0 { step + PSEUDO_TURN } else { step };
    }
    if (total - PSEUDO_TURN).abs() > FULL_TURN {
        return None;
    }
    Some(Ellipse {
        centre,
        rx,
        ry,
        sign,
    })
}

/// A full turn in pseudo-angle units.
const PSEUDO_TURN: f64 = 4.0;

/// A number in `0..4` that grows with the angle of `v` from the positive x axis, as the angle
/// does: one per quarter turn, and exact at each axis.
fn pseudo_angle(v: Vec2) -> f64 {
    let sum = v.x.abs() + v.y.abs();
    if sum == 0.0 {
        return 0.0;
    }
    let along = v.x / sum;
    if v.y >= 0.0 { 1.0 - along } else { 3.0 + along }
}

/// The squared radii, in the ellipse's normalised space, between which a point lies within `error`
/// of the ellipse whose larger radius is `largest`.
///
/// `|‖u‖ − 1| · largest ≤ error` holds exactly when `‖u‖²` lies between the two squares, so a
/// test needs no square root.
fn ring(error: f64, largest: f64) -> (f64, f64) {
    let reach = error / largest;
    ((1.0 - reach).max(0.0).powi(2), (1.0 + reach).powi(2))
}

/// Whether `cubic` follows the axis-aligned ellipse at `centre` with radii `rx` and `ry`.
///
/// Its endpoints lie within `tau` of the ellipse, the points a quarter, half and three quarters
/// along within half of it, and its end tangents are perpendicular to the radius.
fn follows(cubic: &CubicBez, centre: Point, rx: f64, ry: f64, tau: f64) -> bool {
    if !(rx > 0.0 && ry > 0.0) {
        return false;
    }
    let largest = rx.max(ry);
    let normal = |point: Point| Vec2::new((point.x - centre.x) / rx, (point.y - centre.y) / ry);
    let within = |point: Point, (low, high): (f64, f64)| {
        let squared = normal(point).hypot2();
        low <= squared && squared <= high
    };
    let ends = ring(tau, largest);
    if !within(cubic.p0, ends) || !within(cubic.p3, ends) {
        return false;
    }
    let samples = ring(tau / 2.0, largest);
    if ![0.25, 0.5, 0.75]
        .into_iter()
        .all(|t| within(cubic.eval(t), samples))
    {
        return false;
    }
    let perpendicular = |point: Point, tangent: Vec2| {
        let radius = normal(point);
        let tangent = Vec2::new(tangent.x / rx, tangent.y / ry);
        let lengths = radius.hypot2() * tangent.hypot2();
        let dot = radius.dot(tangent);
        lengths > 0.0 && dot * dot <= PERPENDICULAR * PERPENDICULAR * lengths
    };
    perpendicular(cubic.p0, start_tangent(cubic)) && perpendicular(cubic.p3, end_tangent(cubic))
}

/// One corner of a recognised box.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Corner {
    /// Two straight edges meet at it.
    Sharp,
    /// An elliptical quarter arc with these radii.
    Arc {
        /// The horizontal radius.
        rx: f64,
        /// The vertical radius.
        ry: f64,
    },
}

/// A rectangle or rounded rectangle a closed subpath draws.
#[derive(Clone, Copy, Debug)]
struct RoundedBox {
    /// The bounds, as `[x0, y0, x1, y1]`.
    rect: [f64; 4],
    /// The corners in quad order: top left, top right, bottom right, bottom left.
    corners: [Corner; 4],
    /// The sign of its turning.
    sign: f64,
}

/// One edge or one rounded corner of a closed subpath.
#[derive(Debug)]
struct Item {
    /// The axis step it starts along.
    enters: Vec2,
    /// The axis step it ends along.
    leaves: Vec2,
    /// Where its segments start in the subpath.
    first: usize,
    /// How many segments it has: one line, or up to [`CORNER_CUBICS`] cubics.
    count: usize,
}

/// The rectangle or rounded rectangle `segments` draw, or `None`.
fn rounded_box(segments: &[Segment], tau: f64) -> Option<RoundedBox> {
    let count = segments.len();
    if count < 2 {
        return None;
    }
    // A joint splits two items when either side is a line, or when the tangent there is
    // axis-aligned: two corners that meet with no edge between them stay two corners.
    let splits = |at: usize| {
        let before = &segments[(at + count - 1) % count];
        let after = &segments[at];
        match (before, after) {
            (Segment::Cubic(before), Segment::Cubic(after)) => {
                axis_of_tangent(end_tangent(before)).is_some()
                    && axis_of_tangent(start_tangent(after)).is_some()
            }
            _ => true,
        }
    };
    let first = (0..count).find(|&at| splits(at))?;
    // The segments of one item, in order.
    let of = |first: usize, len: usize| (0..len).map(move |step| segments[(first + step) % count]);
    let mut items: SmallVec<[Item; 8]> = SmallVec::new();
    for step in 0..count {
        let at = (first + step) % count;
        if step > 0
            && !splits(at)
            && let Some(item) = items.last_mut()
        {
            item.count += 1;
            continue;
        }
        items.push(Item {
            enters: Vec2::ZERO,
            leaves: Vec2::ZERO,
            first: at,
            count: 1,
        });
    }
    for item in &mut items {
        match segments[item.first] {
            Segment::Line(start, end) => {
                let axis = axis_of_line(end - start, tau)?;
                item.enters = axis;
                item.leaves = axis;
            }
            Segment::Cubic(first) => {
                if item.count > CORNER_CUBICS {
                    return None;
                }
                let Segment::Cubic(last) = segments[(item.first + item.count - 1) % count] else {
                    return None;
                };
                item.enters = axis_of_tangent(start_tangent(&first))?;
                item.leaves = axis_of_tangent(end_tangent(&last))?;
                if item.enters.dot(item.leaves) != 0.0 {
                    return None;
                }
            }
        }
    }

    // Corners: every arc, and every joint between two edges. Any other joint is smooth.
    struct Turn {
        /// The step the corner turns from.
        enters: Vec2,
        /// The step it turns to.
        leaves: Vec2,
        /// The vertex of a sharp corner, or the arc's start and end.
        points: (Point, Point),
        /// Whether it is an arc.
        arc: Option<usize>,
    }
    let mut corners: SmallVec<[Turn; 4]> = SmallVec::new();
    for (at, item) in items.iter().enumerate() {
        let next = &items[(at + 1) % items.len()];
        let line = matches!(segments[item.first], Segment::Line(..));
        let next_line = matches!(segments[next.first], Segment::Line(..));
        if !line {
            let start = segments[item.first].start();
            let end = segments[(item.first + item.count - 1) % count].end();
            corners.push(Turn {
                enters: item.enters,
                leaves: item.leaves,
                points: (start, end),
                arc: Some(at),
            });
        }
        if line && next_line {
            if item.leaves.dot(next.enters) != 0.0 {
                return None;
            }
            let vertex = segments[item.first].end();
            corners.push(Turn {
                enters: item.leaves,
                leaves: next.enters,
                points: (vertex, vertex),
                arc: None,
            });
        } else if item.leaves != next.enters {
            return None;
        }
        if corners.len() > 4 {
            return None;
        }
    }
    if corners.len() != 4 {
        return None;
    }
    let sign = corners[0].enters.cross(corners[0].leaves);
    if corners
        .iter()
        .any(|corner| corner.enters.cross(corner.leaves) != sign)
    {
        return None;
    }

    let mut rect = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for segment in segments {
        for point in [segment.start(), segment.end()] {
            rect = [
                rect[0].min(point.x),
                rect[1].min(point.y),
                rect[2].max(point.x),
                rect[3].max(point.y),
            ];
        }
    }
    let [x0, y0, x1, y1] = rect;
    let mut placed: [Option<Corner>; 4] = [None; 4];
    for corner in &corners {
        // The corner points away from the box along the step it enters minus the step it leaves.
        let outward = corner.enters - corner.leaves;
        let quadrant = match (outward.x > 0.0, outward.y > 0.0) {
            (false, false) => 0,
            (true, false) => 1,
            (true, true) => 2,
            (false, true) => 3,
        };
        if placed[quadrant].is_some() {
            return None;
        }
        let (start, end) = corner.points;
        let kind = match corner.arc {
            None => {
                let expected = [
                    Point::new(x0, y0),
                    Point::new(x1, y0),
                    Point::new(x1, y1),
                    Point::new(x0, y1),
                ][quadrant];
                if (start - expected).hypot() > tau {
                    return None;
                }
                Corner::Sharp
            }
            Some(at) => {
                let centre = if corner.enters.y == 0.0 {
                    Point::new(start.x, end.y)
                } else {
                    Point::new(end.x, start.y)
                };
                let (rx, ry) = ((end.x - start.x).abs(), (end.y - start.y).abs());
                let expected = [
                    Point::new(x0 + rx, y0 + ry),
                    Point::new(x1 - rx, y0 + ry),
                    Point::new(x1 - rx, y1 - ry),
                    Point::new(x0 + rx, y1 - ry),
                ][quadrant];
                if (centre - expected).hypot() > tau {
                    return None;
                }
                let follows_arc =
                    of(items[at].first, items[at].count).all(|segment| match segment {
                        Segment::Cubic(cubic) => follows(&cubic, centre, rx, ry, tau),
                        Segment::Line(..) => false,
                    });
                if !follows_arc {
                    return None;
                }
                Corner::Arc { rx, ry }
            }
        };
        placed[quadrant] = Some(kind);
    }
    let [Some(tl), Some(tr), Some(br), Some(bl)] = placed else {
        return None;
    };
    Some(RoundedBox {
        rect,
        corners: [tl, tr, br, bl],
        sign,
    })
}
