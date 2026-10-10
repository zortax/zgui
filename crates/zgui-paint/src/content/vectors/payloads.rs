//! Mark payloads kept between frames by the identity of what they were built from.
//!
//! A mark payload uploads once per allocation: the renderer keeps it resident by its address. A
//! shape recognised from a path the recognition cache holds, or a series whose data a scene holds,
//! is lowered to the same payload allocation on every encode while its source lives, so a pan or a
//! zoom of a canvas view uploads no payload. A turn or a stretch of the view places each shape
//! again, which is a new path and a new payload. A series keeps its payload under any view until
//! a far pan measures it from a new centre.
//!
//! A series of [`WORKER_POINTS`] points or more builds a new payload on a worker thread. Until the
//! build ends, the frame draws the nearest payload held for the same data and part: the old centre,
//! or another level of detail. Such a draw is provisional, and the frame that draws it is owed a
//! frame of its own.
//!
//! No picture depends on an entry: a source with none is lowered again, to a new allocation that
//! uploads once more. So a full map drops the entries touched least recently, and when paint
//! records pin every shape entry, it lowers new shapes with no entry until a pin drops.

use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

use rustc_hash::FxHashMap;
use zgui_profile::{Counter, counter};
use zgui_scene::MarkPayload;
use zgui_scene::kurbo::{Affine, Point};

use super::recognitions::evict;
use crate::emit::vector::recognise::Decomposition;

/// How many frames a shape entry survives without a lookup once nothing else holds its payload.
const SHAPE_FRAMES: u32 = 2;

/// The most shape entries held. A full map drops [`EVICTED_SHAPES`] entries no paint record pins,
/// those touched least recently first.
const MAX_SHAPES: usize = 4096;

/// How many shape entries a full map drops at once, so one scan for the oldest serves this many
/// inserts.
const EVICTED_SHAPES: usize = MAX_SHAPES / 16;

/// How many frames a series entry survives without a lookup.
const SERIES_FRAMES: u32 = 600;

/// The most series entries held. A full map drops the entry touched least recently.
const MAX_SERIES: usize = 64;

/// How far from the local origin, in device pixels, an item origin may lie before its payload is
/// measured from a new centre. An `f32` there is exact to 2^16 · 2^-23 = 1/128 of a pixel.
const FAR: f64 = 65_536.0;

/// The fewest points of a series whose new payloads are built on a worker thread. A smaller
/// series builds in well under a millisecond, in the frame that asks.
pub(crate) const WORKER_POINTS: usize = 1 << 16;

/// One part of a series, as its payload is keyed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SeriesPart {
    /// Discs, or squares, of these radii in local units, as bits.
    Disc {
        /// The outer radius or half side.
        outer: u32,
        /// The inner radius or half side.
        inner: u32,
        /// Whether the discs are squares.
        square: bool,
    },
    /// The polyline through the points.
    Line,
    /// The polyline through the first, lowest, highest and last point of each column `2^-bucket`
    /// data units wide.
    Columns {
        /// The base-two logarithm of the columns per data unit.
        bucket: i32,
    },
    /// Copies of one outline, drawn from the cells of this sheet.
    Glyph {
        /// The atlas handle of the sheet, which the payload's table names.
        sheet: u64,
    },
}

/// What a series payload is keyed by: the data's allocation, its length and the part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SeriesKey {
    /// The address of the data.
    pub(crate) data: usize,
    /// How many points the data holds.
    pub(crate) len: usize,
    /// The part.
    pub(crate) part: SeriesPart,
}

impl SeriesKey {
    /// The key of `part` of `data`.
    fn of(data: &Arc<[[f32; 2]]>, part: SeriesPart) -> Self {
        Self {
            data: Arc::as_ptr(data) as *const u8 as usize,
            len: data.len(),
            part,
        }
    }

    /// The key every payload that can stand in for this one shares: a line and its reductions
    /// share one, and every other part has its own.
    fn family(self) -> Self {
        let part = match self.part {
            SeriesPart::Columns { .. } => SeriesPart::Line,
            part => part,
        };
        Self { part, ..self }
    }
}

/// How far a payload of `held` is from one of `wanted`, as a stand-in: lower is nearer. The
/// nearest reduction comes first, a finer one before a coarser one as far away, and the whole line
/// last. The finest reduction stands in for the whole line.
fn distance(wanted: SeriesPart, held: SeriesPart) -> i64 {
    match (wanted, held) {
        (SeriesPart::Columns { bucket }, SeriesPart::Columns { bucket: other }) => {
            2 * (i64::from(other) - i64::from(bucket)).abs() + i64::from(other < bucket)
        }
        (SeriesPart::Line, SeriesPart::Columns { bucket }) => (1 << 40) - i64::from(bucket),
        _ => 1 << 41,
    }
}

/// A series payload being built on a worker thread.
#[derive(Debug)]
struct Build {
    /// What the payload is for.
    key: SeriesKey,
    /// The data, held weakly so the build of dead data is let go.
    data: Weak<[[f32; 2]]>,
    /// The worker, which returns the payload.
    worker: JoinHandle<Option<SeriesPayload>>,
}

/// A series payload found for a frame.
#[derive(Clone, Debug)]
pub(crate) struct SeriesLookup {
    /// The payload.
    pub(crate) payload: SeriesPayload,
    /// Whether it stands in for the payload a build makes. The frame that draws it is owed a
    /// frame.
    pub(crate) provisional: bool,
}

/// One part of a series as payloads.
#[derive(Clone, Debug)]
pub(crate) struct SeriesPayload {
    /// The point every payload position is measured from, in data space.
    pub(crate) centre: [f64; 2],
    /// The bounds of the finite points, as `[x0, y0, x1, y1]` in data space.
    pub(crate) bounds: [f64; 4],
    /// The payloads, one per item, each of at most the most prims an item may hold.
    pub(crate) payloads: Vec<Arc<MarkPayload>>,
}

/// One series' payload.
#[derive(Debug)]
struct SeriesEntry {
    /// The data, held so no other data takes its address.
    data: Weak<[[f32; 2]]>,
    /// The payload.
    payload: SeriesPayload,
    /// The frame it was last looked up in.
    touched: u32,
}

