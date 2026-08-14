//! The display list itself.

pub mod batching;
pub mod chunk;
pub mod depends;
pub mod finish;
pub mod insert;
pub(crate) mod live;
pub mod ordering;
pub mod primitives;
pub mod resolve;

#[cfg(test)]
mod tests;

use zgui_geom::{Device, Size};

use crate::batch::Batches;
use crate::clip::ClipTable;
use crate::id::DrawOrder;
use crate::ops::PaintOp;
use crate::order::BoundsTree;
use crate::paint::{PaintTable, TextPaintTable};
use crate::pass::ScenePassPlan;
use crate::place::band::Travels;
use crate::prim::PrimitiveKind;
use crate::scene::resolve::Unresolved;
use crate::spatial::{SpatialId, SpatialTree};

pub use crate::scene::chunk::{ChunkPrims, ChunkSlot, ChunkUpload, TableHolds};
pub use crate::scene::depends::SpatialFault;
pub use crate::scene::ordering::OrderOverlap;
pub use crate::scene::primitives::Primitives;

/// Everything a frame draws.
///
/// A scene is built by pushing primitives in painting order, finished once, and then read: by a
/// renderer, or by a test that prints it as a transcript. Pushing assigns each primitive its draw
/// order and drops anything its clip admits nothing of; finishing sorts the arrays into batches and
/// plans the frame's vector passes.
///
/// The four side tables are public because callers add to them directly, and they **outlive the
/// frame**: their ids are stable across frames, which is what makes replaying a previous frame's
/// recorded operations meaningful.
///
/// ```
/// use zgui_bits::DamageSet;
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_scene::{ClipLink, PaintRef, Quad, Scene};
///
/// let mut scene = Scene::new();
/// scene.begin_frame(Size::new(400, 400));
///
/// let fill = {
///     let id = scene.paints.solid(zgui_color::Color::srgb(0.1, 0.1, 0.1, 1.0));
///     PaintRef::solid(id)
/// };
/// let scrollport = scene.clips.only(ClipLink::rect(Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(0.0)),
///     Size::new(DevicePx(400.0), DevicePx(100.0)),
/// )));
///
/// let visible = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(10.0)),
///     Size::new(DevicePx(100.0), DevicePx(20.0)),
/// );
/// let offscreen = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(300.0)),
///     Size::new(DevicePx(100.0), DevicePx(20.0)),
/// );
///
/// assert!(scene.push_quad(Quad::filled(visible, fill).clipped(scrollport)).is_some());
/// assert!(
///     scene.push_quad(Quad::filled(offscreen, fill).clipped(scrollport)).is_none(),
///     "a row its clip admits nothing of never reaches the display list"
/// );
///
/// scene.finish(&DamageSet::full());
/// assert_eq!(scene.primitives.quads.len(), 1);
/// ```
#[derive(Debug)]
pub struct Scene {
    /// This frame's primitives.
    pub primitives: Primitives,
    /// Every clip chain in the document.
    pub clips: ClipTable,
    /// Every paint source in the document.
    pub paints: PaintTable,
    /// Every text brush in the document.
    pub text_paints: TextPaintTable,
    /// Every coordinate system in the document.
    ///
    /// Named by structure rather than interned by content: a primitive names the box whose
    /// coordinate system it is drawn in, and moving that box is a write into the node rather than
    /// a new identity. See [`spatial`](crate::spatial).
    pub spatial: SpatialTree,

