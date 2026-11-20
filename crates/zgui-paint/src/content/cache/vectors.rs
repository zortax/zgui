//! The vector half of a window's content: route caches and CPU layers.

use core::cell::{RefCell, RefMut};

use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_scene::VectorId;

use crate::content::cache::{ContentCache, FrameContent};
use crate::content::vectors::{
    LayerAnswer, LayerPixels, LayerRequest, VectorLayerCache, VectorLayerSource, VectorMaskSource,
    VectorRouteCache,
};
use crate::emit::record::ImageKey;

/// The route caches and the CPU layers of one window.
#[derive(Debug, Default)]
pub(crate) struct VectorContent {
    /// The route declines and what the routes found.
    routes: VectorRouteCache,
    /// Whole drawings rasterised on the CPU.
    layers: VectorLayerCache,
}

impl VectorContent {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.routes.begin_frame();
        self.layers.begin_frame();
    }

    /// Ends a frame.
    pub(crate) fn end_frame(&mut self) {
        self.routes.end_frame();
        self.layers.end_frame();
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.routes.clear();
        self.layers.clear();
    }
}

impl ContentCache {
    /// Says whether the general vector rasteriser is built.
    ///
    /// While it is, the layer route waits for a drawing to hold still and budgets new layers
    /// tighter, because the general route draws a changing drawing at no setup cost. A cache that
    /// is never told assumes cold.
    pub fn set_vector_raster_ready(&mut self, ready: bool) {
        self.vectors.layers.set_raster_ready(ready);
    }

    /// How many bytes the CPU vector layers hold.
    pub fn layer_bytes(&self) -> u64 {
        self.vectors.layers.bytes()
    }

    /// How many of those bytes this frame drew.
    pub fn layer_pinned_bytes(&self) -> u64 {
        self.vectors.layers.pinned_bytes()
    }

    /// How many CPU vector layers are held.
    pub fn layers_held(&self) -> usize {
        self.vectors.layers.len()
    }

    /// How many layer images the cache has answered, monotonic.
    pub fn layer_hits(&self) -> u64 {
        self.vectors.layers.hits()
    }

    /// The CPU layer for `request`, outside a frame's walk.
    #[doc(hidden)]
    pub fn layer(&mut self, request: LayerRequest<'_>) -> LayerAnswer {
        self.vectors.layers.layer(request)
    }

    /// Rasterises the tiles shown since the last settle where `damage` needs them, outside a
    /// frame's walk, hands each raster to `write`, and returns the rectangles of the tiles it
    /// deferred.
    #[doc(hidden)]
    pub fn settle_layers(
        &mut self,
        viewport: Size<i32, Device>,
        damage: &zgui_bits::DamageSet,
        write: &mut dyn FnMut(&LayerPixels),
    ) -> Vec<Rect<i32, Device>> {
        self.vectors.layers.settle(viewport, damage, write)
    }

    /// Removes the least recently drawn CPU vector layers this frame did not draw until `bytes`
    /// have gone, and reports how many went.
    pub fn evict_layers(&mut self, bytes: u64) -> u64 {
        self.vectors.layers.evict(bytes)
    }
}

impl<'a> FrameContent<'a> {
    /// Returns the vector half for writing.
    fn vectors(&self) -> RefMut<'_, &'a mut VectorContent> {
        self.vectors.borrow_mut()
    }
}

/// The vector half of a [`FrameContent`].
pub(crate) type FrameVectors<'a> = RefCell<&'a mut VectorContent>;

impl VectorMaskSource for FrameContent<'_> {
    fn analytic(&self, owner: VectorId) -> bool {
        self.vectors().routes.analytic_allowed(owner)
    }

    fn analytic_declined(&self, owner: VectorId) {
        self.vectors().routes.note_analytic_declined(owner);
    }

    fn marks(&self, owner: VectorId) -> bool {
        self.vectors().routes.marks_allowed(owner)
    }

    fn marks_declined(&self, owner: VectorId) {
        self.vectors().routes.note_marks_declined(owner);
    }

    fn recognitions(&self) -> Option<RefMut<'_, crate::content::vectors::Recognitions>> {
        Some(RefMut::map(self.vectors(), |vectors| {
            &mut vectors.routes.recognitions
        }))
    }

    fn payloads(&self) -> Option<RefMut<'_, crate::content::vectors::MarkPayloads>> {
        Some(RefMut::map(self.vectors(), |vectors| {
            &mut vectors.routes.payloads
        }))
    }

    fn layers(&self) -> Option<&dyn VectorLayerSource> {
        Some(self)
    }

    fn path_glyphs(&self, owner: VectorId) -> bool {
        self.vectors().routes.path_glyphs_allowed(owner)
    }

    fn path_glyphs_declined(&self, owner: VectorId) {
        self.vectors().routes.note_path_glyphs_declined(owner);
    }

    fn glyph_splits(&self) -> Option<RefMut<'_, crate::content::vectors::Splits>> {
        Some(RefMut::map(self.vectors(), |vectors| {
            &mut vectors.routes.splits
        }))
    }
}

impl VectorLayerSource for FrameContent<'_> {
    fn layer(&self, request: LayerRequest<'_>) -> LayerAnswer {
        self.vectors().layers.layer(request)
    }

    fn per_shape_suffices(&self, owner: VectorId) {
        self.vectors().layers.per_shape_suffices(owner);
    }

    fn tiles_alive(&self, id: u64) -> bool {
        self.vectors().layers.tiles_alive(id)
    }

    fn shown(&self, key: ImageKey, admitted: Option<Rect<DevicePx, Device>>) {
        self.vectors().layers.shown(key, admitted);
    }

    fn settle(
        &self,
        viewport: Size<i32, Device>,
        damage: &zgui_bits::DamageSet,
        write: &mut dyn FnMut(&LayerPixels),
    ) -> Vec<Rect<i32, Device>> {
        self.vectors().layers.settle(viewport, damage, write)
    }
}