/// What one scan of a series' data found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DataFacts {
    /// Whether x never decreases over the finite points.
    pub(crate) monotone: bool,
    /// How many points are finite.
    pub(crate) finite: usize,
    /// The least and the greatest finite x.
    pub(crate) x: [f64; 2],
}

impl DataFacts {
    /// Scans `data`.
    pub(crate) fn of(data: &[[f32; 2]]) -> Self {
        let mut facts = Self {
            monotone: true,
            finite: 0,
            x: [f64::INFINITY, f64::NEG_INFINITY],
        };
        let mut last = f32::NEG_INFINITY;
        for &[x, y] in data {
            if !(x.is_finite() && y.is_finite()) {
                continue;
            }
            facts.monotone &= x >= last;
            last = x;
            facts.finite += 1;
            facts.x = [facts.x[0].min(f64::from(x)), facts.x[1].max(f64::from(x))];
        }
        facts
    }
}

/// The facts of one data allocation.
#[derive(Debug)]
struct FactsEntry {
    /// The data, held so no other data takes its address.
    data: Weak<[[f32; 2]]>,
    facts: DataFacts,
    /// The frame it was last looked up in.
    touched: u32,
}

/// One shape's payload.
#[derive(Debug)]
struct ShapeEntry {
    /// The recognition it was lowered from. Held, so the recognition cache keeps it while a
    /// drawing on the screen draws this payload.
    found: Arc<Decomposition>,
    /// The payload.
    payload: Arc<MarkPayload>,
    /// The [`zgui_scene::MarkFlags`] the lowering found.
    flags: u32,
    /// The frame it was last looked up in.
    touched: u32,
}

/// Mark payloads by the identity of their source.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct MarkPayloads {
    /// Shape payloads, by the address of the recognition and whether the item is a union.
    shapes: FxHashMap<(usize, bool), ShapeEntry>,
    /// Series payloads.
    series: FxHashMap<SeriesKey, SeriesEntry>,
    /// What each series' data is, by its address and length.
    facts: FxHashMap<(usize, usize), FactsEntry>,
    /// The series payloads being built on worker threads, at most one by family.
    builds: FxHashMap<SeriesKey, Build>,
    /// The current frame.
    frame: u32,
    /// Whether the shape map was full with every entry pinned this frame.
    all_pinned: bool,
}

impl ShapeEntry {
    /// Whether a paint record holds the payload: a drawing that may encode again draws it.
    fn pinned(&self) -> bool {
        Arc::strong_count(&self.payload) > 1
    }
}