    /// What was inserted, in the order it was inserted.
    ops: Vec<PaintOp>,
    /// The coordinate system each log entry was pushed under, where it named one.
    ///
    /// Parallel to [`Scene::ops`], and empty unless the checks are switched on. A primitive
    /// carries only the *slot*, which is the one thing that cannot say whether the coordinate
    /// system it names is still the one it was drawn through — see [`mod@depends`].
    spaces: Vec<Option<SpatialId>>,
    /// The chunk capture in progress, if a caller opened one.
    ///
    /// While open, every pushed primitive is also appended here — before the clip cull and before
    /// the order assignment, so the capture is the pushing's complete content rather than what one
    /// position's clip admitted of it. See [`Scene::begin_chunk_capture`].
    capture: Option<crate::scene::chunk::ChunkPrims>,
    /// The draw order of the open capture's own content, against nothing else.
    ///
    /// A second tree, over one fragment, emptied whenever a capture opens. What it answers is the
    /// question the frame's tree cannot: how the chunk's primitives stand against each other,
    /// which is the part of the ordering that is still true on every later frame. The frame's own
    /// answer folds in everything else drawn that day and is worth nothing to a replay.
    capture_order: BoundsTree,
    /// Whether the names are kept at all.
    ///
    /// Read once, when the scene is made, rather than per primitive: it is a word of storage and a
    /// lookup for every primitive of every frame, and how a run is being checked is decided before
    /// the run.
    checking: bool,
    /// The draw-order permutation of each sortable array — see [`Scene::remap`].
    remap: Remap,
    /// Where each instanced primitive came from, parallel to its array — see
    /// [`Scene::provenance`].
    provenance: [Vec<crate::scene::chunk::ChunkSlot>; 6],
    /// Positions pushed under the open capture, awaiting the revision the encoding is stamped
    /// with: (kind, index in the frame array, index in the capture's lane).
    capture_stamped: Vec<(PrimitiveKind, u32, u32)>,
    /// Chunks encoded since the notes were last cleared — see [`Scene::chunk_inserted`].
    chunk_inserted: Vec<crate::scene::chunk::ChunkUpload>,
    /// Chunk revisions dropped since the notes were last cleared.
    chunk_retired: Vec<u64>,
    /// The draw-order assigner.
    order: BoundsTree,
    /// The region each moving coordinate system declared it would visit.
    ///
    /// Read when a matrix is written after the ordering is done, which is the one moment the
    /// order a primitive already holds can stop being the right one. See [`mod@crate::place`].
    pub(crate) travel: Travels,
    /// Orders forced by an explicitly pushed layer, innermost last.
    layer_stack: Vec<DrawOrder>,
    /// The order the next primitive pushed is to take, set for the length of one replayed push.
    ///
    /// A replay knows every order it wants before it pushes anything: the chunk's members were
    /// ordered against each other when they were captured and have moved rigidly since, so only
    /// where the chunk now sits is a question. Set and taken once per push, so nothing can leak
    /// past the primitive it was reserved for.
    pub(crate) replay_order: Option<DrawOrder>,
    /// Every order a layer forced this frame, so a check knows which classes it does not own.
    forced_orders: Vec<DrawOrder>,
    /// Where the nth marker of each direction sits in [`Primitives::groups`].
    ///
    /// Start and end markers share one array but are batched as two streams, so a cursor into
    /// either stream has to be turned into a position in the shared array. Answering that by
    /// filtering the array is a scan *per batch*, which is quadratic in the number of groups; the
    /// two lists are built once, by [`Scene::index_markers`], out of the same sort that put the
    /// markers in order.
    markers: Markers,
    /// This frame's vector work.
    pass_plan: ScenePassPlan,
    /// The surface's extent, which pass regions are clamped to.
    viewport: Size<i32, Device>,
    /// Whether [`Scene::finish`] has run since the last [`Scene::begin_frame`].
    finished: bool,
    /// How many primitives have been pushed that replaying the log would not reproduce.
    unreplayable: u64,
    /// Which sprites were pushed naming a resource nothing had placed yet.
    ///
    /// Empty for a frame every one of whose rasters was already where it was going to be, which is
    /// the ordinary case and costs a pointer. See [`mod@resolve`].
    unresolved: Vec<Unresolved>,
}

/// The draw-order permutation of each sortable array, as indices into it.
///
/// The arrays keep the order primitives were pushed in — a batch is a range of one of these
/// lists, and a consumer reads the array through it. Sorting a list of indices instead of the
/// structs is what lets a primitive's position in its array outlive the frame's ordering, which
/// is what persistent GPU storage needs.
#[derive(Debug, Default)]
pub(crate) struct Remap {
    /// Rounded, bordered rectangles.
    pub(crate) quads: Vec<u32>,
    /// Box shadows.
    pub(crate) shadows: Vec<u32>,
    /// Text decoration lines.
    pub(crate) decorations: Vec<u32>,
    /// Single-channel coverage sprites.
    pub(crate) mono_sprites: Vec<u32>,
    /// Three-channel coverage sprites.
    pub(crate) subpixel_sprites: Vec<u32>,
    /// Full-colour sprites.
    pub(crate) color_sprites: Vec<u32>,
    /// Textures the renderer did not draw.
    pub(crate) externals: Vec<u32>,
    /// Filters over the composite beneath them.
    pub(crate) backdrops: Vec<u32>,
    /// Group start and end markers.
    pub(crate) groups: Vec<u32>,
}

impl Remap {
    /// Empties every list, keeping the allocations for the next frame.
    fn clear(&mut self) {
        self.quads.clear();
        self.shadows.clear();
        self.decorations.clear();
        self.mono_sprites.clear();
        self.subpixel_sprites.clear();
        self.color_sprites.clear();
        self.externals.clear();
        self.backdrops.clear();
        self.groups.clear();
    }
}

/// Where each direction's markers sit in the array the two directions share.
///
/// `starts[n]` is the position in [`Primitives::groups`] of the nth start marker, and `ends[n]` the
/// same for end markers. Rebuilt once per frame from the sorted array, because that is the array a
/// batch cursor is resolved against.
#[derive(Debug, Default)]
struct Markers {
    /// Positions of the start markers, in draw order.
    starts: Vec<u32>,
    /// Positions of the end markers, in draw order.
    ends: Vec<u32>,
}

