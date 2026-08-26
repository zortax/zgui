//! One fragment's compiled painting, owned by whoever recorded it and kept across frames.
//!
//! A [`ChunkPrims`] holds copies of the primitives one encoding pushed, in the order it pushed
//! them. The scene's own log names positions in arrays that are cleared every frame; a chunk owns
//! its bytes, so it stays valid however many frames pass between the encoding and the replay.

use core::ops::Range;

use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::id::DrawOrder;

use crate::clip::link::ClipNode;
use crate::group::BackdropFilter;
use crate::id::{ClipId, PaintId};
use crate::ops::PaintOp;
use crate::paint::PaintRef;
use crate::prim::{
    ColorSprite, Decoration, ExternalQuad, MonoSprite, PrimitiveKind, Quad, Shadow, SubpixelSprite,
};
use crate::scene::Scene;
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

/// Where one pushed instanced primitive came from, for a renderer that keeps chunks resident.
///
/// A primitive replayed out of a chunk carries the chunk's revision and its index in the chunk's
/// own lane, so a renderer holding that chunk's bytes on the device can point a draw at them
/// without any upload. A replay that applied an offset records it in the scene's frame offsets,
/// and the renderer adds it at draw time — the resident bytes hold the encode position always.
/// Everything else — a fresh encoding not yet resident, an outline emitted outside any capture —
/// is transient: this frame's bytes are its only source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkSlot {
    /// The chunk the primitive came from, by revision — zero for transient content.
    pub revision: u64,
    /// Its index in the chunk's own lane for this kind.
    pub index: u32,
}

impl ChunkSlot {
    /// Content whose only source is this frame's arrays.
    pub const TRANSIENT: Self = Self {
        revision: 0,
        index: 0,
    };

    /// Whether the primitive names a chunk at all.
    pub fn is_transient(self) -> bool {
        self.revision == 0
    }
}

/// One chunk a renderer with persistent storage is told about this frame.
#[derive(Clone, Debug)]
pub struct ChunkUpload {
    /// The chunk's identity: the revision its encoding was stamped with, unique for the cache's
    /// life.
    pub revision: u64,
    /// The bytes, at the position the fragment was encoded at.
    pub prims: std::sync::Arc<ChunkPrims>,
}

/// The primitives one encoding pushed, one array per kind, with the order they were pushed in.
///
/// Each entry of [`ChunkPrims::ops`] carries an index into this chunk's own array for its kind,
/// so the chunk means the same thing on every frame it is replayed.
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
    /// What each entry's order is *within the chunk*, parallel to [`ChunkPrims::ops`], counting
    /// from one.
    ///
    /// Not the order the encoding frame gave it. That number is a fact about the frame — where the
    /// chunk's content fell among everything else drawn that day — and a chunk outlives it. What
    /// does survive is how the chunk's own primitives stand against *each other*, which is decided
    /// by the same rule the frame uses and against the same rectangles, over nothing but the
    /// chunk. A replay adds where the chunk now sits to every one of these, and that is the whole
    /// of the ordering it needs.
    ///
    /// So this is filled for the primitives the encoding *culled* as well, which the frame's own
    /// order never was: the capture is the pushing's complete content, and a shape outside the
    /// scroll port when it was encoded is inside it two frames later.
    pub orders: Vec<DrawOrder>,
    /// The highest entry of [`ChunkPrims::orders`], or zero for a chunk holding nothing.
    pub span: DrawOrder,
    /// The rectangle the whole chunk puts ink in, in the coordinates it was recorded in.
    ///
    /// `None` for a chunk with nothing in it. A replay asks the draw-order tree about this one
    /// rectangle in place of asking about every primitive separately.
    pub ink: Option<Rect<DevicePx, Device>>,
    /// Clips the encoding minted for its own content, in mint order.
    ///
    /// A minted clip's rectangle is measured where the encoding drew, so a replay re-interns each
    /// one at the chunk's current position before it re-emits anything — the same identity lands
    /// in the same slot, and the slot comes to hold the moved rectangle.
    pub minted: Vec<ClipId>,
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
            + self.orders.capacity() * size_of::<DrawOrder>()
            + self.minted.capacity() * size_of::<ClipId>()
    }

    /// Whether the chunk carries an order for every operation it holds.
    ///
    /// A chunk built before the orders existed, or one a caller assembled by hand, answers no and
    /// is replayed the long way — every primitive asking the tree for itself, which is what every
    /// replay used to do.
    pub fn carries_orders(&self) -> bool {
        self.orders.len() == self.ops.len() && self.ink.is_some()
    }

    /// Settles the recorded orders into chunk-local ones and derives the span and the ink.
    ///
    /// Called once, where the chunk is closed. The orders arrive as whatever the recording tree
    /// gave them — counting from one for a capture, and from wherever the frame happened to be for
    /// an extraction — and leave counting from one, so a replay adds a base to them and nothing
    /// else.
    pub(crate) fn settle_orders(&mut self) {
        self.span = 0;
        self.ink = None;
        if self.orders.len() != self.ops.len() {
            self.orders.clear();
            return;
        }
        let Some(floor) = self.orders.iter().copied().min() else {
            return;
        };
        for order in &mut self.orders {
            *order = *order - floor + 1;
        }
        self.span = self.orders.iter().copied().max().unwrap_or(0);
        for &op in &self.ops {
            let Some(bounds) = ink_of(self, op) else {
                continue;
            };
            self.ink = Some(match self.ink {
                Some(union) => union.union(bounds),
                None => bounds,
            });
        }
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
        // A minted clip is usually named by a sprite above and deduplicated away; holding it here
        // keeps the slot alive even for a chunk whose every clipped primitive was pushed elsewhere.
        for clip in &self.minted {
            holds.clips.push(*clip);
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
        self.orders.clear();
        self.span = 0;
        self.ink = None;
        self.minted.clear();
    }
}