impl MarkPayloads {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Ends a frame: drops the entries whose source died or that no frame asked for lately.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.all_pinned = false;
        // A payload a paint record holds belongs to a drawing that may encode again, as a canvas
        // panned after a pause does.
        self.shapes
            .retain(|_, entry| entry.pinned() || frame.wrapping_sub(entry.touched) < SHAPE_FRAMES);
        self.series.retain(|_, entry| {
            entry.data.strong_count() > 0 && frame.wrapping_sub(entry.touched) < SERIES_FRAMES
        });
        self.facts.retain(|_, entry| {
            entry.data.strong_count() > 0 && frame.wrapping_sub(entry.touched) < SERIES_FRAMES
        });
        // A build whose data died is let go: its worker ends on its own.
        self.builds.retain(|_, build| build.data.strong_count() > 0);
    }

    /// Forgets everything. A running build is let go.
    pub(crate) fn clear(&mut self) {
        self.shapes.clear();
        self.series.clear();
        self.facts.clear();
        self.builds.clear();
    }

    /// Waits for every series build and keeps what it built.
    #[doc(hidden)]
    pub fn settle(&mut self) {
        let builds: Vec<Build> = self.builds.drain().map(|(_, build)| build).collect();
        for build in builds {
            self.finish(build);
        }
    }

    /// Whether a series build runs.
    #[doc(hidden)]
    pub fn building(&self) -> bool {
        !self.builds.is_empty()
    }

    /// Keeps the payload of the build of `family`, if the build ended.
    fn adopt(&mut self, family: SeriesKey) {
        if !self
            .builds
            .get(&family)
            .is_some_and(|build| build.worker.is_finished())
        {
            return;
        }
        if let Some(build) = self.builds.remove(&family) {
            self.finish(build);
        }
    }

    /// Waits for `build` and keeps its payload while its data lives.
    fn finish(&mut self, build: Build) {
        // A worker that panicked built nothing; a later frame asks again.
        let Ok(Some(payload)) = build.worker.join() else {
            return;
        };
        let Some(data) = build.data.upgrade() else {
            return;
        };
        counter::bump(Counter::SeriesPayloadsBuilt);
        self.insert_series(build.key, &data, payload);
    }

    /// Starts a build of `request` on a worker, unless one of its family runs. Builds it here when
    /// no thread starts.
    fn request(&mut self, data: &Arc<[[f32; 2]]>, request: Request) {
        let key = SeriesKey::of(data, request.part);
        let family = key.family();
        if self.builds.contains_key(&family) {
            return;
        }
        let held = Arc::clone(data);
        let table = request.table.to_vec();
        let Request {
            part,
            to_local,
            far,
            bounds,
            max_prims,
            ..
        } = request;
        let spawned = std::thread::Builder::new()
            .name("zgui-series".into())
            .spawn(move || {
                let request = Request {
                    part,
                    to_local,
                    far,
                    bounds,
                    max_prims,
                    table: &table,
                };
                build(&held, request)
            });
        match spawned {
            Ok(worker) => {
                counter::bump(Counter::SeriesBuildsAsync);
                self.builds.insert(
                    family,
                    Build {
                        key,
                        data: Arc::downgrade(data),
                        worker,
                    },
                );
            }
            Err(_) => {
                if let Some(payload) = build(data, request) {
                    counter::bump(Counter::SeriesPayloadsBuilt);
                    self.insert_series(key, data, payload);
                }
            }
        }
    }

    /// The held payload of `data` nearest to the one `key` names, among those of its family.
    fn nearest(&mut self, key: SeriesKey, data: &Arc<[[f32; 2]]>) -> Option<SeriesPayload> {
        let family = key.family();
        let (nearest, _) = self
            .series
            .iter()
            .filter(|(held, entry)| {
                held.family() == family
                    && **held != key
                    && entry
                        .data
                        .upgrade()
                        .is_some_and(|alive| Arc::ptr_eq(&alive, data))
            })
            .map(|(held, _)| (*held, distance(key.part, held.part)))
            .filter(|(_, distance)| *distance < 1 << 41)
            .min_by_key(|(_, distance)| *distance)?;
        self.series(nearest, data)
    }

    /// The facts of `data`, scanned once per allocation.
    fn facts(&mut self, data: &Arc<[[f32; 2]]>) -> DataFacts {
        let key = (Arc::as_ptr(data) as *const u8 as usize, data.len());
        let frame = self.frame;
        if let Some(entry) = self.facts.get_mut(&key)
            && entry
                .data
                .upgrade()
                .is_some_and(|held| Arc::ptr_eq(&held, data))
        {
            entry.touched = frame;
            return entry.facts;
        }
        let facts = DataFacts::of(data);
        if self.facts.len() >= MAX_SERIES && !self.facts.contains_key(&key) {
            let oldest = self
                .facts
                .iter()
                .max_by_key(|(_, entry)| frame.wrapping_sub(entry.touched))
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.facts.remove(&oldest);
            }
        }
        self.facts.insert(
            key,
            FactsEntry {
                data: Arc::downgrade(data),
                facts,
                touched: frame,
            },
        );
        facts
    }

    /// Whether a reduced payload of `data` at `bucket` is held.
    fn holds_columns(&self, data: &Arc<[[f32; 2]]>, bucket: i32) -> bool {
        self.series.contains_key(&SeriesKey {
            data: Arc::as_ptr(data) as *const u8 as usize,
            len: data.len(),
            part: SeriesPart::Columns { bucket },
        })
    }

    /// The payload and flags lowered from `found` before, if `found` is still that recognition.
    pub(crate) fn shape(
        &mut self,
        found: &Arc<Decomposition>,
        union: bool,
    ) -> Option<(Arc<MarkPayload>, u32)> {
        let frame = self.frame;
        let entry = self.shapes.get_mut(&(Arc::as_ptr(found) as usize, union))?;
        if !Arc::ptr_eq(&entry.found, found) {
            return None;
        }
        entry.touched = frame;
        Some((Arc::clone(&entry.payload), entry.flags))
    }

    /// Keeps the payload and flags lowered from `found`. A full map whose every entry is pinned
    /// keeps nothing.
    pub(crate) fn insert_shape(
        &mut self,
        found: &Arc<Decomposition>,
        union: bool,
        payload: Arc<MarkPayload>,
        flags: u32,
    ) {
        let key = (Arc::as_ptr(found) as usize, union);
        if self.shapes.len() >= MAX_SHAPES && !self.shapes.contains_key(&key) {
            self.all_pinned = self.all_pinned
                || !evict(&mut self.shapes, self.frame, EVICTED_SHAPES, |entry| {
                    (entry.touched, entry.pinned())
                });
            if self.all_pinned {
                counter::bump(Counter::MarksUncached);
                return;
            }
        }
        self.shapes.insert(
            key,
            ShapeEntry {
                found: Arc::clone(found),
                payload,
                flags,
                touched: self.frame,
            },
        );
    }

    /// The payload built for `key` from `data`, if `data` is still that allocation.
    fn series(&mut self, key: SeriesKey, data: &Arc<[[f32; 2]]>) -> Option<SeriesPayload> {
        let frame = self.frame;
        let entry = self.series.get_mut(&key)?;
        let held = entry.data.upgrade()?;
        if !Arc::ptr_eq(&held, data) {
            return None;
        }
        entry.touched = frame;
        Some(entry.payload.clone())
    }

    /// Keeps the payload built for `key` from `data`, in place of any held for it. A full map
    /// drops the entry touched least recently.
    fn insert_series(&mut self, key: SeriesKey, data: &Arc<[[f32; 2]]>, payload: SeriesPayload) {
        if self.series.len() >= MAX_SERIES && !self.series.contains_key(&key) {
            let frame = self.frame;
            let oldest = self
                .series
                .iter()
                .max_by_key(|(_, entry)| frame.wrapping_sub(entry.touched))
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.series.remove(&oldest);
            }
        }
        self.series.insert(
            key,
            SeriesEntry {
                data: Arc::downgrade(data),
                payload,
                touched: self.frame,
            },
        );
    }
}

/// What one series payload is built from, besides the data.
#[derive(Clone, Copy, Debug)]
struct Request<'a> {
    /// The part.
    part: SeriesPart,
    /// The data to the fragment's space, at the frame that asked.
    to_local: Affine,
    /// How far from the local origin an item origin may lie, in local units.
    far: f64,
    /// The bounds of the finite points, when known.
    bounds: Option<[f64; 4]>,
    /// The most prims a payload holds.
    max_prims: usize,
    /// The tile table a glyph payload starts with.
    table: &'a [[u32; 4]],
}

