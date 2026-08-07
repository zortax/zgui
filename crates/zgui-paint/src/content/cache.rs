//! The atlas a window's glyphs and images share, and the borrowed view one frame draws through.

use core::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;
use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, AtlasReport, TextureSink};
use zgui_dom::host::ReplacedId;
use zgui_geom::{Device, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_layout::tree::store::LayoutStore;
use zgui_profile::{Counter, counter};
use zgui_scene::ExternalTextureId;
use zgui_scene::{ResourceGeneration, ResourceKey, ResourceRegistry};
use zgui_text::{GlyphRaster, RasterPath, ShapedGlyphs};

use crate::content::glyphs::OutlineGlyph;
use crate::content::glyphs::{GlyphCache, Rasterising};
use crate::content::images::{Content, ImageError};
use crate::content::vectors::{VectorMaskCache, VectorMaskRequest, VectorMaskSource};
use crate::emit::replaced::Source;
use crate::emit::text::{GlyphRequest, GlyphRun, GlyphSource, PlacedGlyph, RunContent};
use crate::walk::order::ReplacedSource;
use crate::walk::replay::hold::ResourceOwner;

/// Everything one window has rasterised, held between frames.
///
/// One atlas serves both halves rather than one each, because they compete for the same texture
/// memory and a budget split in advance is a budget wrong in one direction or the other: a document
/// that is all text and a document that is all pictures are both ordinary.
///
/// The glyph cache sits beside the atlas rather than inside it, because it answers a wider question
/// than the atlas can: not only where a glyph's pixels are, but where they go and whether there are
/// any. Without it a frame that already held every tile still rasterised every glyph.
#[derive(Debug)]
pub struct ContentCache {
    /// The tiles.
    atlas: Atlas,
    /// What each glyph rasterised to, last time anything asked.
    glyphs: GlyphCache,
    /// Geometry identities for small solid vector masks in the monochrome atlas.
    vector_masks: VectorMaskCache,
    /// What is attached to each replaced node.
    images: FxHashMap<ReplacedId, Content>,
    /// How many times a frame has resolved a replaced node to the content held for it.
    ///
    /// Monotonic and never reset, so two readings subtracted say whether anything drew a picture
    /// between two moments. In a cell because the walk that resolves them is an immutable reader.
    image_hits: Cell<u64>,
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
            vector_masks: VectorMaskCache::default(),
            images: FxHashMap::default(),
            image_hits: Cell::new(0),
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
        // What the frame inherited, published before it adds anything. A cache that is supposed to
        // reach a working set and stay there says so here; one that never stops growing says that
        // here too, and says it as a count rather than as a byte figure an allocator has smeared.
        counter::set(Counter::AtlasEntriesLive, self.atlas.len() as u64);
        counter::set(Counter::GlyphEntriesLive, self.glyphs.held() as u64);
    }

    /// What the cache is holding.
    pub fn report(&self) -> AtlasReport {
        self.atlas.report()
    }

    /// The tiles, for a caller asking what is cached.
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// How many bytes of texture memory the cache is holding.
    pub fn resident_bytes(&self) -> u64 {
        self.atlas.resident_bytes()
    }

    /// How many decoded bytes are referenced by replaced-node attachments.
    ///
    /// Separate from [`ContentCache::resident_bytes`] and not part of it: those are the device's
    /// texture memory and these are host references. Shared attachments are counted once per node,
    /// so this is an attachment diagnostic rather than the window's allocation total; the runtime
    /// image loader owns and reports that deduplicated total. Externally owned textures count for
    /// nothing.
    pub fn image_bytes(&self) -> u64 {
        self.images.values().map(Content::held_bytes).sum()
    }

    /// How many glyph keys have a remembered rasterisation.
    ///
    /// More than the atlas holds tiles for, and legitimately so: a glyph that rasterised to no
    /// pixels at all is remembered here and occupies no tile, which is what stops every space on
    /// the page running the face's hinting program again on every full repaint.
    pub fn glyphs_held(&self) -> usize {
        self.glyphs.held()
    }

    /// How many times a frame has resolved a replaced node to the content held for it.
    pub fn image_hits(&self) -> u64 {
        self.image_hits.get()
    }

    /// Detaches every image, and reports how many that threw away.
    ///
    /// This cache cannot put them back by itself. A runtime loader may retain their source and
    /// re-decode them; directly attached texels and external textures need their owner to attach
    /// them again.
    pub fn forget_images(&mut self) -> usize {
        let held = self.images.len();
        self.images.clear();
        held
    }

    /// Sets the level [`ContentCache::enforce_soft_limit`] frees cold content back down to.
    ///
    /// `None` switches the budget off, which is the state a cache that was never given one is in:
    /// it keeps everything it has ever rasterised.
    pub fn set_soft_bytes(&mut self, bytes: Option<u64>) {
        self.atlas.set_soft_bytes(bytes);
    }

    /// Attaches decoded texels to a replaced node.
    ///
    /// The texels are premultiplied, gamma-encoded sRGB, four bytes per pixel, tightly packed with
    /// the top row first. They are held rather than only uploaded, so that an eviction or a lost
    /// device costs an upload rather than a re-decode.
    ///
    /// # Errors
    ///
    /// [`ImageError::WrongByteCount`] when the buffer does not match the extent.
    pub fn set_image(
        &mut self,
        id: ReplacedId,
        size: Size<u32, Device>,
        texels: Vec<u8>,
    ) -> Result<(), ImageError> {
        let content = Content::decoded(id, size, texels)?;
        self.images.insert(id, content);
        Ok(())
    }

    /// Attaches already-shared texels to a replaced node, under a caller-chosen atlas handle.
    ///
    /// The sharing is the point, twice over: every node attached under one `handle` resolves to
    /// **one atlas tile**, and the texels arrive behind an [`Arc`](std::sync::Arc) rather than by
    /// value, so a loader that decoded a file once serves any number of elements showing it
    /// without copying a byte. The texels obey the same contract as
    /// [`set_image`](ContentCache::set_image): premultiplied, gamma-encoded sRGB, four bytes per
    /// pixel, top row first.
    ///
    /// # Errors
    ///
    /// [`ImageError::WrongByteCount`] when the buffer does not match the extent.
    pub fn set_image_shared(
        &mut self,
        id: ReplacedId,
        handle: u64,
        size: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
    ) -> Result<(), ImageError> {
        let content = Content::shared(handle, size, texels)?;
        self.images.insert(id, content);
        Ok(())
    }

    /// Attaches a texture this framework does not own to a replaced node.
    pub fn set_external(&mut self, id: ReplacedId, texture: ExternalTextureId) {
        self.images.insert(id, Content::External(texture));
    }

    /// Detaches whatever was attached to a replaced node.
    pub fn remove_image(&mut self, id: ReplacedId) {
        self.images.remove(&id);
    }

    /// Borrows everything one emit walk needs into a source of glyphs and of replaced content.
    ///
    /// The borrow is the point: the atlas is written during the walk — a glyph nothing has drawn
    /// before is rasterised the moment it is reached — while the walk itself only reads. Holding
    /// the mutation behind this view is what keeps the emit walk a pure reader of the document.
    ///
    /// **Nothing here is a device.** Rasterising, allocating a tile and growing the atlas are all
    /// decided against this window's own state; the texels and the textures they go into are queued
    /// and performed by [`ContentCache::flush`], which is the only call in the whole of this crate
    /// that a renderer has to be reachable from.
    ///
    /// ```
    /// use zgui_layout::tree::store::LayoutStore;
    /// use zgui_paint::{ContentCache, FrameContent};
    /// use zgui_text::{GlyphRaster, ShapedGlyphs};
    ///
    /// // The whole signature, written out: a fragment tree, shaped text, and a rasteriser. There
    /// // is nowhere in it for a texture sink or a renderer, and a call that offered one would not
    /// // compile.
    /// let _: for<'a> fn(
    ///     &'a mut ContentCache,
    ///     &'a LayoutStore,
    ///     &'a dyn ShapedGlyphs,
    ///     &'a dyn GlyphRaster,
    /// ) -> FrameContent<'a> = ContentCache::frame;
    /// ```
    pub fn frame<'a>(
        &'a mut self,
        store: &'a LayoutStore,
        shaped: &'a dyn ShapedGlyphs,
        raster: &'a dyn GlyphRaster,
    ) -> FrameContent<'a> {
        FrameContent {
            store,
            shaped,
            raster,
            images: &self.images,
            image_hits: &self.image_hits,
            writing: RefCell::new(Rasterising {
                glyphs: &mut self.glyphs,
                atlas: &mut self.atlas,
                vector_masks: &mut self.vector_masks,
                named: Vec::new(),
            }),
        }
    }

    /// Writes every tile this frame allocated.
    ///
    /// Uploads are queued while the walk runs and leave in one batch, so this has to be called
    /// after emitting and before drawing: a sprite reading a tile whose texels were never written
    /// samples whatever the texture held before, which is another glyph's pixels rather than
    /// nothing.
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
        self.vector_masks.forget_tiles(&removed);
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
        self.vector_masks.forget_tiles(&removed);
        counter::add(Counter::AtlasTilesEvicted, freed.tiles as u64);
        freed
    }

    /// Drops every tile and destroys every texture, which a lost device makes necessary.
    ///
    /// The attached images survive, because their texels are held here rather than on the device:
    /// what is lost is the upload, and the next frame that draws one performs it again.
    pub fn clear(&mut self) {
        self.atlas.clear();
        self.glyphs.clear();
        self.vector_masks.clear();
        // Every name handed out before now pointed into a texture that is about to stop existing,
        // so they stop being names. A sprite still carrying one resolves to nothing rather than to
        // whatever has since taken that content's place.
        self.registry.discard();
    }
}