/// The order one of the frame's own primitives was given, for an extraction to rebase.
///
/// Zero for the kinds an extraction skips, which never reach [`ChunkPrims::ops`] at all.
fn order_of(prims: &crate::scene::primitives::Primitives, op: PaintOp) -> DrawOrder {
    let index = op.index as usize;
    match op.kind {
        PrimitiveKind::Quad => prims.quads.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::Shadow => prims.shadows.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::Decoration => prims.decorations.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::MonoSprite => prims.mono_sprites.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::SubpixelSprite => prims
            .subpixel_sprites
            .get(index)
            .map_or(0, |prim| prim.order),
        PrimitiveKind::ColorSprite => prims.color_sprites.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::External => prims.externals.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::Backdrop => prims.backdrops.get(index).map_or(0, |prim| prim.order),
        PrimitiveKind::Vector | PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => 0,
    }
}

/// Where one recorded operation puts ink, in the coordinates the chunk recorded it in.
fn ink_of(chunk: &ChunkPrims, op: PaintOp) -> Option<Rect<DevicePx, Device>> {
    let index = op.index as usize;
    match op.kind {
        PrimitiveKind::Quad => chunk.quads.get(index).map(Quad::ink),
        PrimitiveKind::Shadow => chunk.shadows.get(index).map(Shadow::ink),
        PrimitiveKind::Decoration => chunk.decorations.get(index).map(Decoration::ink),
        PrimitiveKind::MonoSprite => chunk.mono_sprites.get(index).map(MonoSprite::ink),
        PrimitiveKind::SubpixelSprite => chunk.subpixel_sprites.get(index).map(SubpixelSprite::ink),
        PrimitiveKind::ColorSprite => chunk.color_sprites.get(index).map(ColorSprite::ink),
        PrimitiveKind::External => chunk.externals.get(index).map(ExternalQuad::ink),
        PrimitiveKind::Backdrop => chunk.backdrops.get(index).map(|prim| prim.bounds),
        // A vector item is rasterised elsewhere and composited back in, and a replay does not
        // re-plan a pass for one; its extent is no part of what the block is asked about.
        PrimitiveKind::Vector | PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => None,
    }
}

impl Scene {
    /// Overwrites the provenance of the primitive just pushed with its chunk source.
    fn stamp_replayed(&mut self, kind: PrimitiveKind, source: u64, intra: u32) {
        let lane = provenance_lane(kind);
        if let Some(slot) = self.provenance[lane].last_mut() {
            *slot = ChunkSlot {
                revision: source,
                index: intra,
            };
        }
    }
}

/// The provenance lane one instanced kind's entries live in.
fn provenance_lane(kind: PrimitiveKind) -> usize {
    provenance_lane_of(kind).expect("only instanced kinds are stamped")
}

/// The provenance lane for `kind`, or `None` for a kind that has none.
pub(crate) fn provenance_lane_of(kind: PrimitiveKind) -> Option<usize> {
    Some(match kind {
        PrimitiveKind::Quad => 0,
        PrimitiveKind::Shadow => 1,
        PrimitiveKind::Decoration => 2,
        PrimitiveKind::MonoSprite => 3,
        PrimitiveKind::SubpixelSprite => 4,
        PrimitiveKind::ColorSprite => 5,
        _ => return None,
    })
}

