//! Painting an element with an effect.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_paint::content::custom::ScenePainter;

use crate::handle::{ShaderHandle, ShaderParams};

/// Drawing an effect through the painter a custom element is given.
pub trait ShaderPainterExt {
    /// Fills `rect` with `handle`'s effect, rounded by `corner_radius`.
    ///
    /// Coordinates reach the effect measured from `rect`'s own corner, so an effect is written
    /// against the box it is drawn in rather than against wherever the element ended up. The
    /// fragment's clip, its transform and the alpha folded in from the groups above are applied to
    /// what the effect returns.
    fn effect<P: ShaderParams>(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        handle: &ShaderHandle<P>,
    );
}

impl ShaderPainterExt for ScenePainter<'_> {
    fn effect<P: ShaderParams>(
        &mut self,
        rect: Rect<DevicePx, Device>,
        corner_radius: f32,
        handle: &ShaderHandle<P>,
    ) {
        self.shade(rect, corner_radius, handle.id(), handle.params());
    }
}
