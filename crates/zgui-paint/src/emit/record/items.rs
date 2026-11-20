//! The items of one chunk, held until the chunk is recorded.

use compositor_types::{
    ClipId, Glyph, GlyphRun, ItemKind, ItemOp, Line, LocalClip, PaintId, PaintKind, Quad, Shadow,
    TransformId,
};
use zgui_scene::{ClipId as SceneClip, SpatialId};

/// The items of one chunk in record order, with the ids they refer to.
///
/// Item references are already compositor slots. The ids are written into the chunk's `refs` when
/// the chunk is recorded.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Items {
    /// The coordinate system of every item whose space is `SpaceRef::INHERIT`.
    pub(crate) space: SpatialId,
    /// The clip chain of every item whose clip is `ClipRef::INHERIT`.
    pub(crate) clip: SceneClip,
    pub(crate) ops: Vec<ItemOp>,
    /// How many scene primitives the fragment had pushed when each item was recorded, parallel to
    /// `ops`.
    pub(crate) at: Vec<u32>,
    pub(crate) quads: Vec<Quad>,
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) lines: Vec<Line>,
    pub(crate) glyph_runs: Vec<GlyphRun>,
    /// The glyph arena the glyph runs take ranges of.
    pub(crate) glyphs: Vec<Glyph>,
    /// The coordinate arena the glyph runs take ranges of.
    pub(crate) coords: Vec<i16>,
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
}

impl Default for Items {
    fn default() -> Self {
        Self {
            space: SpatialId::VIEWPORT,
            clip: SceneClip::ROOT,
            ops: Vec::new(),
            at: Vec::new(),
            quads: Vec::new(),
            shadows: Vec::new(),
            lines: Vec::new(),
            glyph_runs: Vec::new(),
            glyphs: Vec::new(),
            coords: Vec::new(),
            local_clips: Vec::new(),
            refs: Vec::new(),
        }
    }
}

impl Items {
    /// Empties the items and sets the chunk's default space and clip, keeping the allocations.
    pub(crate) fn reset(&mut self, space: SpatialId, clip: SceneClip) {
        self.space = space;
        self.clip = clip;
        self.ops.clear();
        self.at.clear();
        self.quads.clear();
        self.shadows.clear();
        self.lines.clear();
        self.glyph_runs.clear();
        self.glyphs.clear();
        self.coords.clear();
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

    pub(crate) fn push_quad(&mut self, quad: Quad, at: usize) {
        self.push_op(ItemKind::QUAD, self.quads.len(), at);
        self.quads.push(quad);
    }

    pub(crate) fn push_shadow(&mut self, shadow: Shadow, at: usize) {
        self.push_op(ItemKind::SHADOW, self.shadows.len(), at);
        self.shadows.push(shadow);
    }

    pub(crate) fn push_line(&mut self, line: Line, at: usize) {
        self.push_op(ItemKind::LINE, self.lines.len(), at);
        self.lines.push(line);
    }

    pub(crate) fn push_glyph_run(&mut self, run: GlyphRun, at: usize) {
        self.push_op(ItemKind::GLYPH_RUN, self.glyph_runs.len(), at);
        self.glyph_runs.push(run);
    }

    /// Appends the item `op` of `other`, with its arena ranges and local clip references valid in
    /// `other`.
    pub(crate) fn push_from(&mut self, other: &Items, op: ItemOp, at: usize) {
        let index = op.index as usize;
        match op.kind {
            ItemKind::QUAD => self.push_quad(other.quads[index], at),
            ItemKind::SHADOW => self.push_shadow(other.shadows[index], at),
            ItemKind::LINE => self.push_line(other.lines[index], at),
            ItemKind::GLYPH_RUN => self.push_glyph_run(other.glyph_runs[index], at),
            kind => unreachable!("the recorder holds no item of kind {kind:?}"),
        }
    }

    fn push_op(&mut self, kind: ItemKind, index: usize, at: usize) {
        self.ops.push(ItemOp {
            kind,
            _pad: [0; 3],
            index: index as u32,
        });
        self.at.push(at as u32);
    }
}
