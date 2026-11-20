//! The items of one chunk, held until the chunk is recorded.

use std::sync::Arc;

use compositor_types::{
    Backdrop, ClipId, Filter, Glyph, GlyphRun, Image, ImageId, ItemKind, ItemOp, Line, LocalClip,
    MarkPayload, Marks, PaintId, PaintKind, PathItem, Quad, Shadow, TransformId,
};
use zgui_scene::{ClipId as SceneClip, SpatialId};

/// The items of one chunk in record order, with the ids they refer to.
///
/// Item references are already compositor slots. The ids are written into the chunk's `refs` when
/// the chunk is recorded.
#[derive(Clone, Debug)]
pub(crate) struct Items {
    /// The coordinate system of every item whose space is `SpaceRef::INHERIT`.
    pub(crate) space: SpatialId,
    /// The clip chain of every item whose clip is `ClipRef::INHERIT`.
    pub(crate) clip: SceneClip,
    pub(crate) ops: Vec<ItemOp>,
    pub(crate) quads: Vec<Quad>,
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) lines: Vec<Line>,
    pub(crate) glyph_runs: Vec<GlyphRun>,
    pub(crate) images: Vec<Image>,
    pub(crate) marks: Vec<Marks>,
    pub(crate) paths: Vec<PathItem>,
    pub(crate) backdrops: Vec<Backdrop>,
    /// The glyph arena the glyph runs take ranges of.
    pub(crate) glyphs: Vec<Glyph>,
    /// The payload list the marks items index.
    pub(crate) payloads: Vec<Arc<MarkPayload>>,
    /// The filter-chain list the backdrops index.
    pub(crate) filters: Vec<Arc<[Filter]>>,
    pub(crate) local_clips: Vec<LocalClip>,
    /// The ids the items refer to, each once.
    pub(crate) refs: Vec<Ref>,
}

/// An id a chunk's items refer to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ref {
    /// A transform node.
    Space(TransformId),
    /// A clip node.
    Clip(ClipId),
    /// A paint slot of the kind.
    Paint(PaintKind, PaintId),
    /// An image.
    Image(ImageId),
}

impl Default for Items {
    fn default() -> Self {
        Self {
            space: SpatialId::VIEWPORT,
            clip: SceneClip::ROOT,
            ops: Vec::new(),
            quads: Vec::new(),
            shadows: Vec::new(),
            lines: Vec::new(),
            glyph_runs: Vec::new(),
            images: Vec::new(),
            marks: Vec::new(),
            paths: Vec::new(),
            backdrops: Vec::new(),
            glyphs: Vec::new(),
            payloads: Vec::new(),
            filters: Vec::new(),
            local_clips: Vec::new(),
            refs: Vec::new(),
        }
    }
}

// Shared geometry compares by address, as the compositor's identical-replace rule does.
impl PartialEq for Items {
    fn eq(&self, other: &Self) -> bool {
        self.space == other.space
            && self.clip == other.clip
            && self.ops == other.ops
            && self.quads == other.quads
            && self.shadows == other.shadows
            && self.lines == other.lines
            && self.glyph_runs == other.glyph_runs
            && self.images == other.images
            && self.marks == other.marks
            && self.backdrops == other.backdrops
            && self.glyphs == other.glyphs
            && self.local_clips == other.local_clips
            && self.refs == other.refs
            && self.instanced == other.instanced
            && self.filters == other.filters
            && all(&self.payloads, &other.payloads, Arc::ptr_eq)
            && all(&self.paths, &other.paths, same_path)
    }
}

impl Items {
    /// Empties the items and sets the chunk's default space and clip, keeping the allocations.
    pub(crate) fn reset(&mut self, space: SpatialId, clip: SceneClip) {
        self.space = space;
        self.clip = clip;
        self.ops.clear();
        self.quads.clear();
        self.shadows.clear();
        self.lines.clear();
        self.glyph_runs.clear();
        self.images.clear();
        self.marks.clear();
        self.paths.clear();
        self.backdrops.clear();
        self.glyphs.clear();
        self.payloads.clear();
        self.filters.clear();
        self.local_clips.clear();
        self.refs.clear();
    }

    /// Returns the number of items.
    pub(crate) fn len(&self) -> usize {
        self.ops.len()
    }

    /// Reports whether the chunk holds no item.
    pub(crate) fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub(crate) fn push_quad(&mut self, quad: Quad) {
        self.push_op(ItemKind::QUAD, self.quads.len());
        self.quads.push(quad);
    }

    pub(crate) fn push_shadow(&mut self, shadow: Shadow) {
        self.push_op(ItemKind::SHADOW, self.shadows.len());
        self.shadows.push(shadow);
    }

    pub(crate) fn push_line(&mut self, line: Line) {
        self.push_op(ItemKind::LINE, self.lines.len());
        self.lines.push(line);
    }

    pub(crate) fn push_glyph_run(&mut self, run: GlyphRun) {
        self.push_op(ItemKind::GLYPH_RUN, self.glyph_runs.len());
        self.glyph_runs.push(run);
    }

    pub(crate) fn push_image(&mut self, image: Image) {
        self.push_op(ItemKind::IMAGE, self.images.len());
        self.images.push(image);
    }

    pub(crate) fn push_marks(&mut self, marks: Marks) {
        self.push_op(ItemKind::MARKS, self.marks.len());
        self.marks.push(marks);
    }

    pub(crate) fn push_path(&mut self, path: PathItem) {
        self.push_op(ItemKind::PATH, self.paths.len());
        self.paths.push(path);
    }

    pub(crate) fn push_backdrop(&mut self, backdrop: Backdrop) {
        self.push_op(ItemKind::BACKDROP, self.backdrops.len());
        self.backdrops.push(backdrop);
    }

    /// Returns the index of `payload` in the payload list, adding it unless the list holds it.
    pub(crate) fn payload(&mut self, payload: Arc<MarkPayload>) -> u32 {
        if let Some(at) = self
            .payloads
            .iter()
            .position(|held| Arc::ptr_eq(held, &payload))
        {
            return at as u32;
        }
        self.payloads.push(payload);
        (self.payloads.len() - 1) as u32
    }

    /// Adds `chain` to the filter-chain list and returns its index.
    pub(crate) fn filters(&mut self, chain: Arc<[Filter]>) -> u32 {
        self.filters.push(chain);
        (self.filters.len() - 1) as u32
    }

    fn push_op(&mut self, kind: ItemKind, index: usize) {
        self.ops.push(ItemOp {
            kind,
            _pad: [0; 3],
            index: index as u32,
        });
    }
}

/// Reports whether `a` and `b` have equal lengths and `same` holds for every pair.
fn all<T>(a: &[T], b: &[T], same: impl Fn(&T, &T) -> bool) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
}

/// Reports whether two path items have equal fields, with shared geometry compared by address.
fn same_path(a: &PathItem, b: &PathItem) -> bool {
    Arc::ptr_eq(&a.path, &b.path)
        && Arc::ptr_eq(&a.clips, &b.clips)
        && a.placement == b.placement
        && a.brush == b.brush
        && a.fill == b.fill
        && a.stroke == b.stroke
        && a.clip == b.clip
        && a.space == b.space
}