/// The payloads of `part` of the series over `data`, which `to_local` maps to the fragment's
/// space, or `None` for data with no finite point.
///
/// Built once per data allocation, and held in `cache`. A payload is measured from the centre of
/// the data's bounds, or from the point `to_local` takes to the origin when that centre lands more
/// than half of [`FAR`] device pixels from it. `device` is how many device pixels a local unit
/// spans. A payload whose centre lands farther than [`FAR`] device pixels is measured again, which
/// keeps every position within 1/128 of a pixel.
///
/// A series of [`WORKER_POINTS`] points or more builds a new payload on a worker, and draws the
/// nearest held payload of its family meanwhile as a provisional one. It starts the build as its
/// centre passes half of [`FAR`], so a steady pan seldom draws a provisional payload. Only a series
/// with no payload of its family held builds in the frame.
///
/// Each payload holds at most `max_prims` prims. A glyph payload starts with `table`, the tile
/// table of its sheet.
pub(crate) fn series_payload(
    cache: Option<&mut MarkPayloads>,
    data: &Arc<[[f32; 2]]>,
    part: SeriesPart,
    to_local: Affine,
    device: f64,
    max_prims: usize,
    table: &[[u32; 4]],
) -> Option<SeriesLookup> {
    let far = FAR / device.clamp(1e-3, 1e3);
    let off = |payload: &SeriesPayload| {
        let origin = to_local * Point::new(payload.centre[0], payload.centre[1]);
        origin.x.abs().max(origin.y.abs())
    };
    let exact = |payload: SeriesPayload| SeriesLookup {
        payload,
        provisional: false,
    };
    let mut request = Request {
        part,
        to_local,
        far,
        bounds: None,
        max_prims,
        table,
    };
    let Some(cache) = cache else {
        return build(data, request).map(exact);
    };
    let key = SeriesKey::of(data, part);
    cache.adopt(key.family());
    let held = cache.series(key, data);
    let worker = data.len() >= WORKER_POINTS;
    if let Some(payload) = &held {
        let off = off(payload);
        request.bounds = Some(payload.bounds);
        if off <= far / 2.0 || (off <= far && !worker) {
            return held.map(exact);
        }
        if off <= far {
            cache.request(data, request);
            return held.map(exact);
        }
    }
    if worker {
        let standing = held.or_else(|| cache.nearest(key, data));
        if let Some(payload) = standing {
            request.bounds = Some(payload.bounds);
            cache.request(data, request);
            counter::bump(Counter::SeriesDrawsProvisional);
            return Some(SeriesLookup {
                payload,
                provisional: true,
            });
        }
    }
    let payload = build(data, request)?;
    counter::bump(Counter::SeriesPayloadsBuilt);
    cache.insert_series(key, data, payload.clone());
    Some(exact(payload))
}

/// The most points per column of the reduction a line is drawn with whole.
const POINTS_PER_COLUMN: f64 = 4.0;

/// How many columns of the reduction a device column holds at least, as a power of two.
///
/// The columns start at data x = 0 and the device columns start where the view puts them, so a
/// device column shares the columns at its two edges with its neighbours. The reduction keeps the
/// extremes of a shared column, which can lie in the neighbour. With two columns a device column,
/// what the device column can lose is within half a pixel of its edges. Narrower columns keep more
/// points, and on smooth dense data each point adds to the overlap the marks route sums.
const SUB_COLUMNS_LOG2: i32 = 1;

/// The column bucket for a device that draws `per_unit` columns per data unit: columns at most
/// half a device pixel wide.
fn bucket_of(per_unit: f64) -> Option<i32> {
    if !(per_unit.is_finite() && per_unit > 0.0) {
        return None;
    }
    let need = per_unit.log2().ceil();
    if !(-1000.0..=1000.0).contains(&need) {
        return None;
    }
    Some(need as i32 + SUB_COLUMNS_LOG2)
}

/// The column bucket a line over `data` is reduced at, when the device draws `per_unit` columns per
/// data unit, or `None` when it is drawn whole.
///
/// The data must run left to right and hold more than [`POINTS_PER_COLUMN`] finite points per
/// column of the reduction over its x range. The bucket is `ceil(log2(per_unit)) + 1`, so a column
/// is never wider than half a device pixel. A payload already built one bucket finer is
/// kept, so a zoom builds again only when it passes twice or half the scale it was built for.
pub(crate) fn lod_bucket(
    cache: Option<&mut MarkPayloads>,
    data: &Arc<[[f32; 2]]>,
    per_unit: f64,
) -> Option<i32> {
    let bucket = bucket_of(per_unit)?;
    let (facts, finer) = match cache {
        Some(cache) => (cache.facts(data), cache.holds_columns(data, bucket + 1)),
        None => (DataFacts::of(data), false),
    };
    if !facts.monotone || facts.finite == 0 {
        return None;
    }
    let columns =
        ((facts.x[1] - facts.x[0]) * per_unit).max(1.0) * f64::from(1 << SUB_COLUMNS_LOG2);
    if facts.finite as f64 / columns <= POINTS_PER_COLUMN {
        return None;
    }
    Some(if finer { bucket + 1 } else { bucket })
}

/// `data` reduced to the first, lowest, highest and last point of each column `2^-bucket` data
/// units wide, from data x = 0, in data order. A non-finite point ends a run, and is kept as its
/// end.
pub(crate) fn m4(data: &[[f32; 2]], bucket: i32) -> Vec<[f32; 2]> {
    let scale = 2f64.powi(bucket);
    let mut out = Vec::new();
    // The column and the indices of its first, lowest, highest and last point.
    let mut open: Option<(i64, [usize; 4])> = None;
    let flush = |out: &mut Vec<[f32; 2]>, open: &mut Option<(i64, [usize; 4])>| {
        if let Some((_, mut picked)) = open.take() {
            picked.sort_unstable();
            let mut last = usize::MAX;
            for index in picked {
                if index != last {
                    out.push(data[index]);
                    last = index;
                }
            }
        }
    };
    for (index, &[x, y]) in data.iter().enumerate() {
        if !(x.is_finite() && y.is_finite()) {
            flush(&mut out, &mut open);
            out.push([x, y]);
            continue;
        }
        let column = (f64::from(x) * scale).floor() as i64;
        match &mut open {
            Some((held, picked)) if *held == column => {
                if y < data[picked[1]][1] {
                    picked[1] = index;
                }
                if y > data[picked[2]][1] {
                    picked[2] = index;
                }
                picked[3] = index;
            }
            _ => {
                flush(&mut out, &mut open);
                open = Some((column, [index; 4]));
            }
        }
    }
    flush(&mut out, &mut open);
    out
}

/// The bounds of the finite points of `data`, or `None` when it holds none.
fn bounds_of(data: &[[f32; 2]]) -> Option<[f64; 4]> {
    let mut bounds: Option<[f64; 4]> = None;
    for &[x, y] in data {
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let (x, y) = (f64::from(x), f64::from(y));
        bounds = Some(match bounds {
            None => [x, y, x, y],
            Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
        });
    }
    bounds
}

