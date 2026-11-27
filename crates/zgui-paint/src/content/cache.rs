//! The content one window draws from, and the borrowed view one frame draws through.

mod images;
mod vectors;

use core::cell::RefCell;

use zgui_layout::fragment::ParagraphId;
use zgui_layout::tree::store::LayoutStore;
use zgui_text::ShapedGlyphs;

use crate::content::cache::images::ImageContent;
use crate::content::cache::vectors::{FrameVectors, VectorContent};
use crate::emit::text::GlyphSource;

/// The content one window keeps between frames: the images attached to replaced nodes, and the
/// vector route caches and CPU layers.
#[derive(Debug)]
pub struct ContentCache {
    /// The vector route caches and the CPU layers.
    vectors: VectorContent,
    /// What is attached to each replaced node.
    images: ImageContent,
}

impl ContentCache {
    /// Returns a cache holding nothing.
    pub fn new() -> Self {
        Self {
            vectors: VectorContent::default(),
            images: ImageContent::default(),
        }
    }

    /// Starts a frame, which is what makes eviction able to tell cold content from hot.
    pub fn begin_frame(&mut self) {
        self.vectors.begin_frame();
    }

    /// Ends a frame, dropping the layer rasters no drawing uses any more.
    pub fn end_frame(&mut self) {
        self.vectors.end_frame();
    }

    /// Returns how many bytes of CPU vector layers the cache is holding.
    pub fn resident_bytes(&self) -> u64 {
        self.layer_bytes()
    }

    /// Borrows everything one emit walk needs into a source of glyphs, of replaced content and of
    /// vector content.
    ///
    /// The vector caches and the layer rasters are written during the walk while the walk itself
    /// only reads, so the mutation stays behind this view. Texels reach the compositor through the
    /// frame's transaction.
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
        }
    }

    /// Forgets the vector caches, for a device that was lost.
    ///
    /// The attached images survive, because their texels are held here rather than on the device.
    pub fn clear(&mut self) {
        self.vectors.clear();
    }
}

impl Default for ContentCache {
    fn default() -> Self {
        Self::new()
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
