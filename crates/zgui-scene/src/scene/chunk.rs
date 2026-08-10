//! One fragment's compiled painting, owned by whoever recorded it and kept across frames.
//!
//! A [`ChunkPrims`] holds copies of the primitives one encoding pushed, in the order it pushed
//! them. The scene's own log names positions in arrays that are cleared every frame; a chunk owns
//! its bytes, so it stays valid however many frames pass between the encoding and the replay.

use core::ops::Range;

use zgui_geom::{Device, DevicePx, Size};
use zgui_profile::{Counter, counter};

use crate::group::BackdropFilter;
use crate::id::{ClipId, PaintId};
use crate::ops::PaintOp;
use crate::paint::PaintRef;
use crate::prim::{
    ColorSprite, Decoration, ExternalQuad, MonoSprite, PrimitiveKind, Quad, Shadow, SubpixelSprite,
};
use crate::scene::Scene;
use crate::scene::replay::translate;
use crate::spatial::SpatialId;
use crate::vector::VectorItem;

/// Every side-table entry a chunk's primitives name, distinct per table.
///
/// What a record holds against eviction for as long as it stands: a replayed primitive carries
/// these indices and looks nothing up, so the tables must keep resolving them to what they meant
/// at the encoding. Recomputed from the chunk at release rather than stored, so the two cannot
/// disagree.
#[derive(Debug, Default)]
pub struct TableHolds {
    /// The clip chains named, distinct.
    pub clips: Vec<ClipId>,
    /// The paint sources named, distinct.
    pub paints: Vec<PaintId>,
}

impl TableHolds {
    /// Empties both lists, keeping their allocations.
    pub fn clear(&mut self) {
        self.clips.clear();
        self.paints.clear();
    }

    /// Notes one clip chain.
    fn clip(&mut self, raw: u32) {
        self.clips.push(ClipId(raw));
    }

    /// Notes one paint reference, if it names a table entry.
    fn paint(&mut self, reference: PaintRef) {
        if let Some(id) = reference.id() {
            self.paints.push(id);
        }
    }

    /// Sorts and deduplicates both lists, so each entry is held exactly once per record.
    fn settle(&mut self) {
        self.clips.sort_unstable();
        self.clips.dedup();
        self.paints.sort_unstable();
        self.paints.dedup();
    }
}

/// The primitives one encoding pushed, one array per kind, with the order they were pushed in.
///
/// Each entry of [`ChunkPrims::ops`] carries an index into this chunk's own array for its kind,
/// so the chunk means the same thing on every frame it is replayed. Vector items have an array
/// here for the capture that arrives with vector replay; until then it stays empty.
#[derive(Clone, Debug, Default)]
pub struct ChunkPrims {
    /// What was pushed, in order. Indices name positions in the arrays below.
    pub ops: Vec<PaintOp>,
    /// Rounded, bordered rectangles.
    pub quads: Vec<Quad>,
    /// Box shadows.
    pub shadows: Vec<Shadow>,
    /// Text decoration lines.
    pub decorations: Vec<Decoration>,
    /// Single-channel coverage sprites.
    pub mono_sprites: Vec<MonoSprite>,
    /// Three-channel coverage sprites.
    pub subpixel_sprites: Vec<SubpixelSprite>,
    /// Full-colour sprites.
    pub color_sprites: Vec<ColorSprite>,
    /// Textures the renderer did not draw.
    pub externals: Vec<ExternalQuad>,
    /// Filters over the composite beneath them.
    pub backdrops: Vec<BackdropFilter>,
    /// Vector content, rasterised elsewhere and composited back in.
    pub vectors: Vec<VectorItem>,
    /// The coordinate system each entry was pushed under, parallel to [`ChunkPrims::ops`].
    ///
    /// Empty unless the invariant checks are on — see [`Scene`]'s `spaces`.
    pub spaces: Vec<Option<SpatialId>>,
}

impl ChunkPrims {
    /// How many operations the chunk holds.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether the chunk holds nothing.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// How many bytes the chunk keeps allocated, which is what a budget counts.
    pub fn bytes(&self) -> usize {
        self.ops.capacity() * size_of::<PaintOp>()
            + self.quads.capacity() * size_of::<Quad>()
            + self.shadows.capacity() * size_of::<Shadow>()
            + self.decorations.capacity() * size_of::<Decoration>()
            + self.mono_sprites.capacity() * size_of::<MonoSprite>()
            + self.subpixel_sprites.capacity() * size_of::<SubpixelSprite>()
            + self.color_sprites.capacity() * size_of::<ColorSprite>()
            + self.externals.capacity() * size_of::<ExternalQuad>()
            + self.backdrops.capacity() * size_of::<BackdropFilter>()
            + self.vectors.capacity() * size_of::<VectorItem>()
            + self.spaces.capacity() * size_of::<Option<SpatialId>>()
    }