/// One frame's view of a [`ContentCache`]: where its glyphs and its images are.
///
/// Rasterisation happens on demand, as the emit walk reaches each line, which is what makes a
/// document whose thousandth row is off screen cost nothing for that row's glyphs.
pub struct FrameContent<'a> {
    /// The fragment tree, for the way back from a paragraph's name to its shaping key.
    store: &'a LayoutStore,
    /// Where positioned glyphs come from.
    shaped: &'a dyn ShapedGlyphs,
    /// What turns a glyph into pixels.
    raster: &'a dyn GlyphRaster,
    /// What is attached to each replaced node.
    images: &'a FxHashMap<ReplacedId, Content>,
    /// Where a resolved picture is counted, so that a budget can tell a cache nothing draws from
    /// from one a picture on the screen is being served out of every frame.
    image_hits: &'a Cell<u64>,
    /// The atlas, the glyph cache and their sink, behind a cell because emitting is an immutable
    /// walk.
    writing: RefCell<Rasterising<'a>>,
}

impl GlyphSource for FrameContent<'_> {
    /// Answers in tiles or in curves, according to what the run and the surface make of each other.
    ///
    /// This is the one place the promotion happens, and it happens *before* anything is rasterised:
    /// a run that leaves the atlas never allocates a tile, never uploads one, and never touches the
    /// glyph cache, so a page of turned headings does not evict the body text behind it.
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        request: GlyphRequest,
        visit: &mut dyn FnMut(GlyphRun<'_>),
    ) {
        let Some(key) = self.store.paragraph_key(paragraph) else {
            return;
        };
        let mut placed: Vec<PlacedGlyph> = Vec::new();
        let mut outlined: Vec<OutlineGlyph> = Vec::new();
        self.shaped.visit_line(key, line, &mut |run| {
            if run.raster_path(request.surface) == RasterPath::Vector {
                outlined.clear();
                crate::content::glyphs::curve::place(
                    self.raster,
                    &run,
                    request.origin,
                    &mut outlined,
                );
                if !outlined.is_empty() {
                    visit(GlyphRun {
                        content: RunContent::Outlines(&outlined),
                        // A run drawn as curves has no tiles; the format is what a tile's bytes
                        // mean, and there are none.
                        format: crate::content::glyphs::format_of(run.raster_style(false)),
                        paint: run.brush,
                        synthetic_bold: run.synthetic_bold * run.size,
                    });
                    return;
                }
                // A face that has no curves at all — a bitmap-only face — is drawn from tiles
                // however large it is and whatever the transform does to it. A resampled letter is
                // a letter; the alternative is a run that draws nothing, which is what a promotion
                // that could not be honoured would silently be.
            }
            let style = run.raster_style(request.subpixel);
            placed.clear();
            {
                crate::content::glyphs::place(
                    &mut self.writing.borrow_mut(),
                    self.raster,
                    &run,
                    style,
                    request.origin,
                    &mut placed,
                );
            }
            if placed.is_empty() {
                return;
            }
            visit(GlyphRun {
                content: RunContent::Tiles(&placed),
                format: crate::content::glyphs::format_of(style),
                paint: run.brush,
                synthetic_bold: 0.0,
            });
        });
    }
}

impl ReplacedSource for FrameContent<'_> {
    fn source(&self, id: ReplacedId) -> Option<Source> {
        let content = self.images.get(&id)?;
        self.image_hits.set(self.image_hits.get() + 1);
        let mut writing = self.writing.borrow_mut();
        let Rasterising { atlas, named, .. } = &mut *writing;
        let (source, key) = crate::content::images::source_of(atlas, content)?;
        // A picture is one tile where a line is fifty, and it is held for the same reason: a
        // replayed range draws it without asking for it, so nothing else says it is still there.
        if let Some(key) = key {
            named.push(key);
        }
        Some(source)
    }
}

impl VectorMaskSource for FrameContent<'_> {
    fn vector_mask(
        &self,
        request: VectorMaskRequest<'_>,
    ) -> Option<crate::content::vectors::VectorMask> {
        let mut writing = self.writing.borrow_mut();
        let Rasterising {
            atlas,
            vector_masks,
            named,
            ..
        } = &mut *writing;
        let mask = vector_masks.tile_for(atlas, request)?;
        named.push(mask.key);
        Some(mask)
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
}
