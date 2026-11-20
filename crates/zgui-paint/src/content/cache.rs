//! The content one window draws from, and the borrowed view one frame draws through.

mod images;
mod vectors;

use core::cell::RefCell;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, AtlasReport, TextureSink};
use zgui_geom::Device;
use zgui_layout::fragment::ParagraphId;
use zgui_layout::tree::store::LayoutStore;
use zgui_profile::{Counter, counter};
use zgui_scene::{ResourceGeneration, ResourceKey, ResourceRegistry};
use zgui_text::ShapedGlyphs;

use crate::content::cache::images::ImageContent;
use crate::content::cache::vectors::{FrameVectors, VectorContent};
use crate::content::glyphs::{GlyphCache, Rasterising};
use crate::emit::text::{GlyphPlacementSource, GlyphSource, PlacedGlyph};
use crate::walk::replay::hold::ResourceOwner;

/// The content one window keeps between frames: the images attached to replaced nodes, the vector
/// route caches and CPU layers, and a tile atlas.
#[derive(Debug)]
pub struct ContentCache {
    /// The tiles.
    atlas: Atlas,
    /// What each glyph rasterised to, last time anything asked.
    glyphs: GlyphCache,
    /// The vector route caches and the CPU layers.
    vectors: VectorContent,
    /// What is attached to each replaced node.
    images: ImageContent,
    /// Where each named raster is, for the sprites that were pushed naming one.
    ///
    /// Empty whenever every raster a frame drew was already placed as it was reached, which is what
    /// happens when rasterising is synchronous — so the ordinary frame files nothing here and its
    /// fix-up pass is a check on an empty list.
    registry: ResourceRegistry,
}

impl ContentCache {
    /// A cache allocating within `limits` and holding nothing.
    pub fn new(limits: AtlasLimits) -> Self {
        Self {
            atlas: Atlas::new(limits),
            glyphs: GlyphCache::default(),
            vectors: VectorContent::default(),
            images: ImageContent::default(),
            registry: ResourceRegistry::new(),
        }
    }

    /// Where each named raster currently is.
    ///
    /// What a frame's fix-up pass resolves its sprites through. See
    /// [`Scene::resolve_resources`](zgui_scene::Scene::resolve_resources).
    pub fn registry(&self) -> &ResourceRegistry {
        &self.registry
    }

    /// Which lifetime of this cache names are being handed out in.
    pub fn generation(&self) -> ResourceGeneration {
        self.registry.generation()
    }

    /// The name this cache would hold `key`'s content under right now.
    pub fn name(&self, key: AtlasKey) -> ResourceKey {
        ResourceKey::of(key, self.registry.generation())
    }

    /// Records where a named raster is, so a sprite that carries the name can be finished.
    pub fn place(&mut self, key: ResourceKey, tile: zgui_atlas::AtlasTile) {
        self.registry.place(key, tile);
    }

    /// Starts a frame, which is what makes eviction able to tell cold content from hot.
    pub fn begin_frame(&mut self) {
        self.atlas.begin_frame();
        self.vectors.begin_frame();
        // What the frame inherited, published before it adds anything. A cache that is supposed to
        // reach a working set and stay there says so here; one that never stops growing says that
        // here too, and says it as a count rather than as a byte figure an allocator has smeared.
        counter::set(Counter::AtlasEntriesLive, self.atlas.len() as u64);
        counter::set(Counter::GlyphEntriesLive, self.glyphs.held() as u64);
    }

    /// Ends a frame.
    ///
    /// This drops the prefetched glyphs the walk did not ask for and the layer rasters no drawing
    /// uses any more. Call it after [`ContentCache::flush`], for the reason
    /// [`ContentCache::enforce_soft_limit`] gives.
    pub fn end_frame(&mut self) {
        self.glyphs.drop_prefetched();
        self.vectors.end_frame();
    }

    /// What the cache is holding.
    pub fn report(&self) -> AtlasReport {
        self.atlas.report()
    }

    /// The tiles, for a caller asking what is cached.
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// How many bytes of rasters the cache is holding: the atlas, and the CPU vector layers.
    pub fn resident_bytes(&self) -> u64 {
        self.atlas.resident_bytes() + self.layer_bytes()
    }