    /// Collects every side-table entry the chunk's primitives name into `holds`, distinct.
    ///
    /// Emptied first, so the answer is this chunk's and nobody's leftovers.
    pub fn named_ids(&self, holds: &mut TableHolds) {
        holds.clear();
        for quad in &self.quads {
            holds.clip(quad.clip);
            holds.paint(quad.fill);
            holds.paint(quad.stroke);
        }
        for shadow in &self.shadows {
            holds.clip(shadow.clip);
        }
        for decoration in &self.decorations {
            holds.clip(decoration.clip);
        }
        for sprite in &self.mono_sprites {
            holds.clip(sprite.clip);
        }
        for sprite in &self.subpixel_sprites {
            holds.clip(sprite.clip);
        }
        for sprite in &self.color_sprites {
            holds.clip(sprite.clip);
        }
        for external in &self.externals {
            holds.clips.push(external.clip);
        }
        for backdrop in &self.backdrops {
            holds.clips.push(backdrop.clip);
        }
        for vector in &self.vectors {
            holds.clips.push(vector.clip);
            if let Some(fill) = vector.fill {
                holds.paint(fill);
            }
            if let Some(stroke) = &vector.stroke {
                holds.paint(stroke.paint);
            }
        }
        holds.settle();
    }

    /// Empties every array, keeping the allocations for the next capture.
    pub fn clear(&mut self) {
        self.ops.clear();
        self.quads.clear();
        self.shadows.clear();
        self.decorations.clear();
        self.mono_sprites.clear();
        self.subpixel_sprites.clear();
        self.color_sprites.clear();
        self.externals.clear();
        self.backdrops.clear();
        self.vectors.clear();
        self.spaces.clear();
    }
}

/// Copies one primitive out of a frame array into a chunk array, and says where it landed.
fn copied<T: Clone>(source: &[T], index: usize, into: &mut Vec<T>) -> Option<u32> {
    let value = source.get(index)?.clone();
    let at = into.len() as u32;
    into.push(value);
    Some(at)
}

impl Scene {
    /// Opens a chunk capture: until [`Scene::take_chunk_capture`], every pushed primitive is also
    /// appended to the capture — before the clip cull and before the order assignment.
    ///
    /// That makes the capture the pushing's *complete* content: a fragment encoded at the edge of
    /// a scroll port captures the whole of itself, and the cull decides what the frame gets at
    /// every later selection rather than once at the encoding. Group markers are never captured —
    /// their order comes from a barrier, and they are pass state rather than fragment state.
    ///
    /// `recycled` is emptied and used as the storage, so a caller that hands back what it took
    /// last time allocates only where a painting grew.
    pub fn begin_chunk_capture(&mut self, mut recycled: ChunkPrims) {
        debug_assert!(
            self.capture.is_none(),
            "a chunk capture is already open: fragments are captured one at a time"
        );
        recycled.clear();
        self.capture = Some(recycled);
    }

    /// Closes the capture opened by [`Scene::begin_chunk_capture`] and returns it.
    pub fn take_chunk_capture(&mut self) -> ChunkPrims {
        self.capture
            .take()
            .expect("a chunk capture was opened before being taken")
    }

