//! Replaying a previous frame's recorded operations.

use core::ops::Range;

use zgui_geom::{Device, DevicePx, Size};
use zgui_profile::{Counter, counter};

use crate::prim::PrimitiveKind;
use crate::scene::Scene;

impl Scene {
    /// Re-emits the previous frame's operations in `range`, offset by `by`, and returns the range
    /// they occupy in this frame's log.
    ///
    /// This is what a fragment that has not changed costs instead of being re-emitted: a scrolled
    /// list's five hundred rows are copied forward with one translation each rather than
    /// re-derived from their styles and geometry. A fragment records the range its primitives
    /// occupied — that is what [`Scene::ops`] is for — and hands it back here next frame.
    ///
    /// Draw order is **not** replayed: every re-emitted primitive is pushed through the ordinary
    /// path, so it is ordered against *this* frame's neighbours. Order depends on what else is on
    /// the surface, and carrying a stale one forward would put a row underneath something newly
    /// drawn over it.
    ///
    /// Group markers and vector items are not replayed and are skipped: a marker's order comes from
    /// a barrier rather than from geometry, and vector content is planned into passes rather than
    /// emitted as instances. A caller re-emits those directly.
    ///
    /// An out-of-bounds range replays nothing, which is the right answer for a fragment whose cache
    /// refers to a frame that no longer exists.
    pub fn replay(&mut self, range: Range<u32>, by: Size<DevicePx, Device>) -> Range<u32> {
        let start = self.ops.len() as u32;
        let (first, last) = (range.start as usize, range.end as usize);
        // Out of bounds replays nothing at all rather than the prefix that happens to exist: a
        // range naming a frame that is gone names none of it.
        if first > last || last > self.retained_ops.len() {
            return start..start;
        }

        // Indexed rather than collected. `PaintOp` is `Copy`, so one entry is copied out per
        // iteration and the borrow of the retained log ends before the push that wants `&mut
        // self` — where collecting the range first cost one allocation per replayed fragment per
        // frame, which for a scrolling list is one per row for as long as the scroll lasts.
        for position in 0..last - first {
            let op = self.retained_ops[first + position];
            let index = op.index as usize;
            // Where this primitive's log entry will land, so the name it was *originally* pushed
            // under can be put back over the one the ordinary push path just wrote. A replayed
            // primitive was drawn through the coordinate system that was live when it was encoded,
            // and renaming it here would make the check that says so agree with itself for ever.
            let logged = self.ops.len();
            let recorded = self
                .retained_spaces
                .get(range.start as usize + position)
                .copied()
                .flatten();
            match op.kind {
                PrimitiveKind::Quad => {
                    let Some(mut quad) = self.retained.quads.get(index).copied() else {
                        continue;
                    };
                    translate(&mut quad.bounds, by);
                    // The rectangle moved; its paints did not. A ramp and a sampled image are both
                    // read at the point being drawn, in the coordinates they were resolved
                    // against, so the displacement travels in the instance and the sampler undoes
                    // it. Nothing is interned, nothing is rewritten, and the paint table is the
                    // same length after a thousand scroll steps as before the first.
                    quad.reanchor_paint(by);
                    if quad.samples_its_paint() && (by.width.0 != 0.0 || by.height.0 != 0.0) {
                        counter::bump(Counter::PaintsReanchored);
                    }
                    self.push_quad(quad);
                }
                PrimitiveKind::Shadow => {
                    let Some(mut shadow) = self.retained.shadows.get(index).copied() else {
                        continue;
                    };
                    translate(&mut shadow.bounds, by);
                    translate(&mut shadow.element_bounds, by);
                    self.push_shadow(shadow);
                }
                PrimitiveKind::Decoration => {
                    let Some(mut decoration) = self.retained.decorations.get(index).copied() else {
                        continue;
                    };
                    translate(&mut decoration.bounds, by);
                    self.push_decoration(decoration);
                }
                PrimitiveKind::MonoSprite => {
                    let Some(mut sprite) = self.retained.mono_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    self.push_mono_sprite(sprite);
                }
                PrimitiveKind::SubpixelSprite => {
                    let Some(mut sprite) = self.retained.subpixel_sprites.get(index).copied()
                    else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    self.push_subpixel_sprite(sprite);
                }
                PrimitiveKind::ColorSprite => {
                    let Some(mut sprite) = self.retained.color_sprites.get(index).copied() else {
                        continue;
                    };
                    translate(&mut sprite.bounds, by);
                    self.push_color_sprite(sprite);
                }
                PrimitiveKind::External => {
                    let Some(mut external) = self.retained.externals.get(index).copied() else {
                        continue;
                    };
                    external.bounds = external.bounds.translate(by);
                    self.push_external(external);
                }
                PrimitiveKind::Backdrop => {
                    let Some(mut backdrop) = self.retained.backdrops.get(index).cloned() else {
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

    /// How many operations the previous frame recorded.
    ///
    /// A recorded range that ends past this belongs to a frame that no longer exists and replays
    /// nothing.
    pub fn retained_ops(&self) -> usize {
        self.retained_ops.len()
    }
}

/// Moves an `[x, y, width, height]` field.
fn translate(bounds: &mut [f32; 4], by: Size<DevicePx, Device>) {
    bounds[0] += by.width.0;
    bounds[1] += by.height.0;
}