    /// How many glyph keys have a remembered rasterisation.
    ///
    /// More than the atlas holds tiles for, and legitimately so: a glyph that rasterised to no
    /// pixels at all is remembered here and occupies no tile, which is what stops every space on
    /// the page running the face's hinting program again on every full repaint.
    pub fn glyphs_held(&self) -> usize {
        self.glyphs.held()
    }

    /// Sets the level [`ContentCache::enforce_soft_limit`] frees cold content back down to.
    ///
    /// `None` switches the budget off, which is the state a cache that was never given one is in:
    /// it keeps everything it has ever rasterised.
    pub fn set_soft_bytes(&mut self, bytes: Option<u64>) {
        self.atlas.set_soft_bytes(bytes);
    }

    /// Borrows everything one emit walk needs into a source of glyphs, of replaced content and of
    /// vector content.
    ///
    /// The vector caches and the layer rasters are written during the walk while the walk itself
    /// only reads, so the mutation stays behind this view. Nothing here is a device: texels reach
    /// the compositor through the frame's transaction.
    ///
    /// ```
    /// use zgui_layout::tree::store::LayoutStore;
    /// use zgui_paint::{ContentCache, FrameContent};
    /// use zgui_text::ShapedGlyphs;
    ///
    /// // The whole signature, written out: a fragment tree and shaped text. There is nowhere in it
    /// // for a texture sink or a renderer, and a call that offered one would not compile.
    /// let _: for<'a> fn(
    ///     &'a mut ContentCache,
    ///     &'a LayoutStore,
    ///     &'a dyn ShapedGlyphs,
    /// ) -> FrameContent<'a> = ContentCache::frame;
    /// ```
    pub fn frame<'a>(
        &'a mut self,
        store: &'a LayoutStore,
        shaped: &'a dyn ShapedGlyphs,
    ) -> FrameContent<'a> {
        FrameContent {
            store,
            shaped,
            images: &self.images,
            vectors: RefCell::new(&mut self.vectors),
            writing: RefCell::new(Rasterising {
                atlas: &mut self.atlas,
                named: Vec::new(),
            }),
        }
    }

    /// Writes every tile the atlas allocated since the last flush.
    ///
    /// # Errors
    ///
    /// [`AtlasError::Sink`](zgui_atlas::AtlasError::Sink) when the device refused a write. The
    /// remaining uploads stay queued, so a caller that recovers can flush again.
    pub fn flush(&mut self, sink: &mut dyn TextureSink) -> Result<u64, zgui_atlas::AtlasError> {
        let mut sink = sink;
        self.atlas.flush_uploads(&mut sink)
    }

    /// Frees the least recently used tiles nothing is holding.
    ///
    /// One generation, whatever that leaves resident. This is the step a caller that has just been
    /// refused an allocation takes; a caller enforcing a budget wants
    /// [`ContentCache::enforce_soft_limit`], which takes as many steps as the budget needs.
    pub fn evict(&mut self) -> zgui_atlas::Eviction {
        let mut removed = Vec::new();
        let freed = self.atlas.evict_least_recently_used_into(&mut removed);
        self.glyphs.forget_tiles(&removed);
        counter::add(Counter::AtlasTilesEvicted, freed.tiles as u64);
        freed
    }

    /// Frees cold content until the atlas is back under the soft limit its limits declare.
    ///
    /// Nothing happens without one — see
    /// [`AtlasLimits::soft_bytes`](zgui_atlas::AtlasLimits::soft_bytes) — so a window that has not
    /// stated a budget keeps everything it has ever rasterised, exactly as it did before there was
    /// a budget to state.
    ///
    /// Called once a frame, *after* the frame that used the content has been emitted and its
    /// uploads flushed. Before the emit walk it would be freeing tiles against last frame's
    /// working set; before the flush it would be discarding uploads the frame is about to draw
    /// from.
    pub fn enforce_soft_limit(&mut self) -> zgui_atlas::Eviction {
        let mut removed = Vec::new();
        let freed = self.atlas.evict_to_soft_limit_into(&mut removed);
        self.glyphs.forget_tiles(&removed);
        counter::add(Counter::AtlasTilesEvicted, freed.tiles as u64);
        freed
    }

    /// Drops every tile and destroys every texture, which a lost device makes necessary, and
    /// forgets the vector caches.
    ///
    /// The attached images survive, because their texels are held here rather than on the device.
    pub fn clear(&mut self) {
        self.atlas.clear();
        self.glyphs.clear();
        self.vectors.clear();
        // Every name handed out before now pointed into a texture that is about to stop existing,
        // so they stop being names. A sprite still carrying one resolves to nothing rather than to
        // whatever has since taken that content's place.
        self.registry.discard();
    }
}

