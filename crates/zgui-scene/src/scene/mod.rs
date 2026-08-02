//! The display list itself.

pub mod batching;
pub mod depends;
pub mod finish;
pub mod insert;
pub(crate) mod live;
pub mod ordering;
pub mod primitives;
pub mod replay;
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
use crate::scene::resolve::Unresolved;
use crate::spatial::{SpatialId, SpatialTree};

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
    /// The previous frame's primitives, kept so a range of its log can be replayed.
    retained: Primitives,
    /// The previous frame's log.
    retained_ops: Vec<PaintOp>,
    /// The previous frame's names, so a replayed range keeps the ones it was recorded with.
    retained_spaces: Vec<Option<SpatialId>>,
    /// Whether the names are kept at all.
    ///
    /// Read once, when the scene is made, rather than per primitive: it is a word of storage and a
    /// lookup for every primitive of every frame, and how a run is being checked is decided before
    /// the run.
    checking: bool,
    /// The draw-order assigner.
    order: BoundsTree,
    /// The region each moving coordinate system declared it would visit.
    ///
    /// Read when a matrix is written after the ordering is done, which is the one moment the
    /// order a primitive already holds can stop being the right one. See [`mod@crate::place`].
    pub(crate) travel: Travels,
    /// Orders forced by an explicitly pushed layer, innermost last.
    layer_stack: Vec<DrawOrder>,
    /// Every order a layer forced this frame, so a check knows which classes it does not own.
    forced_orders: Vec<DrawOrder>,
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
            retained: Primitives::default(),
            retained_ops: Vec::new(),
            retained_spaces: Vec::new(),
            checking: crate::invariant::enabled(),
            order: BoundsTree::new(),
            travel: Travels::new(),
            layer_stack: Vec::new(),
            forced_orders: Vec::new(),
            pass_plan: ScenePassPlan::default(),
            viewport: Size::new(0, 0),
            finished: false,
            unreplayable: 0,
            unresolved: Vec::new(),
        }
    }

    /// How many primitives have been pushed that replaying the log would not reproduce.
    ///
    /// Two things are counted, because they are the same fact: a primitive the clip refused never
    /// reached the log at all, and a vector item is in the log but is planned into a rasterisation
    /// pass rather than re-emitted, so [`Scene::replay`] skips it. Either way the log is less than
    /// what was drawn.
    ///
    /// The count runs across frames and never resets, so a caller reads it either side of the
    /// pushing it is asking about and compares the two. Equal means the log holds the whole of what
    /// that pushing drew, which is exactly the condition under which the range may stand in for the
    /// drawing later.
    pub fn unreplayable(&self) -> u64 {
        self.unreplayable
    }

    /// Records that something was drawn which replaying the log would not reproduce.
    pub(crate) fn note_unreplayable(&mut self) {
        self.unreplayable += 1;
    }

    /// Starts a frame over a surface of `viewport` device pixels.
    ///
    /// The previous frame's primitives and log are retained rather than dropped, so that a fragment
    /// that has not changed can be replayed out of them. The side tables are *not* cleared: their
    /// ids have to keep resolving to the same content, or a replayed range would draw one fragment
    /// with another fragment's paint.
    pub fn begin_frame(&mut self, viewport: Size<i32, Device>) {
        core::mem::swap(&mut self.primitives, &mut self.retained);
        core::mem::swap(&mut self.ops, &mut self.retained_ops);
        core::mem::swap(&mut self.spaces, &mut self.retained_spaces);
        self.primitives.clear();
        self.ops.clear();
        self.spaces.clear();
        self.order.clear();
        self.layer_stack.clear();
        self.forced_orders.clear();
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
    ///
    /// A fragment records the range of this log its primitives occupy, and replays that range next
    /// frame when nothing about it changed.
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
