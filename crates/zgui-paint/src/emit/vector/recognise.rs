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
pub(crate) fn recognise(path: &BezPath, part: Part<'_>, limits: Limits) -> Option<Decomposition> {
    let tau = limits.tau;
    if !tau.is_finite() || tau <= 0.0 {
        return None;
    }
    let moves = path
        .elements()
        .iter()
        .filter(|element| matches!(element, PathEl::MoveTo(_)))
        .count();
    if moves > limits.max_prims {
        return None;
    }
    let subpaths = subpaths(path, tau)?;
    let mut found = Found::default();
    match part {
        Part::Fill(_) => {
            for subpath in subpaths {
                fill(subpath, tau, &mut found)?;
            }
        }
        Part::Stroke(style) => {
            let width = style.width;
            if !style.dash_pattern.is_empty() || !width.is_finite() || width <= 0.0 {
                return None;
            }
            if !style.miter_limit.is_finite() {
                return None;
            }
            found.half_width = width / 2.0;
            for subpath in subpaths {
                stroke(subpath, style, tau, &mut found)?;
            }
        }
    }
    found.finish(limits.max_prims)
}

/// Whether the device rectangles `rects`, each as `[x0, y0, x1, y1]` and inflated by `margin` on
/// every side, are pairwise disjoint.
///
/// Touching edges count as disjoint. A grid cell crowded past a fixed count answers false, which
/// means "not proven" and keeps the cost linear.
pub(crate) fn separated(rects: &[[f32; 4]], margin: f32) -> bool {
    if rects.len() < 2 {
        return true;
    }
    let inflated: Vec<[f32; 4]> = rects
        .iter()
        .map(|rect| {
            [
                rect[0] - margin,
                rect[1] - margin,
                rect[2] + margin,
                rect[3] + margin,
            ]
        })
        .collect();
    // The cell is as large as the largest rectangle, so each one meets at most four cells.
    let cell = inflated
        .iter()
        .map(|rect| (rect[2] - rect[0]).max(rect[3] - rect[1]))
        .fold(0.0f32, f32::max);
    if !cell.is_finite() || cell <= 0.0 || inflated.iter().flatten().any(|v| !v.is_finite()) {
        return false;
    }
    let mut grid: FxHashMap<(i32, i32), SmallVec<[u32; 4]>> = FxHashMap::default();
    for (index, rect) in inflated.iter().enumerate() {
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

/// One subpath as written, with its degenerate segments dropped.
#[derive(Debug)]
struct Subpath {
    /// The segments.
    segments: Vec<Segment>,
    /// The point its move went to.
    start: Point,
    /// The point it ended at.
    end: Point,
    /// Whether it ended with a close.
    closed: bool,
    /// Whether anything followed its move.
    drawn: bool,
}

/// The subpaths of `path`, or `None` for a quadratic, a non-finite point, a segment before any
/// move, or too many segments in one subpath.
fn subpaths(path: &BezPath, tau: f64) -> Option<Vec<Subpath>> {
    let short = tau / 8.0;
    let finite = |point: Point| point.is_finite().then_some(point);
    let mut out = Vec::new();
    let mut open: Option<Subpath> = None;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                let point = finite(point)?;
                out.extend(open.take());
                open = Some(Subpath {
                    segments: Vec::new(),
                    start: point,
                    end: point,
                    closed: false,
                    drawn: false,
                });
            }
            PathEl::LineTo(point) => {
                let point = finite(point)?;
                let subpath = open.as_mut()?;
                subpath.drawn = true;
                if (point - subpath.end).hypot() >= short {
                    subpath.segments.push(Segment::Line(subpath.end, point));
                    subpath.end = point;
                }
            }
            PathEl::CurveTo(first, second, point) => {
                let [first, second, point] = [finite(first)?, finite(second)?, finite(point)?];
                let subpath = open.as_mut()?;
                subpath.drawn = true;
                let points = [subpath.end, first, second, point];
                if spread(&points) >= short {
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
                let mut subpath = open.take()?;
                subpath.closed = true;
                subpath.drawn = true;
                out.push(subpath);
            }
        }
        if open
            .as_ref()
            .is_some_and(|subpath| subpath.segments.len() > SEGMENTS)
        {
            return None;
        }
    }
    out.extend(open);
    Some(out)
}

/// The largest distance between two of `points`.
fn spread(points: &[Point]) -> f64 {
    let mut largest = 0.0f64;
    for (at, first) in points.iter().enumerate() {
        for second in &points[at + 1..] {
            largest = largest.max((*first - *second).hypot());
        }
    }
    largest
}

/// The segments of `subpath`, closed by a line back to its start when `close` and the gap is
/// longer than `gap`, with collinear lines merged.
fn normalised(subpath: &Subpath, close: bool, gap: f64, tau: f64) -> Vec<Segment> {
    let mut segments = subpath.segments.clone();
    if close && (subpath.start - subpath.end).hypot() >= gap {
        segments.push(Segment::Line(subpath.end, subpath.start));
    }
    merged(segments, close, tau / 8.0)
}