    /// Copies the operations in `range` of this frame's log into `chunk`, re-based so every
    /// entry indexes the chunk's own arrays.
    ///
    /// This runs before [`Scene::finish`]: the log indices are as pushed, and the sort rewrites
    /// them. Vector items and group markers are left out, because a replay skips them — a chunk
    /// holds exactly what a replay re-emits, so what a replay cannot reproduce is counted by
    /// [`Scene::unreplayable`] at the pushes instead.
    pub fn extract_chunk(&self, range: Range<u32>, chunk: &mut ChunkPrims) {
        debug_assert!(
            !self.finished,
            "a chunk is extracted before the sort rewrites the log it indexes"
        );
        chunk.clear();
        let (first, last) = (range.start as usize, range.end as usize);
        if first > last || last > self.ops.len() {
            return;
        }
        for position in first..last {
            let op = self.ops[position];
            let index = op.index as usize;
            let at = match op.kind {
                PrimitiveKind::Quad => copied(&self.primitives.quads, index, &mut chunk.quads),
                PrimitiveKind::Shadow => {
                    copied(&self.primitives.shadows, index, &mut chunk.shadows)
                }
                PrimitiveKind::Decoration => {
                    copied(&self.primitives.decorations, index, &mut chunk.decorations)
                }
                PrimitiveKind::MonoSprite => copied(
                    &self.primitives.mono_sprites,
                    index,
                    &mut chunk.mono_sprites,
                ),
                PrimitiveKind::SubpixelSprite => copied(
                    &self.primitives.subpixel_sprites,
                    index,
                    &mut chunk.subpixel_sprites,
                ),
                PrimitiveKind::ColorSprite => copied(
                    &self.primitives.color_sprites,
                    index,
                    &mut chunk.color_sprites,
                ),
                PrimitiveKind::External => {
                    copied(&self.primitives.externals, index, &mut chunk.externals)
                }
                PrimitiveKind::Backdrop => {
                    copied(&self.primitives.backdrops, index, &mut chunk.backdrops)
                }
                PrimitiveKind::Vector | PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => None,
            };
            let Some(at) = at else {
                continue;
            };
            chunk.ops.push(PaintOp::new(op.kind, at));
            if self.checking {
                chunk
                    .spaces
                    .push(self.spaces.get(position).copied().flatten());
            }
        }
    }

    /// Re-emits a chunk's operations, offset by `by`, and returns the range they occupy in this
    /// frame's log.
    ///
    /// This is [`Scene::replay`] reading a caller-owned chunk instead of the previous frame's
    /// log, and everything that holds there holds here: draw order is re-derived through the
    /// ordinary push path, the paints travel in the instances and are re-anchored rather than
    /// re-interned, and vector items are skipped.
    pub fn replay_chunk(&mut self, chunk: &ChunkPrims, by: Size<DevicePx, Device>) -> Range<u32> {
        let start = self.ops.len() as u32;
        for (position, op) in chunk.ops.iter().enumerate() {
            let index = op.index as usize;
            // Where this primitive's log entry will land, so the name it was originally pushed
            // under can be put back over the one the ordinary push path just wrote.
            let logged = self.ops.len();
            let recorded = chunk.spaces.get(position).copied().flatten();
            match op.kind {
                PrimitiveKind::Quad => {
                    let Some(mut quad) = chunk.quads.get(index).copied() else {
                        continue;
                    };
                    translate(&mut quad.bounds, by);
                    quad.reanchor_paint(by);
                    if quad.samples_its_paint() && (by.width.0 != 0.0 || by.height.0 != 0.0) {
                        counter::bump(Counter::PaintsReanchored);
                    }
                    self.push_quad(quad);
                }
                PrimitiveKind::Shadow => {
                    let Some(mut shadow) = chunk.shadows.get(index).copied() else {
                        continue;
                    };
                    translate(&mut shadow.bounds, by);
                    translate(&mut shadow.element_bounds, by);
                    self.push_shadow(shadow);
                }
                PrimitiveKind::Decoration => {
                    let Some(mut decoration) = chunk.decorations.get(index).copied() else {
                        continue;
                    };
                    translate(&mut decoration.bounds, by);
                    self.push_decoration(decoration);
                }
                PrimitiveKind::MonoSprite => {
                    let Some(mut sprite) = chunk.mono_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    self.push_mono_sprite(sprite);
                }
                PrimitiveKind::SubpixelSprite => {
                    let Some(mut sprite) = chunk.subpixel_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    self.push_subpixel_sprite(sprite);
                }
                PrimitiveKind::ColorSprite => {
                    let Some(mut sprite) = chunk.color_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    // The frame moves with the picture it confines, or a replayed `cover` is cut
                    // against the rectangle its box moved away from.
                    translate(&mut sprite.frame, by);
                    self.push_color_sprite(sprite);
                }
                PrimitiveKind::External => {
                    let Some(mut external) = chunk.externals.get(index).copied() else {
                        continue;
                    };
                    external.bounds = external.bounds.translate(by);
                    self.push_external(external);
                }
                PrimitiveKind::Backdrop => {
                    let Some(mut backdrop) = chunk.backdrops.get(index).cloned() else {
                        continue;
                    };
                    backdrop.bounds = backdrop.bounds.translate(by);
                    backdrop.source = backdrop.source.translate(by);
                    self.push_backdrop(backdrop);
                }
                PrimitiveKind::Vector | PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {}
            }
            if self.checking && self.ops.len() > logged {
                self.spaces[logged] = recorded;
            }
        }
        start..self.ops.len() as u32
    }
}

#[cfg(test)]
mod tests;