impl Markers {
    /// Empties both lists, keeping what they allocated.
    fn clear(&mut self) {
        self.starts.clear();
        self.ends.clear();
    }

    /// The list for one direction.
    fn stream(&self, is_start: bool) -> &[u32] {
        if is_start { &self.starts } else { &self.ends }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    /// An empty scene, with its side tables seeded and nothing pushed.
    pub fn new() -> Self {
        Self {
            primitives: Primitives::default(),
            clips: ClipTable::rooted(),
            paints: PaintTable::new(),
            text_paints: TextPaintTable::new(),
            spatial: SpatialTree::with_viewport(),
            ops: Vec::new(),
            spaces: Vec::new(),
            capture: None,
            capture_order: BoundsTree::new(),
            replay_order: None,
            checking: crate::invariant::enabled(),
            remap: Remap::default(),
            provenance: Default::default(),
            capture_stamped: Vec::new(),
            chunk_inserted: Vec::new(),
            chunk_retired: Vec::new(),
            order: BoundsTree::new(),
            travel: Travels::new(),
            layer_stack: Vec::new(),
            forced_orders: Vec::new(),
            markers: Markers::default(),
            pass_plan: ScenePassPlan::default(),
            viewport: Size::new(0, 0),
            finished: false,
            unreplayable: 0,
            unresolved: Vec::new(),
        }
    }

    /// How many primitives have been pushed that the frame's log does not hold the whole of.
    ///
    /// A primitive the clip refused never reached the log at all, and a blanked unresolved sprite
    /// left it holding a blank. A diagnostic: the count runs across frames and never resets, so a
    /// caller reads it either side of a pushing and compares the two.
    pub fn unreplayable(&self) -> u64 {
        self.unreplayable
    }

    /// Records that something was drawn which replaying the log would not reproduce.
    pub(crate) fn note_unreplayable(&mut self) {
        self.unreplayable += 1;
    }

    /// Starts a frame over a surface of `viewport` device pixels.
    ///
    /// The side tables are *not* cleared: a record's chunk carries their indices across frames,
    /// and an id that stopped resolving to the same content would draw one fragment with another
    /// fragment's paint.
    pub fn begin_frame(&mut self, viewport: Size<i32, Device>) {
        self.primitives.clear();
        self.ops.clear();
        self.spaces.clear();
        self.remap.clear();
        for lane in &mut self.provenance {
            lane.clear();
        }
        self.capture_stamped.clear();
        // The chunk notes are deliberately not cleared here: a frame that never reaches the
        // renderer — skipped, undamaged — leaves them standing for the next frame that does, and
        // an eviction after a draw lands in the notes the following draw consumes. The runtime
        // clears them once a draw has retired its damage.
        self.order.clear();
        self.layer_stack.clear();
        self.forced_orders.clear();
        self.markers.clear();
        self.pass_plan.clear();
        self.unresolved.clear();
        self.viewport = viewport;
        self.finished = false;

        self.clips.begin_frame();
        self.paints.begin_frame();
        crate::scene::live::publish(self);
    }

    /// The surface extent this frame is being built for.
    pub fn viewport(&self) -> Size<i32, Device> {
        self.viewport
    }

    /// What was inserted this frame, in the order it was inserted.
    pub fn ops(&self) -> &[PaintOp] {
        &self.ops
    }

    /// This frame's vector work, valid once [`Scene::finish`] has run.
    pub fn pass_plan(&self) -> &ScenePassPlan {
        &self.pass_plan
    }

    /// The draw-order assigner, for a caller that needs to ask it a question directly.
    ///
    /// Deciding whether a subtree's contents overlap each other is the case this exists for: it is
    /// the same question ordering already answers, and asking it twice with two implementations is
    /// how the two come to disagree.
    pub fn order(&self) -> &BoundsTree {
        &self.order
    }

    /// The draw-order assigner, for a caller that has to insert into it directly.
    pub(crate) fn order_mut(&mut self) -> &mut BoundsTree {
        &mut self.order
    }

    /// The highest draw order assigned this frame.
    pub fn max_order(&self) -> DrawOrder {
        self.order.max_order()
    }

    /// The primitives grouped into draw calls.
    ///
    /// # Panics
    ///
    /// Panics unless [`Scene::finish`] has run for this frame: the arrays are not in order before
    /// then, and a batch taken from unordered arrays would be drawn in the wrong sequence.
    pub fn batches(&self) -> Batches<'_> {
        assert!(
            self.finished,
            "batches() needs a finished scene; call finish() first"
        );
        Batches::new(self)
    }

    /// Whether [`Scene::finish`] has run for this frame.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Whether the checks that cost something are being kept for this scene.
    pub fn is_checking(&self) -> bool {
        self.checking
    }

    /// Whether a layer forced this order, which suspends what equal order otherwise promises.
    pub(crate) fn is_forced_order(&self, order: DrawOrder) -> bool {
        self.forced_orders.contains(&order)
    }
}