/// Moves an `[x, y, width, height]` field.
fn translate(bounds: &mut [f32; 4], by: Size<DevicePx, Device>) {
    bounds[0] += by.width.0;
    bounds[1] += by.height.0;
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
        self.capture_order.clear();
        self.capture = Some(recycled);
    }

    /// Notes a clip the open capture's encoding minted for its own content.
    ///
    /// A minted clip's rectangle is where the content is being drawn this frame, so the note is
    /// what lets [`Scene::replay_chunk`] carry the clip along with the content. A no-op outside a
    /// capture: an emission nobody records is never replayed.
    pub fn note_minted_clip(&mut self, id: ClipId) {
        if let Some(capture) = &mut self.capture {
            capture.minted.push(id);
        }
    }

    /// Closes the capture opened by [`Scene::begin_chunk_capture`] and returns it.
    pub fn take_chunk_capture(&mut self) -> ChunkPrims {
        let mut chunk = self
            .capture
            .take()
            .expect("a chunk capture was opened before being taken");
        chunk.settle_orders();
        chunk
    }

    /// Stamps every primitive pushed under the last capture with the chunk revision its encoding
    /// was given.
    ///
    /// Called once per encoding, after the capture is taken and the revision exists. Until then
    /// the pushed primitives read as transient, which is also what an unbound capture leaves
    /// behind — a test that captures without encoding loses nothing.
    pub fn bind_capture(&mut self, revision: u64) {
        for (kind, at, intra) in self.capture_stamped.drain(..) {
            let lane = &mut self.provenance[provenance_lane(kind)];
            if let Some(slot) = lane.get_mut(at as usize) {
                *slot = ChunkSlot {
                    revision,
                    index: intra,
                };
            }
        }
    }

    /// Where each primitive of `kind`'s array came from, parallel to the array.
    ///
    /// Instanced kinds only; every other kind answers an empty slice.
    pub fn provenance(&self, kind: PrimitiveKind) -> &[ChunkSlot] {
        match provenance_lane_of(kind) {
            Some(lane) => &self.provenance[lane],
            None => &[],
        }
    }

    /// Notes a chunk a renderer with persistent storage should hold, until the notes are cleared.
    pub fn note_chunk_inserted(&mut self, revision: u64, prims: std::sync::Arc<ChunkPrims>) {
        self.chunk_inserted.push(ChunkUpload { revision, prims });
    }

    /// Notes a chunk that ceased to exist, until the notes are cleared.
    pub fn note_chunk_retired(&mut self, revision: u64) {
        self.chunk_retired.push(revision);
    }

    /// The chunks noted since the notes were last cleared, in note order.
    pub fn chunk_inserted(&self) -> &[ChunkUpload] {
        &self.chunk_inserted
    }

    /// The revisions dropped since the notes were last cleared, in note order.
    pub fn chunk_retired(&self) -> &[u64] {
        &self.chunk_retired
    }

    /// How far each chunk replayed away from where it was encoded this frame, by revision.
    pub fn chunk_offsets(&self) -> &rustc_hash::FxHashMap<u64, [f32; 2]> {
        &self.chunk_offsets
    }

    /// Empties both chunk-note lists.
    ///
    /// The runtime calls this once a draw has consumed them — after the outcome retired the
    /// frame's damage, so a skipped frame's notes stand for the next drawn one, and an eviction
    /// after the draw lands in the notes the following draw consumes.
    pub fn clear_chunk_notes(&mut self) {
        self.chunk_inserted.clear();
        self.chunk_retired.clear();
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
            // The frame's own order, which `settle_orders` rebases to the chunk's. Available here
            // and not in a capture, because an extraction copies primitives the frame has already
            // ordered rather than ones on their way to being ordered.
            chunk.orders.push(order_of(&self.primitives, op));
            if self.checking {
                chunk
                    .spaces
                    .push(self.spaces.get(position).copied().flatten());
            }
        }
        chunk.settle_orders();
    }

    /// Re-emits a chunk's operations, offset by `by`, and returns the range they occupy in this
    /// frame's log.
    ///
    /// Draw order is re-derived through the ordinary push path, the paints travel in the
    /// instances and are re-anchored rather than re-interned, and a vector item is re-pushed into
    /// this frame's pass planning exactly as a fresh emission pushes it. One restriction: a
    /// vector item replays only where `by` is zero, because its curves are placed in device
    /// coordinates and shared with the rasteriser's encoding cache — translating them would mean
    /// copying the path. The caller encodes a moved drawing instead.
    /// `source` is the chunk's revision, stamped as the provenance of every primitive a replay
    /// pushes — a renderer holding the chunk resident points those draws at its copy. A replay
    /// away from the encode position records its offset beside the stamp, and the renderer adds
    /// it at draw time; the frame arrays still hold the translated bytes, which is what a
    /// renderer without residence for the chunk correctly falls back to. Zero for a chunk nobody
    /// tracks. Vectors, externals and backdrops are never stamped — their movement is not a
    /// uniform translation of instanced bytes.
    pub fn replay_chunk(
        &mut self,
        chunk: &ChunkPrims,
        by: Size<DevicePx, Device>,
        source: u64,
    ) -> Range<u32> {
        let start = self.ops.len() as u32;
        let in_place = by.width.0 == 0.0 && by.height.0 == 0.0;
        if !in_place && source != 0 {
            self.chunk_offsets.insert(source, [by.width.0, by.height.0]);
        }
        // A clip the encoding minted is named by its encode-position rectangle, so re-interning it
        // shifted by the chunk's own movement lands in the same slot and rewrites the stored
        // rectangle to where the content now is — the insert cull and the shader both read the
        // moved window, and the clip index baked into the chunk stays valid. Unconditional, so a
        // replay back in place restores the rectangle a moved replay wrote over.
        for id in &chunk.minted {
            let Some(&ClipNode::Link {
                link,
                parent,
                shift,
                ..
            }) = self.clips.get(*id)
            else {
                continue;
            };
            let settled = link.unshifted(shift);
            let moved = settled.unshifted(Size::new(DevicePx(-by.width.0), DevicePx(-by.height.0)));
            let re = self.clips.push_shifted(parent, moved, by);
            debug_assert_eq!(
                re, *id,
                "a minted clip re-interned under its own identity keeps its slot"
            );
        }
        // One question for the whole chunk rather than one per primitive. The chunk's members were
        // ordered against each other when they were captured and have moved rigidly since, so what
        // the tree is asked is where the chunk sits now; the orders inside it are that answer plus
        // the offsets they already had.
        //
        // This is what a scrolled port is made of. The emit walk over one is three quarters of the
        // frame, and the largest thing inside that walk was re-inserting every primitive of every
        // replayed row to rediscover an order none of them had changed.
        let base = chunk.carries_orders().then(|| {
            let ink = chunk
                .ink
                .expect("a chunk that carries orders carries its ink");
            let moved = Rect::new(
                zgui_geom::Point::new(
                    DevicePx(ink.origin.x.0 + by.width.0),
                    DevicePx(ink.origin.y.0 + by.height.0),
                ),
                ink.size,
            );
            self.order.insert_block(moved, chunk.span.saturating_sub(1))
        });
        for (position, op) in chunk.ops.iter().enumerate() {
            let index = op.index as usize;
            if let Some(base) = base {
                // Counting from one, so the chunk's lowest order is the base itself.
                self.replay_order = chunk.orders.get(position).map(|order| base + order - 1);
            }
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
                    if self.push_quad(quad).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::Quad, source, op.index);
                    }
                }
                PrimitiveKind::Shadow => {
                    let Some(mut shadow) = chunk.shadows.get(index).copied() else {
                        continue;
                    };
                    translate(&mut shadow.bounds, by);
                    translate(&mut shadow.element_bounds, by);
                    if self.push_shadow(shadow).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::Shadow, source, op.index);
                    }
                }
                PrimitiveKind::Decoration => {
                    let Some(mut decoration) = chunk.decorations.get(index).copied() else {
                        continue;
                    };
                    translate(&mut decoration.bounds, by);
                    if self.push_decoration(decoration).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::Decoration, source, op.index);
                    }
                }
                PrimitiveKind::MonoSprite => {
                    let Some(mut sprite) = chunk.mono_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    if self.push_mono_sprite(sprite).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::MonoSprite, source, op.index);
                    }
                }
                PrimitiveKind::SubpixelSprite => {
                    let Some(mut sprite) = chunk.subpixel_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    if self.push_subpixel_sprite(sprite).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::SubpixelSprite, source, op.index);
                    }
                }
                PrimitiveKind::ColorSprite => {
                    let Some(mut sprite) = chunk.color_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    // The frame moves with the picture it confines, or a replayed `cover` is cut
                    // against the rectangle its box moved away from.
                    translate(&mut sprite.frame, by);
                    if self.push_color_sprite(sprite).is_some() && source != 0 {
                        self.stamp_replayed(PrimitiveKind::ColorSprite, source, op.index);
                    }
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
                PrimitiveKind::Vector => {
                    let Some(item) = chunk.vectors.get(index) else {
                        continue;
                    };
                    debug_assert!(
                        by.width.0 == 0.0 && by.height.0 == 0.0,
                        "a moved drawing is encoded, never translated: the caller's reuse \
                         decision guarantees a vector item only replays in place"
                    );
                    if by.width.0 != 0.0 || by.height.0 != 0.0 {
                        continue;
                    }
                    self.push_vector(item.clone());
                }
                PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {}
            }
            if self.checking && self.ops.len() > logged {
                self.spaces[logged] = recorded;
            }
        }
        // A push that never happened — a kind a replay skips, or a primitive the chunk no longer
        // holds — leaves its order unclaimed, and nothing outside a replay may take one.
        self.replay_order = None;
        start..self.ops.len() as u32
    }
}

#[cfg(test)]
mod tests;
