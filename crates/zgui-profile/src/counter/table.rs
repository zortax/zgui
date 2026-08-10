//! The counter set itself.

use crate::counter::define::counters;
use crate::counter::group::Group;

counters! {
    /// Elements whose selector matching ran again.
    ElementsRestyled => elements_restyled, Group::BackendNeutral;

    /// Elements whose cascade ran again while their selector matches were kept.
    ElementsRecascaded => elements_recascaded, Group::BackendNeutral;

    /// Individual selector-against-element tests performed.
    SelectorMatches => selector_matches, Group::BackendNeutral;

    /// Nodes a phase traversal looked at, whether or not they owed any work.
    ///
    /// Every other counter records work performed, so a traversal that touched six thousand clean
    /// nodes to service one dirty one would satisfy all of them. This is the counter that notices.
    NodesVisited => nodes_visited, Group::BackendNeutral;

    /// Steps taken by an invalidation walk, including the ones that ended in a subtree being
    /// skipped.
    DirtyWalkSteps => dirty_walk_steps, Group::BackendNeutral;

    /// Sibling links followed while recording which of a node's children owe work.
    ///
    /// The record that lets a traversal skip a clean sibling range has to be told which children
    /// to keep, and the cheapest description of five or more of them is the run between them —
    /// which says nothing about where a sixth sits. This counts the links walked answering that,
    /// and it is the only thing that separates a record maintained in a bounded number of steps
    /// per mark from one that walks the child list on every mark. The second is invisible to every
    /// other counter here: the same children are marked, the same nodes are visited, and the same
    /// work comes out.
    DirtyChildSteps => dirty_child_steps, Group::BackendNeutral;

    /// Boxes rebuilt from their element.
    BoxesRebuilt => boxes_rebuilt, Group::BackendNeutral;

    /// Nodes whose size or position was computed again.
    NodesRelaidOut => nodes_relaid_out, Group::BackendNeutral;

    /// Boxes the `overflow: auto` fixpoint examined for a gutter decision.
    ///
    /// The boxes written `overflow: auto`, and no others. This used to be every box in the
    /// document, twice a layout pass, because finding them meant walking the tree — so the figure
    /// to watch is that it tracks what the document *scrolls* rather than what it contains.
    GuttersExamined => gutters_examined, Group::BackendNeutral;

    /// Times a layout pass had to start from the document root instead of a subtree.
    LayoutReachedRoot => layout_reached_root, Group::BackendNeutral;

    /// Times a layout pass was asked for and the results already held answered it.
    LayoutsHeld => layouts_held, Group::Skip { done: Counter::NodesRelaidOut };

    /// Size-only measurements answered from one a box had already given.
    SizesHeld => sizes_held, Group::Skip { done: Counter::SizesMeasured };

    /// Size-only measurements a box had to compute.
    SizesMeasured => sizes_measured, Group::BackendNeutral;

    /// Text runs shaped, which is the expensive half of laying out text.
    TextShaped => text_shaped, Group::BackendNeutral;

    /// Generated text bytes handed to a shaping pass.
    ///
    /// Read beside [`Counter::TextShaped`]: one enormous paragraph and one short label are both one
    /// shape, while their costs are not remotely alike.
    TextBytesShaped => text_bytes_shaped, Group::BackendNeutral;

    /// Paragraphs broken into lines again while reusing the shaping they already had.
    TextRebroken => text_rebroken, Group::BackendNeutral;

    /// Fragments compared against their previous geometry.
    FragmentsDiffed => fragments_diffed, Group::BackendNeutral;

    /// Fragments whose geometry was recomputed rather than carried over.
    FragmentsRebuilt => fragments_rebuilt, Group::BackendNeutral;

    /// Fragments whose paint operations were emitted afresh instead of replayed from cache.
    Repaints => repaints, Group::BackendNeutral;

    /// Computed styles lowered into a downstream stage's own representation.
    ///
    /// A cascade result holds every CSS property. A stage that consumes one reads a handful of
    /// them, so it lowers the result into a shape of its own once per *distinct style* rather than
    /// once per element. This counts the lowerings that were performed.
    StylesLowered => styles_lowered, Group::BackendNeutral;

    /// Lowerings answered out of the cache rather than performed.
    ///
    /// Read beside [`Counter::StylesLowered`]: what matters is the ratio, since a document whose
    /// elements share styles should lower far fewer times than it has elements.
    StylesLoweredFromCache => styles_lowered_from_cache, Group::BackendNeutral;

    /// Transitions and animations advanced on the cheap path, without a restyle.
    TierBTransitions => tier_b_transitions, Group::BackendNeutral;

    /// Transitions and animations advanced by re-placing a fragment, without a restyle.
    ///
    /// The tier between a repaint and a cascade: what moved is where the box is drawn, so its
    /// fragments are composed again from an interpolated matrix and nothing is styled, measured or
    /// rebuilt. Read beside [`Counter::TierBTransitions`] — together they are every animation the
    /// frame served without asking the style engine anything.
    TierCPlacements => tier_c_placements, Group::BackendNeutral;

    /// Transform changes served by writing one coordinate system and damaging what it moved.
    ///
    /// The skip [`Counter::PlaceWritesWithReemit`] is measured against: a box whose matrix moved
    /// used to cost its fragments being composed again and its primitives being emitted again, and
    /// what it costs here is one write into the node the box established plus the union of where
    /// its ink was and where it is.
    PlaceWritesWithoutReemit => place_writes_without_reemit,
        Group::Skip { done: Counter::PlaceWritesWithReemit };

    /// Transform changes that had to be composed again rather than written.
    ///
    /// An interactive transform with no declared travel, and a keyframed one that left the travel
    /// it declared. Both are correct and both cost what a transform used to cost, so this is the
    /// denominator the skip above is only meaningful against.
    PlaceWritesWithReemit => place_writes_with_reemit, Group::BackendNeutral;

    /// Frames on which a moving box left the order band its animation declared.
    ///
    /// Draw order is assigned once, before anything moves, and a box given its own band is ordered
    /// against the whole of where its animation said it would go. Leaving that region means the
    /// order it holds can no longer be trusted against its neighbours, so the frame composes it
    /// again and inserts it afresh. Anything other than zero over a run whose animations stay
    /// inside their keyframes is a travel union that was computed too small.
    OrderBandEscapes => order_band_escapes, Group::BackendNeutral;

    /// Offscreen targets allocated for groups that must be composited as a unit.
    GroupTargets => group_targets, Group::BackendNeutral;

    /// Cached primitive ranges re-encoded from scratch.
    ChunksReencoded => chunks_reencoded, Group::BackendNeutral;

    /// Cached primitive ranges reused by offsetting them rather than re-encoding them.
    ChunksTranslated => chunks_translated, Group::Skip { done: Counter::ChunksReencoded };

    /// Paint records kept across a frame that did not visit their fragment.
    ///
    /// The retention owned chunks buy: a record survives culling, clean frames and invisibility,
    /// and dies with its fragment. On a partial-damage frame this counts the document outside the
    /// damage — records the old cache dropped and re-encoded on re-entry.
    ChunksRetainedUnvisited => chunks_retained_unvisited, Group::BackendNeutral;

    /// Fragment names handed to the paint cache as destroyed.
    FragmentsRetired => fragments_retired, Group::BackendNeutral;

    /// Primitives added to the scene.
    PrimitivesEmitted => primitives_emitted, Group::BackendNeutral;

    /// Primitives dropped before reaching the scene because nothing damaged could see them.
    PrimitivesCulled => primitives_culled, Group::Skip { done: Counter::PrimitivesEmitted };

    /// Sprites whose resource had to be looked up again after the whole frame had been pushed.
    ///
    /// A sprite names the content it samples; where that content landed in a texture is a separate
    /// answer, and a sprite pushed before the answer exists carries a placeholder until a pass over
    /// the frame fills it in. The pass is per instance and only over the instances that need it,
    /// which is why the alternative — reading the placement in the shader — was refused: that is one
    /// more load per *pixel* of the commonest primitive there is.
    SpritesFixedUp => sprites_fixed_up, Group::BackendNeutral;

    /// Sprites whose resource was already placed when they were pushed.
    SpritesResolvedAtPush => sprites_resolved_at_push,
        Group::Skip { done: Counter::SpritesFixedUp };

    /// Insertions into the structure that assigns draw order.
    ///
    /// Nothing else observes how large the scene the paint stage builds actually is, which is what
    /// makes a scene rebuild that has quietly become the dominant cost visible at all.
    BoundsTreeInserts => bounds_tree_inserts, Group::BackendNeutral;

    /// Hit-test entries updated in place.
    HitEntriesUpdated => hit_entries_updated, Group::BackendNeutral;

    /// Hit-test entries whose new rectangle was written where the entry already sat.
    ///
    /// A subset of [`HitEntriesUpdated`](Counter::HitEntriesUpdated). The two together say whether
    /// a frame that moved a great many fragments moved them through the spatial hierarchy or merely
    /// through the leaves already holding them — which is the difference between a scroll costing
    /// the document and a scroll costing what it moved.
    HitEntriesMovedInPlace => hit_entries_moved_in_place, Group::BackendNeutral;

    /// Hit-test entries taken out of the spatial hierarchy and put back somewhere else.
    HitEntriesReinserted => hit_entries_reinserted, Group::BackendNeutral;

    /// Times the hit-test index was rebuilt wholesale instead of updated.
    HitIndexRebuilds => hit_index_rebuilds, Group::BackendNeutral;

    /// Extra passes run to deliver size and position observations and let their handlers settle.
    ObservationPasses => observation_passes, Group::BackendNeutral;

    /// Timers whose deadline passed and whose callback ran.
    TimersFired => timers_fired, Group::BackendNeutral;

    /// Times the event loop was woken from a wait.
    ///
    /// A well-behaved idle application wakes when something asked it to and not otherwise, so this
    /// is what distinguishes waiting from spinning.
    Wakes => wakes, Group::BackendNeutral;

    /// Draw calls submitted to the GPU.
    DrawCalls => draw_calls, Group::RendererSpecific;

    /// Vector rasterisation passes the scene stage planned.
    ///
    /// The plan is made while the scene is finished, not while it is drawn, so this counts a
    /// decision about the scene and is meaningful under any renderer — including one that draws
    /// nothing at all.
    VelloPasses => vello_passes, Group::BackendNeutral;

    /// Clip layers folded into vector passes so that differently clipped items could share one.
    ///
    /// It sits beside [`Counter::VelloPasses`] because folding a clip *moves* the cost of a
    /// distinctly clipped item rather than deleting it, and a suite watching only the pass count
    /// would read that trade as a free win.
    VectorClipLayers => vector_clip_layers, Group::BackendNeutral;

    /// Glyphs positioned on the surface, whether or not they had to be rasterised.
    GlyphsPlaced => glyphs_placed, Group::BackendNeutral;

    /// Glyphs turned into pixels, which is the expensive half of drawing text.
    ///
    /// Read beside [`Counter::GlyphsPlaced`]: a page whose text has not changed places the same
    /// glyphs every frame and must rasterise none of them, so anything other than zero on a repaint
    /// of unchanged text is a cache that is not being consulted. Hinting a single glyph costs tens
    /// of microseconds, which is why this is the one text counter a budget is written against.
    GlyphsRasterised => glyphs_rasterised, Group::BackendNeutral;

    /// Glyphs turned into pixels again because the tile holding them had been freed.
    ///
    /// A subset of [`Counter::GlyphsRasterised`], and the one that separates a cache filling up
    /// from a cache thrashing: a first sighting of a glyph has to be rasterised whatever the
    /// policy is, and a glyph rasterised twice is work a different policy would not have done.
    /// Read as a fraction of [`Counter::GlyphsPlaced`] over a window rather than as a total —
    /// a total only ever grows, so a session that thrashed for one second and behaved for an hour
    /// is indistinguishable from one that thrashed throughout.
    RebuiltAfterEviction => rebuilt_after_eviction, Group::BackendNeutral;

    /// Cached rasters freed to bring the atlas back under its soft limit.
    ///
    /// Read beside [`Counter::RebuiltAfterEviction`]: this one alone says only that the budget is
    /// doing something, and a budget that frees a thousand tiles a frame and rebuilds them all is
    /// doing something worse than nothing.
    AtlasTilesEvicted => atlas_tiles_evicted, Group::BackendNeutral;

    /// Images that could not enter the atlas, even after an eviction step made room.
    ///
    /// Each count is a picture a frame resolved and then drew nothing for. A nonzero total that
    /// keeps growing means the colour pool is too small for the working set, or a single image is
    /// larger than any texture the device may create.
    ImageInsertFailed => image_insert_failed, Group::BackendNeutral;

    /// Encoded bytes live `ImageBytes` registrations are holding.
    ///
    /// These sit beside the decoded texels and the tiles: an in-memory picture costs its encoded
    /// form for as long as the application holds the handle, whatever the caches do.
    EncodedImageBytes => encoded_image_bytes, Group::BackendNeutral;

    /// Atlas tiles a cached primitive range took ownership of when it was recorded.
    ///
    /// A range that is replayed rather than encoded draws its glyphs without looking any of them
    /// up, so nothing else tells the atlas those tiles are still being drawn. The record holds
    /// them instead, and this counts the holds it took.
    RecordTilesRetained => record_tiles_retained, Group::BackendNeutral;

    /// Holds given up when a cached range was re-encoded or dropped.
    ///
    /// Its pair. Over the life of a window the two totals converge, and a gap between them that
    /// grows is a record whose keys were never given back — which is an atlas where nothing is
    /// ever evictable again, and no other counter here can see it.
    RecordTilesReleased => record_tiles_released, Group::BackendNeutral;

    /// Surface pixels inside the damage rectangles that were redrawn.
    DamagePx => damage_px, Group::RendererSpecific;

    /// Bytes copied to the GPU.
    BytesUploaded => bytes_uploaded, Group::RendererSpecific;

    /// Individual atlas rectangles copied to GPU textures.
    AtlasTextureWrites => atlas_texture_writes, Group::RendererSpecific;

    /// Non-empty staged atlas upload batches submitted to the queue.
    AtlasUploadBatches => atlas_upload_batches, Group::RendererSpecific;

    /// Side-table slots flattened again for a renderer.
    ///
    /// Read beside upload bytes: this stays at zero for unchanged tables and exposes a CPU-side
    /// regression even when the upload cache still happens to hide it.
    SideTableSlotsPrepared => side_table_slots_prepared, Group::RendererSpecific;

    /// Reusable upload chunks allocated rather than reclaimed from an earlier submission.
    ///
    /// Ordinarily non-zero only while the belt grows to its working set. A recurring value points
    /// directly at GPU backlog or a workload that repeatedly exceeds that working set.
    UploadChunksAllocated => upload_chunks_allocated, Group::RendererSpecific;

    /// Mapped staging bytes the upload belt keeps warm for reuse.
    ///
    /// The belt's steady working set. A figure far above what ordinary frames write means a burst
    /// grew the belt and the retention period has not yet let it go.
    StagingWarmBytes => staging_warm_bytes, Group::RendererSpecific;

    /// Staging bytes in one-shot chunks: oversized transfers that leave with their frame.
    ///
    /// Read as a high-water sign of image and scene bursts. It falls to zero on its own; a value
    /// that persists means large uploads on every frame.
    StagingOneShotBytes => staging_one_shot_bytes, Group::RendererSpecific;

    /// Frames the loop actually drew.
    ///
    /// Named for what it counts. A window with nothing moving in it is supposed to leave this
    /// where it found it, so a panel that redraws on every vsync reads as three hundred over
    /// three hundred and not as three hundred idle frames.
    FramesDrawn => frames_drawn, Group::BackendNeutral;

    /// Clip chains the scene is holding right now.
    ClipEntriesLive => clip_entries_live, Group::Live;

    /// Paints the scene is holding right now.
    PaintEntriesLive => paint_entries_live, Group::Live;

    /// Coordinate systems the spatial tree is holding right now.
    SpatialNodesLive => spatial_nodes_live, Group::Live;

    /// Fragments the layout tree is holding right now.
    FragmentsLive => fragments_live, Group::Live;

    /// Box slots the layout tree is holding right now, live and awaiting recycling.
    ///
    /// The arena's capacity rather than its live count. A removed box stays readable until the
    /// frame is recycled, so the live count reads flat through a removal that is never given back
    /// while the memory it holds is never returned — which is exactly the failure this watches for.
    BoxesLive => boxes_live, Group::Live;

    /// Rasterised entries the texture atlas is holding right now.
    AtlasEntriesLive => atlas_entries_live, Group::Live;

    /// Glyph placement or blank-raster answers remembered beside the atlas.
    GlyphEntriesLive => glyph_entries_live, Group::Live;

    /// Layers the vector scratch texture is allocated with right now.
    ScratchLayers => scratch_layers, Group::Live;

    /// Bytes the vector scratch texture occupies on the device right now.
    ScratchBytes => scratch_bytes, Group::Live;

    /// Frames whose vector content was refused because the scratch had no room for it.
    ///
    /// Every one of these is content the user asked for and did not get, drawn nowhere and
    /// reported nowhere else.
    VectorFramesDropped => vector_frames_dropped, Group::BackendNeutral;

    /// Primitives whose paint was re-anchored because the primitive moved.
    ///
    /// A ramp and a sampled image are read at the point being drawn, in the coordinates they were
    /// resolved against, so a rectangle carried forward to a new position has to say how far it
    /// came. Only a paint that is read at a point is counted: a flat fill is the same colour
    /// everywhere and moving it is not re-anchoring anything.
    PaintsReanchored => paints_reanchored, Group::BackendNeutral;

    /// Shaped paragraphs thrown away.
    ParagraphsForgotten => paragraphs_forgotten, Group::BackendNeutral;

    /// Times a box was marked as needing everything rebuilt rather than one thing.
    BoxesMarkedAllDirty => boxes_marked_all_dirty, Group::BackendNeutral;

    /// Shaped paragraphs dropped by name.
    ///
    /// Read beside [`Counter::ParagraphsForgotten`], which is the same event at document scale.
    /// One element's glyphs carrying a brush that can no longer be rewritten costs the paragraphs
    /// that element's text is in and no others, so this rises by a handful where that one rises by
    /// everything the window has ever shaped.
    ParagraphsEvicted => paragraphs_evicted, Group::BackendNeutral;

    /// Boxes whose layout was thrown away because the text in them has to be shaped again.
    BoxesReshaped => boxes_reshaped, Group::BackendNeutral;
}

impl Counter {
    /// Every counter that is a live count, in declaration order.
    ///
    /// The set a growth check walks: each of these is sampled early in a run and again late in the
    /// same run, and the two samples are required to be equal.
    pub fn live() -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(|counter| counter.group().is_live())
    }
}
