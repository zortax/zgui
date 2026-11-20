//! A chunk's items recorded without a compositor, for reading back.

use std::sync::Arc;

use compositor_types::{
    Backdrop, Filter, Glyph, GlyphRun, Image, ItemOp, Line, MarkPayload, Marks, PathItem, Quad,
    Shadow,
};
use zgui_scene::{ClipId, Scene, SpatialId};

use crate::emit::record::{Items, Recorder};

/// One chunk's items recorded without a compositor, for reading back.
///
/// Nothing refers to a node or a slot. Items name the chunk's default space and clip, colours are
/// carried in their references, images are slot zero and every other paint is none.
#[derive(Debug, Default)]
pub struct Recording {
    items: Items,
}

impl Recording {
    /// Creates an empty recording.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a recorder that empties the recording and records into it, with `space` and `clip`
    /// as the chunk's defaults.
    pub fn recorder<'a>(
        &'a mut self,
        scene: &'a mut Scene,
        space: SpatialId,
        clip: ClipId,
    ) -> Recorder<'a> {
        Recorder::new(scene, &mut self.items, None, space, clip)
    }

    /// Returns every item's kind and index into its kind's list, in record order.
    pub fn ops(&self) -> &[ItemOp] {
        &self.items.ops
    }

    /// Returns the quads, in record order.
    pub fn quads(&self) -> &[Quad] {
        &self.items.quads
    }

    /// Returns the shadows, in record order.
    pub fn shadows(&self) -> &[Shadow] {
        &self.items.shadows
    }

    /// Returns the decoration lines, in record order.
    pub fn lines(&self) -> &[Line] {
        &self.items.lines
    }

    /// Returns the glyph runs, in record order.
    pub fn glyph_runs(&self) -> &[GlyphRun] {
        &self.items.glyph_runs
    }

    /// Returns the glyphs of `run`.
    pub fn glyphs(&self, run: &GlyphRun) -> &[Glyph] {
        &self.items.glyphs[run.glyphs.indices()]
    }

    /// Returns the images, in record order.
    pub fn images(&self) -> &[Image] {
        &self.items.images
    }

    /// Returns the marks items, in record order.
    pub fn marks(&self) -> &[Marks] {
        &self.items.marks
    }

    /// Returns the payload `marks` draws from.
    pub fn payload(&self, marks: &Marks) -> &Arc<MarkPayload> {
        &self.items.payloads[marks.payload as usize]
    }

    /// Returns the paths, in record order.
    pub fn paths(&self) -> &[PathItem] {
        &self.items.paths
    }

    /// Returns the backdrops, in record order.
    pub fn backdrops(&self) -> &[Backdrop] {
        &self.items.backdrops
    }

    /// Returns the filter chain `backdrop` applies.
    pub fn filters(&self, backdrop: &Backdrop) -> &Arc<[Filter]> {
        &self.items.filters[backdrop.filters as usize]
    }
}