/// The point a payload of data within `bounds` is measured from: the centre of the bounds, or the
/// point `to_local` takes to the origin when the centre lands more than half of `far` from it.
fn centre_of(bounds: [f64; 4], to_local: Affine, far: f64) -> [f64; 2] {
    let centre = Point::new((bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0);
    let origin = to_local * centre;
    if origin.x.abs().max(origin.y.abs()) <= far / 2.0 {
        return [centre.x, centre.y];
    }
    let centre = to_local.inverse() * Point::ORIGIN;
    [centre.x, centre.y]
}

/// The payloads `request` asks for, of `data`, or `None` for data with no finite point. A glyph
/// payload starts with the table.
fn build(data: &[[f32; 2]], request: Request<'_>) -> Option<SeriesPayload> {
    let Request {
        part,
        to_local,
        far,
        bounds,
        max_prims,
        table,
    } = request;
    let bounds = match bounds {
        Some(bounds) => bounds,
        None => bounds_of(data)?,
    };
    let centre = centre_of(bounds, to_local, far);
    let reduced;
    let data = match part {
        SeriesPart::Columns { bucket } => {
            reduced = m4(data, bucket);
            &reduced[..]
        }
        _ => data,
    };
    let at = |[x, y]: [f32; 2]| {
        [
            (f64::from(x) - centre[0]) as f32,
            (f64::from(y) - centre[1]) as f32,
        ]
    };
    let max_prims = max_prims.max(2);
    let mut payloads = Vec::new();
    match part {
        SeriesPart::Disc { outer, inner, .. } => {
            let (outer, inner) = (f32::from_bits(outer), f32::from_bits(inner));
            let discs: Vec<[f32; 4]> = data
                .iter()
                .filter(|[x, y]| x.is_finite() && y.is_finite())
                .map(|&point| {
                    let [x, y] = at(point);
                    [x, y, outer, inner]
                })
                .collect();
            for chunk in discs.chunks(max_prims) {
                payloads.push(Arc::new(MarkPayload {
                    discs: chunk.to_vec(),
                    ..MarkPayload::default()
                }));
            }
        }
        SeriesPart::Glyph { .. } => {
            let anchors: Vec<[f32; 2]> = data
                .iter()
                .filter(|[x, y]| x.is_finite() && y.is_finite())
                .map(|&point| at(point))
                .collect();
            let lead = table.len() as u32;
            for chunk in anchors.chunks(max_prims) {
                let mut glyphs = Vec::with_capacity(table.len() + chunk.len());
                glyphs.extend_from_slice(table);
                for (index, [x, y]) in chunk.iter().enumerate() {
                    glyphs.push([x.to_bits(), y.to_bits(), 0, lead + index as u32]);
                }
                payloads.push(Arc::new(MarkPayload {
                    glyphs,
                    ..MarkPayload::default()
                }));
            }
        }
        SeriesPart::Line | SeriesPart::Columns { .. } => {
            let separator = [f32::NAN, f32::NAN];
            let mut vertices = vec![separator];
            let mut run = 0usize;
            let close = |vertices: &mut Vec<[f32; 2]>, run: &mut usize| {
                // A run of one point is a dot: the point written twice.
                if *run == 1 {
                    let last = vertices[vertices.len() - 1];
                    vertices.push(last);
                }
                if *run > 0 {
                    vertices.push(separator);
                }
                *run = 0;
            };
            for &point in data {
                if !(point[0].is_finite() && point[1].is_finite()) {
                    close(&mut vertices, &mut run);
                    continue;
                }
                // A payload past the limit ends here; the next one starts at the same point,
                // so the line runs on.
                if vertices.len() + 2 > max_prims && run > 0 {
                    let last = vertices[vertices.len() - 1];
                    close(&mut vertices, &mut run);
                    payloads.push(Arc::new(MarkPayload {
                        vertices: core::mem::replace(&mut vertices, vec![separator, last]),
                        ..MarkPayload::default()
                    }));
                    run = 1;
                }
                vertices.push(at(point));
                run += 1;
            }
            close(&mut vertices, &mut run);
            if vertices.len() > 1 {
                payloads.push(Arc::new(MarkPayload {
                    vertices,
                    ..MarkPayload::default()
                }));
            }
        }
    }
    Some(SeriesPayload {
        centre,
        bounds,
        payloads,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_canvas::{Brush, CanvasScene};
    use zgui_color::Color;
    use zgui_geom::Size;
    use zgui_profile::Counter;
    use zgui_scene::kurbo::{Affine, BezPath, Circle, Shape as _};
    use zgui_scene::{ClipId, MarkPayload, Scene, SpatialId, VectorId};

    use super::{
        EVICTED_SHAPES, MAX_SHAPES, MarkPayloads, SeriesLookup, SeriesPart, WORKER_POINTS,
        lod_bucket, m4, series_payload,
    };
    use crate::content::Drawing;
    use crate::content::vectors::recognitions::MAX_ENTRIES;
    use crate::content::vectors::{CachedMarks, PartKey};
    use crate::emit::vector::recognise::{Decomposition, Orientation};
    use crate::emit::vector::{ShapePaint, VectorPlacement, draw_drawing};

    /// A disc part of radius 3.
    const DISC: SeriesPart = SeriesPart::Disc {
        outer: 0x4040_0000,
        inner: 0,
        square: false,
    };

    /// Points over x in 1000..1010 and y in -4..4, with a NaN among them.
    fn data() -> Arc<[[f32; 2]]> {
        (0..=10)
            .map(|i| {
                if i == 5 {
                    [f32::NAN, 0.0]
                } else {
                    [1000.0 + i as f32, (i as f32 - 5.0) * 0.8]
                }
            })
            .collect()
    }

    #[test]
    fn a_series_payload_is_built_once_per_data() {
        let mut cache = MarkPayloads::default();
        cache.begin_frame();
        let data = data();
        let first = series_payload(
            Some(&mut cache),
            &data,
            DISC,
            Affine::IDENTITY,
            1.0,
            1 << 22,
            &[],
        )
        .expect("finite points")
        .payload;
        assert_eq!(
            first.payloads[0].discs.len(),
            10,
            "the NaN point is skipped"
        );
        let pan = Affine::translate((-40.0, 7.0)) * Affine::scale(3.0);
        let second = series_payload(Some(&mut cache), &data, DISC, pan, 1.0, 1 << 22, &[])
            .expect("finite points")
            .payload;
        assert!(Arc::ptr_eq(&first.payloads[0], &second.payloads[0]));
        let other: Arc<[[f32; 2]]> = data.iter().copied().collect();
        let third = series_payload(Some(&mut cache), &other, DISC, pan, 1.0, 1 << 22, &[])
            .expect("points")
            .payload;
        assert!(
            !Arc::ptr_eq(&first.payloads[0], &third.payloads[0]),
            "equal data in another allocation is another payload"
        );
    }

    #[test]
    fn a_series_payload_is_rebased_on_its_data() {
        let data = data();
        let built = series_payload(None, &data, DISC, Affine::IDENTITY, 1.0, 1 << 22, &[])
            .expect("points")
            .payload;
        assert_eq!(built.bounds, [1000.0, -4.0, 1010.0, 4.0]);
        assert_eq!(built.centre, [1005.0, 0.0]);
        for &[x, y, outer, _] in &built.payloads[0].discs {
            assert!(
                x.abs() <= 5.0 && y.abs() <= 4.0,
                "({x}, {y}) is within the half extent"
            );
            assert_eq!(outer, 3.0);
        }
        let line = series_payload(
            None,
            &data,
            SeriesPart::Line,
            Affine::IDENTITY,
            1.0,
            1 << 22,
            &[],
        )
        .expect("points")
        .payload;
        let vertices = &line.payloads[0].vertices;
        assert!(vertices[0][0].is_nan() && vertices[vertices.len() - 1][0].is_nan());
        assert_eq!(vertices.len(), 13, "two runs of five, three separators");
    }

    #[test]
    fn a_far_pan_rebases_the_payload() {
        let mut cache = MarkPayloads::default();
        cache.begin_frame();
        let data = data();
        let near = series_payload(
            Some(&mut cache),
            &data,
            DISC,
            Affine::translate((-1000.0, 0.0)),
            1.0,
            1 << 22,
            &[],
        )
        .expect("points")
        .payload;
        assert_eq!(near.centre, [1005.0, 0.0]);
        let far = Affine::translate((70_000.0, 0.0));
        let moved = series_payload(Some(&mut cache), &data, DISC, far, 1.0, 1 << 22, &[])
            .expect("points")
            .payload;
        assert_eq!(
            moved.centre,
            [-70_000.0, 0.0],
            "the centre is where the origin lands"
        );
        assert!(!Arc::ptr_eq(&near.payloads[0], &moved.payloads[0]));
        let again = series_payload(Some(&mut cache), &data, DISC, far, 1.0, 1 << 22, &[])
            .expect("points")
            .payload;
        assert!(
            Arc::ptr_eq(&moved.payloads[0], &again.payloads[0]),
            "the rebased payload replaces the entry"
        );
    }

    #[test]
    fn m4_keeps_first_lowest_highest_last_per_column() {
        // Two columns of width one half, a NaN, and a third column.
        let data = [
            [0.0, 1.0],
            [0.1, -3.0],
            [0.2, 0.5],
            [0.3, 4.0],
            [0.4, 2.0],
            [0.6, 0.0],
            [0.7, 0.0],
            [f32::NAN, f32::NAN],
            [1.1, 5.0],
            [1.2, 6.0],
            [1.3, 7.0],
            [1.4, 8.0],
            [1.45, 7.5],
        ];
        let reduced = m4(&data, 1);
        let expected = [
            [0.0, 1.0],
            [0.1, -3.0],
            [0.3, 4.0],
            [0.4, 2.0],
            [0.6, 0.0],
            [0.7, 0.0],
        ];
        assert_eq!(&reduced[..6], &expected, "first, lowest, highest, last");
        assert!(reduced[6][0].is_nan(), "the run ends where it ended");
        assert_eq!(
            &reduced[7..],
            &[[1.1, 5.0], [1.4, 8.0], [1.45, 7.5]],
            "the first is the lowest here"
        );
    }

    /// `count` points of a zigzag over x in 0..1, left to right.
    fn dense(count: usize) -> Arc<[[f32; 2]]> {
        (0..count)
            .map(|i| {
                let x = i as f32 / count as f32;
                [x, if i % 2 == 0 { 0.0 } else { 1.0 } + x]
            })
            .collect()
    }

    #[test]
    fn a_line_that_turns_back_is_not_reduced() {
        let mut data: Vec<[f32; 2]> = dense(10_000).to_vec();
        data.swap(10, 20);
        let data: Arc<[[f32; 2]]> = data.into();
        assert_eq!(lod_bucket(None, &data, 100.0), None);
    }

    #[test]
    fn a_sparse_line_is_not_reduced() {
        let data = dense(390);
        assert_eq!(
            lod_bucket(None, &data, 50.0),
            None,
            "fewer than eight points a device column, four a column of the reduction"
        );
        assert_eq!(
            lod_bucket(None, &data, 25.0),
            Some(6),
            "sixteen points a device column, columns of a sixty-fourth of a unit"
        );
    }

    #[test]
    fn a_device_column_keeps_its_extremes_off_the_grid() {
        // One device column a unit, its edge at x = 0.5. The highest point right of the edge is
        // not the highest of its unit.
        let data = [[0.1, 0.0], [0.3, 10.0], [0.6, 8.0], [0.9, 0.0]];
        let bucket = super::bucket_of(1.0).expect("a bucket");
        assert_eq!(bucket, 1, "half a device column");
        assert!(
            m4(&data, bucket).contains(&[0.6, 8.0]),
            "the extreme of the device column right of the edge"
        );
        assert!(
            !m4(&data, 0).contains(&[0.6, 8.0]),
            "a whole column loses it"
        );
    }

    /// Draws `canvas` once through `source` and returns how many payloads were built.
    fn built(canvas: &CanvasScene, source: &CachedMarks) -> u64 {
        let before = zgui_profile::counter::get(Counter::SeriesPayloadsBuilt);
        let drawing = Drawing::canvas(canvas, Affine::IDENTITY);
        draw(&drawing, source);
        source.end_frame();
        zgui_profile::counter::get(Counter::SeriesPayloadsBuilt) - before
    }

    #[test]
    fn a_pan_and_a_small_zoom_keep_the_reduced_payload() {
        let _turn = zgui_profile::counter::exclusive();
        let data = dense(10_000);
        let mut canvas = CanvasScene::default();
        canvas.push_series_lod(
            zgui_canvas::Series::Line {
                data: Arc::clone(&data),
                to_canvas: Affine::scale_non_uniform(100.0, 20.0),
                stroke: zgui_scene::kurbo::Stroke::new(1.0),
                brush: Brush::Solid(Color::WHITE),
            },
            zgui_canvas::Lod::Columns,
        );
        let source = CachedMarks::new();
        assert_eq!(built(&canvas, &source), 1);
        let reduced = {
            let drawing = Drawing::canvas(&canvas, Affine::IDENTITY);
            draw(&drawing, &source).vertices.len()
        };
        assert!(
            reduced <= 4 * 257 + 2,
            "four points a column at most: {reduced}"
        );
        // 100 device columns a unit needs 256 columns; 120 too; 140 needs 512; back to 100 keeps
        // the finer one; 40 needs 128 and keeps 256, which is twice as fine.
        let views = [
            (Affine::translate((13.0, 0.0)), 0),
            (Affine::scale(1.2), 0),
            (Affine::scale(1.4), 1),
            (Affine::IDENTITY, 0),
            (Affine::scale(0.4), 0),
            (Affine::scale(0.2), 1),
        ];
        for (view, expected) in views {
            canvas.set_transform(view);
            assert_eq!(built(&canvas, &source), expected, "{view:?}");
        }
    }

    /// Looks up `part` of `data` under `view` at one device pixel a unit.
    fn look(
        cache: &mut MarkPayloads,
        data: &Arc<[[f32; 2]]>,
        part: SeriesPart,
        view: Affine,
    ) -> SeriesLookup {
        series_payload(Some(cache), data, part, view, 1.0, 1 << 22, &[]).expect("points")
    }

    #[test]
    fn a_held_payload_stands_in_while_a_worker_builds() {
        let _turn = zgui_profile::counter::exclusive();
        let (started, standing) = (
            zgui_profile::counter::get(Counter::SeriesBuildsAsync),
            zgui_profile::counter::get(Counter::SeriesDrawsProvisional),
        );
        let mut cache = MarkPayloads::default();
        cache.begin_frame();
        let data = dense(WORKER_POINTS + 4);

        // The first payload of a part is built in the frame.
        let first = look(&mut cache, &data, DISC, Affine::IDENTITY);
        assert!(!first.provisional && !cache.building());
        // A far pan draws it while a worker measures it from a new centre.
        let far = Affine::translate((200_000.0, 0.0));
        let held = look(&mut cache, &data, DISC, far);
        assert!(held.provisional && cache.building());
        assert!(Arc::ptr_eq(
            &held.payload.payloads[0],
            &first.payload.payloads[0]
        ));
        cache.settle();
        let built = look(&mut cache, &data, DISC, far);
        assert!(!built.provisional);
        assert_eq!(built.payload.centre, [-200_000.0, 0.0]);

        // A reduction stands in for another one.
        let coarse = look(
            &mut cache,
            &data,
            SeriesPart::Columns { bucket: 8 },
            Affine::IDENTITY,
        );
        assert!(!coarse.provisional);
        let finer = SeriesPart::Columns { bucket: 10 };
        let held = look(&mut cache, &data, finer, Affine::IDENTITY);
        assert!(held.provisional);
        assert!(Arc::ptr_eq(
            &held.payload.payloads[0],
            &coarse.payload.payloads[0]
        ));
        cache.settle();
        let built = look(&mut cache, &data, finer, Affine::IDENTITY);
        assert!(!built.provisional);
        assert!(
            built.payload.payloads[0].vertices.len() > coarse.payload.payloads[0].vertices.len()
        );
        if zgui_profile::COUNTERS_ENABLED {
            assert_eq!(
                zgui_profile::counter::get(Counter::SeriesBuildsAsync) - started,
                2
            );
            assert_eq!(
                zgui_profile::counter::get(Counter::SeriesDrawsProvisional) - standing,
                2
            );
        }
    }

    #[test]
    fn a_pan_past_half_the_limit_builds_ahead_and_draws_exact() {
        // Its build moves the counters another test reads.
        let _turn = zgui_profile::counter::exclusive();
        let mut cache = MarkPayloads::default();
        cache.begin_frame();
        let data = dense(WORKER_POINTS + 4);
        let first = look(&mut cache, &data, DISC, Affine::IDENTITY);
        // Past half the limit and within it: the held payload is still exact.
        let pan = Affine::translate((40_000.0, 0.0));
        let held = look(&mut cache, &data, DISC, pan);
        assert!(!held.provisional);
        assert!(Arc::ptr_eq(
            &held.payload.payloads[0],
            &first.payload.payloads[0]
        ));
        assert!(cache.building(), "the next centre is built ahead");
        cache.settle();
        let built = look(&mut cache, &data, DISC, pan);
        assert!(!built.provisional);
        assert_eq!(built.payload.centre, [-40_000.0, 0.0]);
    }

    #[test]
    fn a_series_entry_dies_with_its_data() {
        let mut cache = MarkPayloads::default();
        cache.begin_frame();
        let data = data();
        series_payload(
            Some(&mut cache),
            &data,
            DISC,
            Affine::IDENTITY,
            1.0,
            1 << 22,
            &[],
        )
        .expect("points");
        cache.end_frame();
        cache.begin_frame();
        assert_eq!(cache.series.len(), 1);
        drop(data);
        cache.end_frame();
        assert!(cache.series.is_empty());
    }

    /// One disc.
    fn found() -> Arc<Decomposition> {
        Arc::new(Decomposition {
            discs: vec![[4.0, 4.0, 2.0, 0.0]],
            boxes: Vec::new(),
            capsules: Vec::new(),
            caps: Vec::new(),
            half_width: 0.0,
            ink: [2.0, 2.0, 6.0, 6.0],
            orientation: Orientation::Positive,
            count: 1,
            max_extent: 4.0,
        })
    }

    #[test]
    fn a_shape_payload_is_reused_by_its_recognition() {
        let mut payloads = MarkPayloads::default();
        payloads.begin_frame();
        let found = found();
        let payload = Arc::new(MarkPayload::default());
        payloads.insert_shape(&found, true, Arc::clone(&payload), 7);
        let (held, flags) = payloads.shape(&found, true).expect("held");
        assert!(Arc::ptr_eq(&held, &payload));
        assert_eq!(flags, 7);
        assert!(
            payloads.shape(&found, false).is_none(),
            "the union is part of the key"
        );
        assert!(
            payloads.shape(&self::found(), true).is_none(),
            "another recognition misses"
        );

        // Touched every frame, the entry stays.
        drop(held);
        for _ in 0..4 {
            payloads.end_frame();
            payloads.begin_frame();
            assert!(payloads.shape(&found, true).is_some());
        }
        // Untouched, it stays while a record holds the payload, and goes two frames after.
        for _ in 0..8 {
            payloads.end_frame();
            payloads.begin_frame();
        }
        assert_eq!(payloads.shapes.len(), 1, "a record holds the payload");
        drop(payload);
        for _ in 0..2 {
            payloads.end_frame();
            payloads.begin_frame();
        }
        assert!(payloads.shapes.is_empty());
        assert_eq!(Arc::strong_count(&found), 1, "the recognition is let go");
    }

    /// Draws `drawing` into a scene of its own and returns the payload of its one mark.
    fn draw(drawing: &Drawing, source: &CachedMarks) -> Arc<MarkPayload> {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(64, 64));
        let paint = ShapePaint {
            fill: Color::WHITE,
            stroke: None,
            stroke_width: 1.0,
        };
        let placement = VectorPlacement {
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            scale: 1.0,
        };
        draw_drawing(&mut scene, VectorId(1), drawing, paint, source, placement);
        assert_eq!(scene.primitives.marks.len(), 1, "the shape draws as a mark");
        Arc::clone(&scene.primitives.mark_payloads[0])
    }

    #[test]
    fn a_new_canvas_draws_when_pins_fill_every_cache() {
        assert_eq!(MAX_SHAPES, MAX_ENTRIES);
        let source = CachedMarks::new();
        // Every entry of both maps belongs to a payload a paint record holds.
        let mut pins = Vec::with_capacity(MAX_SHAPES);
        for _ in 0..MAX_SHAPES {
            let path = Arc::new(BezPath::new());
            let found = found();
            source.recognitions.borrow_mut().insert(
                &path,
                PartKey::Fill,
                0,
                1,
                Some(Arc::clone(&found)),
            );
            let payload = Arc::new(MarkPayload::default());
            source
                .payloads
                .borrow_mut()
                .insert_shape(&found, false, Arc::clone(&payload), 0);
            pins.push(payload);
        }
        source.end_frame();
        assert_eq!(source.recognitions.borrow().len(), MAX_ENTRIES);
        assert_eq!(source.payloads.borrow().shapes.len(), MAX_SHAPES);

        let mut path = BezPath::new();
        for (x, y) in [(10.0, 10.0), (30.0, 12.0), (20.0, 40.0)] {
            path.extend(Circle::new((x, y), 4.0).path_elements(0.1));
        }
        let mut canvas = CanvasScene::default();
        canvas.replace(vec![
            zgui_canvas::ShapeBuilder::new(path)
                .fill(Brush::Inherited { alpha: 1.0 })
                .build(),
        ]);
        let drawing = Drawing::canvas(&canvas, Affine::IDENTITY);

        let uncached = zgui_profile::counter::get(Counter::MarksUncached);
        let first = draw(&drawing, &source);
        let second = draw(&drawing, &source);
        assert_eq!(first.discs.len(), 3, "the canvas draws in full");
        assert_eq!(first.discs, second.discs);
        assert!(!Arc::ptr_eq(&first, &second), "no entry keeps the payload");
        if zgui_profile::COUNTERS_ENABLED {
            assert!(zgui_profile::counter::get(Counter::MarksUncached) >= uncached + 2);
        }

        // Once the pins drop, the old entries expire and the canvas keeps its payload.
        drop(pins);
        let mut kept = None;
        for frame in 0..4 {
            source.end_frame();
            let one = draw(&drawing, &source);
            let two = draw(&drawing, &source);
            assert_eq!(one.discs, first.discs);
            if Arc::ptr_eq(&one, &two) {
                kept = Some(frame);
                break;
            }
        }
        assert!(kept.is_some(), "the canvas caches again once the pins drop");
        source.end_frame();
        let held = draw(&drawing, &source);
        assert!(Arc::ptr_eq(&held, &draw(&drawing, &source)));
    }

    #[test]
    fn a_full_map_drops_the_oldest_unpinned_entries() {
        let mut payloads = MarkPayloads::default();
        payloads.begin_frame();
        let mut pins = Vec::new();
        let mut found_all = Vec::new();
        for index in 0..MAX_SHAPES {
            // The first four entries are a frame older than the rest.
            if index == 4 {
                payloads.end_frame();
                payloads.begin_frame();
            }
            let found = found();
            let payload = Arc::new(MarkPayload::default());
            payloads.insert_shape(&found, false, Arc::clone(&payload), 0);
            // Every other entry is pinned.
            if index % 2 == 0 {
                pins.push(payload);
            }
            found_all.push(found);
        }
        let extra = found();
        payloads.insert_shape(&extra, false, Arc::new(MarkPayload::default()), 0);
        assert_eq!(payloads.shapes.len(), MAX_SHAPES - EVICTED_SHAPES + 1);
        assert!(
            payloads.shape(&extra, false).is_some(),
            "the new entry is kept"
        );
        for index in [1, 3] {
            assert!(
                payloads.shape(&found_all[index], false).is_none(),
                "the oldest unpinned entries go first"
            );
        }
        for found in found_all.iter().step_by(2) {
            assert!(
                payloads.shape(found, false).is_some(),
                "a pinned entry stays"
            );
        }
    }
}