/// `segments` with each run of collinear lines that go one way merged into one line.
///
/// Every point inside a run lies within `tolerance` of the merged line. When `cyclic`, a run may
/// continue from the last segment into the first.
fn merged(segments: Vec<Segment>, cyclic: bool, tolerance: f64) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
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
    out
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
#[derive(Debug, Default)]
struct Found {
    /// Discs and rings.
    discs: Vec<[f64; 4]>,
    /// Boxes.
    boxes: Vec<BoxPrim>,
    /// Capsules.
    capsules: Vec<[f64; 4]>,
    /// Their caps.
    caps: Vec<u8>,
    /// Half the stroke width.
    half_width: f64,
    /// Whether a closed subpath turned each way.
    turned: [bool; 2],
}

impl Found {
    /// Notes the turning sign of one closed subpath.
    fn turned(&mut self, sign: f64) {
        self.turned[usize::from(sign < 0.0)] = true;
    }

    /// The decomposition, or `None` past `max_prims`.
    fn finish(self, max_prims: usize) -> Option<Decomposition> {
        let count = self.discs.len() + self.boxes.len() + self.capsules.len();
        if count > max_prims {
            return None;
        }
        let mut bounds: Vec<[f64; 4]> = Vec::with_capacity(count);
        bounds.extend(self.discs.iter().map(|disc| {
            [
                disc[0] - disc[2],
                disc[1] - disc[2],
                disc[0] + disc[2],
                disc[1] + disc[2],
            ]
        }));
        bounds.extend(self.boxes.iter().map(|prim| prim.rect.map(f64::from)));
        bounds.extend(
            self.capsules
                .iter()
                .zip(&self.caps)
                .map(|(capsule, caps)| capsule_bounds(*capsule, *caps, self.half_width)),
        );
        let ink = bounds
            .iter()
            .copied()
            .reduce(|one, two| {
                [
                    one[0].min(two[0]),
                    one[1].min(two[1]),
                    one[2].max(two[2]),
                    one[3].max(two[3]),
                ]
            })
            .unwrap_or_default();
        let max_extent = bounds
            .iter()
            .map(|rect| (rect[2] - rect[0]).max(rect[3] - rect[1]))
            .fold(0.0f64, f64::max);
        let orientation = match self.turned {
            [_, false] => Orientation::Positive,
            [false, true] => Orientation::Negative,
            [true, true] => Orientation::Mixed,
        };
        let narrow = |values: [f64; 4]| values.map(|value| value as f32);
        Some(Decomposition {
            discs: self.discs.into_iter().map(narrow).collect(),
            boxes: self.boxes,
            capsules: self.capsules.into_iter().map(narrow).collect(),
            caps: self.caps,
            half_width: self.half_width as f32,
            ink: narrow(ink),
            orientation,
            count,
            max_extent: max_extent as f32,
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
fn fill(subpath: Subpath, tau: f64, found: &mut Found) -> Option<()> {
    // A fill closes every subpath. One that already ends within tau of its start is a ring.
    let segments = normalised(&subpath, true, tau, tau);
    if segments.is_empty() || flat(&segments, tau / 4.0) {
        return Some(());
    }
    if let Some(ellipse) = ellipse(&segments, tau) {
        found.turned(ellipse.sign);
        let [cx, cy] = [ellipse.centre.x, ellipse.centre.y];
        if (ellipse.rx - ellipse.ry).abs() <= tau {
            found
                .discs
                .push([cx, cy, (ellipse.rx + ellipse.ry) / 2.0, 0.0]);
        } else {
            let (rx, ry) = (ellipse.rx as f32, ellipse.ry as f32);
            found.boxes.push(BoxPrim {
                rect: [
                    cx - ellipse.rx,
                    cy - ellipse.ry,
                    cx + ellipse.rx,
                    cy + ellipse.ry,
                ]
                .map(|value| value as f32),
                radii: [rx, ry, rx, ry, rx, ry, rx, ry],
                exponent: 2.0,
                border: 0.0,
            });
        }
        return Some(());
    }
    let shape = rounded_box(&segments, tau)?;
    found.turned(shape.sign);
    let mut radii = [0.0f32; 8];
    for (at, corner) in shape.corners.iter().enumerate() {
        if let Corner::Arc { rx, ry } = *corner {
            radii[at * 2] = rx as f32;
            radii[at * 2 + 1] = ry as f32;
        }
    }
    found.boxes.push(BoxPrim {
        rect: shape.rect.map(|value| value as f32),
        radii,
        exponent: 2.0,
        border: 0.0,
    });
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
fn stroke(subpath: Subpath, style: &kurbo::Stroke, tau: f64, found: &mut Found) -> Option<()> {
    let half = style.width / 2.0;
    let segments = normalised(&subpath, subpath.closed, tau / 8.0, tau);
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
                let at = subpath.start;
                found.capsules.push([at.x, at.y, at.x, at.y]);
                found.caps.push(ROUND | (ROUND << 2));
                Some(())
            }
            (kurbo::Cap::Butt, kurbo::Cap::Butt) => Some(()),
            _ => None,
        };
    }
    if !subpath.closed {
        return polyline(&segments, style, tau, found);
    }
    if let Some(ellipse) = ellipse(&segments, tau) {
        let radius = (ellipse.rx + ellipse.ry) / 2.0;
        if (ellipse.rx - ellipse.ry).abs() > tau || radius <= half {
            return None;
        }
        found.turned(ellipse.sign);
        found.discs.push([
            ellipse.centre.x,
            ellipse.centre.y,
            radius + half,
            radius - half,
        ]);
        return Some(());
    }
    let shape = rounded_box(&segments, tau)?;
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
    found.boxes.push(BoxPrim {
        rect: [x0 - half, y0 - half, x1 + half, y1 + half].map(|value| value as f32),
        radii,
        exponent,
        border: style.width as f32,
    });
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
    let lines: Vec<(Point, Point)> = segments
        .iter()
        .map(|segment| match *segment {
            Segment::Line(start, end) => Some((start, end)),
            Segment::Cubic(_) => None,
        })
        .collect::<Option<_>>()?;
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
    let last = lines.len() - 1;
    for (at, line) in lines.iter().enumerate() {
        let start = if at == 0 {
            cap_code(style.start_cap)
        } else {
            inner(lines[at - 1], *line)?
        };
        let end = if at == last {
            cap_code(style.end_cap)
        } else {
            inner(*line, lines[at + 1])?
        };
        found
            .capsules
            .push([line.0.x, line.0.y, line.1.x, line.1.y]);
        found.caps.push(start | (end << 2));
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
    // one full turn.
    let normal = |point: Point| Vec2::new((point.x - centre.x) / rx, (point.y - centre.y) / ry);
    let mut total = 0.0;
    let mut sign = 0.0;
    for cubic in &cubics {
        let (from, to) = (normal(cubic.p0), normal(cubic.p3));
        let angle = from.cross(to).atan2(from.dot(to));
        if angle == 0.0 || angle.abs() >= core::f64::consts::PI {
            return None;
        }
        if sign != 0.0 && angle.signum() != sign {
            return None;
        }
        sign = angle.signum();
        total += angle;
    }
    if (total.abs() - core::f64::consts::TAU).abs() > FULL_TURN {
        return None;
    }
    Some(Ellipse {
        centre,
        rx,
        ry,
        sign,
    })
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
    let error = |point: Point| (normal(point).hypot() - 1.0).abs() * largest;
    if error(cubic.p0) > tau || error(cubic.p3) > tau {
        return false;
    }
    if [0.25, 0.5, 0.75]
        .into_iter()
        .any(|t| error(cubic.eval(t)) > tau / 2.0)
    {
        return false;
    }
    let perpendicular = |point: Point, tangent: Vec2| {
        let radius = normal(point);
        let tangent = Vec2::new(tangent.x / rx, tangent.y / ry);
        let lengths = radius.hypot() * tangent.hypot();
        lengths > 0.0 && (radius.dot(tangent) / lengths).abs() <= PERPENDICULAR
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
    /// The segments it is made of: one line, or up to [`CORNER_CUBICS`] cubics.
    segments: SmallVec<[Segment; 2]>,
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
    let mut items: Vec<Item> = Vec::new();
    for step in 0..count {
        let at = (first + step) % count;
        let segment = segments[at];
        if step > 0
            && !splits(at)
            && let Some(item) = items.last_mut()
        {
            item.segments.push(segment);
            continue;
        }
        items.push(Item {
            enters: Vec2::ZERO,
            leaves: Vec2::ZERO,
            segments: SmallVec::from_elem(segment, 1),
        });
    }
    for item in &mut items {
        match item.segments[0] {
            Segment::Line(start, end) => {
                let axis = axis_of_line(end - start, tau)?;
                item.enters = axis;
                item.leaves = axis;
            }
            Segment::Cubic(first) => {
                if item.segments.len() > CORNER_CUBICS {
                    return None;
                }
                let Segment::Cubic(last) = item.segments[item.segments.len() - 1] else {
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
        let line = matches!(item.segments[0], Segment::Line(..));
        let next_line = matches!(next.segments[0], Segment::Line(..));
        if !line {
            let start = item.segments[0].start();
            let end = item.segments[item.segments.len() - 1].end();
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
            let vertex = item.segments[0].end();
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
                let follows_arc = items[at].segments.iter().all(|segment| match segment {
                    Segment::Cubic(cubic) => follows(cubic, centre, rx, ry, tau),
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