/// One frame's view of a [`ContentCache`]: its shaped lines, its images and its vector caches.
pub struct FrameContent<'a> {
    /// The fragment tree, for the way back from a paragraph's name to its shaping key.
    store: &'a LayoutStore,
    /// Where positioned glyphs come from.
    shaped: &'a dyn ShapedGlyphs,
    /// What is attached to each replaced node.
    images: &'a ImageContent,
    /// The vector route caches and the CPU layers, behind a cell because emitting is an immutable
    /// walk.
    vectors: FrameVectors<'a>,
    /// The atlas, the glyph cache and their sink, behind a cell because emitting is an immutable
    /// walk.
    writing: RefCell<Rasterising<'a>>,
}

/// The atlas a frame holds tiles of, and the keys handed out during the walk.
pub(crate) struct Rasterising<'a> {
    /// The tiles.
    pub(crate) atlas: &'a mut Atlas,
    /// Geometry identities for small solid shapes stored in the monochrome atlas.
    pub(crate) vector_masks: &'a mut VectorMaskCache,
    /// Whole drawings rasterised into the image pool.
    pub(crate) vector_layers: &'a mut VectorLayerCache,
    /// Every atlas key handed out since the list was last drained.
    ///
    /// The list is how what a fragment *drew* becomes something that can be held: a primitive
    /// carries a rectangle of a texture and not the name the atlas knows it by, so once the walk
    /// has moved on there is no way back from the display list to the keys it depends on. Whoever
    /// is going to keep a range of that list has to be told the names while the placement is
    /// happening, and this is where they are collected.
    pub(crate) named: Vec<AtlasKey>,
}

impl GlyphSource for FrameContent<'_> {
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
        if let Some(key) = self.store.paragraph_key(paragraph) {
            self.shaped.visit_line(key, line, visit);
        }
    }

    fn brush(&self, brush: zgui_text::Brush) -> Option<zgui_text::TextPaint> {
        self.shaped.brush(brush)
    }
}

/// The atlas as a record owner outside a frame — the release half of eviction and teardown.
///
/// A frame releases through [`FrameContent`]; a budget enforcing between frames has no frame to
/// borrow, and this is the same atlas behind the same trait with nothing else attached.
pub struct TileOwner<'a>(RefCell<&'a mut Atlas>);

impl ContentCache {
    /// The atlas as a record owner, for release paths that run outside a frame.
    pub fn tile_owner(&mut self) -> TileOwner<'_> {
        TileOwner(RefCell::new(&mut self.atlas))
    }
}

impl ResourceOwner for TileOwner<'_> {
    fn take_named(&self, _out: &mut Vec<AtlasKey>) {}

    fn retain(&self, key: AtlasKey) {
        self.0.borrow_mut().retain(key);
    }

    fn release(&self, key: AtlasKey) {
        self.0.borrow_mut().release(key);
    }

    fn contains(&self, key: AtlasKey) -> bool {
        self.0.borrow().contains(key)
    }

    fn refusals(&self) -> u64 {
        self.0.borrow().refusals()
    }
}

impl ResourceOwner for FrameContent<'_> {
    fn take_named(&self, out: &mut Vec<AtlasKey>) {
        out.append(&mut self.writing.borrow_mut().named);
    }

    fn retain(&self, key: AtlasKey) {
        self.writing.borrow_mut().atlas.retain(key);
    }

    fn release(&self, key: AtlasKey) {
        self.writing.borrow_mut().atlas.release(key);
    }

    fn contains(&self, key: AtlasKey) -> bool {
        self.writing.borrow().atlas.contains(key)
    }

    fn refusals(&self) -> u64 {
        self.writing.borrow().atlas.refusals()
    }
}
