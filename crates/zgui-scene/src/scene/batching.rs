//! What the batch iterator asks the scene.

use zgui_geom::{Device, DevicePx, Rect};

use crate::batch::Batch;
use crate::id::DrawOrder;
use crate::ops::PaintOp;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;
use crate::spatial::SpatialId;

impl Scene {
    /// The draw-order permutation of `kind`'s array: position in draw order to index in the array.
    ///
    /// The arrays keep push order, and a batch's range is a range of this list — a renderer that
    /// draws a batch reads the array through it, which is what the shaders' remap binding does.
    /// Kinds whose batches carry array indices directly — vector composites and group markers —
    /// answer an empty slice.
    pub fn remap(&self, kind: PrimitiveKind) -> &[u32] {
        match kind {
            PrimitiveKind::Quad => &self.remap.quads,
            PrimitiveKind::Shaded => &self.remap.shaded,
            PrimitiveKind::Shadow => &self.remap.shadows,
            PrimitiveKind::Decoration => &self.remap.decorations,
            PrimitiveKind::MonoSprite => &self.remap.mono_sprites,
            PrimitiveKind::SubpixelSprite => &self.remap.subpixel_sprites,
            PrimitiveKind::ColorSprite => &self.remap.color_sprites,
            PrimitiveKind::External => &self.remap.externals,
            PrimitiveKind::Backdrop => &self.remap.backdrops,
            PrimitiveKind::Vector | PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => &[],
        }
    }

    /// The array index of the primitive at draw-order `position` of `kind`, or `None` past the
    /// end.
    fn slot(&self, kind: PrimitiveKind, position: usize) -> Option<usize> {
        self.remap(kind).get(position).map(|slot| *slot as usize)
    }

    /// What the primitive one log entry names paints.
    ///
    /// Group markers report what the group writes; a vector item is never asked, because it is
    /// handled as a pass rather than as a primitive.
    pub(crate) fn ink_of(&self, op: PaintOp) -> Rect<DevicePx, Device> {
        let index = op.index as usize;
        match op.kind {
            PrimitiveKind::Quad => self.primitives.quads[index].ink(),
            PrimitiveKind::Shaded => self.primitives.shaded[index].ink(),
            PrimitiveKind::Shadow => self.primitives.shadows[index].ink(),
            PrimitiveKind::Decoration => self.primitives.decorations[index].ink(),
            PrimitiveKind::MonoSprite => self.primitives.mono_sprites[index].ink(),
            PrimitiveKind::SubpixelSprite => self.primitives.subpixel_sprites[index].ink(),
            PrimitiveKind::ColorSprite => self.primitives.color_sprites[index].ink(),
            PrimitiveKind::External => self.primitives.externals[index].ink(),
            PrimitiveKind::Backdrop => self.primitives.backdrops[index].bounds,
            PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {
                self.primitives.groups[index].bounds
            }
            PrimitiveKind::Vector => self.primitives.vectors[index].ink,
        }
    }

    /// The coordinate system the primitive one log entry names is drawn under.
    ///
    /// Resolved from the slot the primitive carries rather than from anything recorded beside it,
    /// so it is answerable on every run and not only on one that asked for the dependency check.
    /// `None` for a primitive that names no coordinate system, and for a slot nothing occupies.
    pub(crate) fn space_of_op(&self, op: PaintOp) -> Option<SpatialId> {
        let index = op.index as usize;
        let slot = match op.kind {
            PrimitiveKind::Quad => self.primitives.quads.get(index)?.transform,
            PrimitiveKind::Shaded => self.primitives.shaded.get(index)?.transform,
            PrimitiveKind::Shadow => self.primitives.shadows.get(index)?.transform,
            PrimitiveKind::Decoration => self.primitives.decorations.get(index)?.transform,
            PrimitiveKind::MonoSprite => self.primitives.mono_sprites.get(index)?.transform,
            PrimitiveKind::SubpixelSprite => self.primitives.subpixel_sprites.get(index)?.transform,
            PrimitiveKind::ColorSprite => self.primitives.color_sprites.get(index)?.transform,
            PrimitiveKind::External => self.primitives.externals.get(index)?.transform.index(),
            PrimitiveKind::Vector => return self.primitives.vectors.get(index)?.transform,
            PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {
                return self.primitives.groups.get(index)?.transform;
            }
            PrimitiveKind::Backdrop => return None,
        };
        self.spatial.at(slot)
    }

    /// The draw order of the primitive one log entry names, read from its array directly.
    ///
    /// The log's indices are array indices for the whole of the frame, so this bypasses the
    /// remap — which may not even be sorted yet when a caller sweeps the emission stream.
    pub(crate) fn order_of_op(&self, op: PaintOp) -> Option<DrawOrder> {
        let index = op.index as usize;
        match op.kind {
            PrimitiveKind::Quad => self.primitives.quads.get(index).map(|held| held.order),
            PrimitiveKind::Shaded => self.primitives.shaded.get(index).map(|held| held.order),
            PrimitiveKind::Shadow => self.primitives.shadows.get(index).map(|held| held.order),
            PrimitiveKind::Decoration => self
                .primitives
                .decorations
                .get(index)
                .map(|held| held.order),
            PrimitiveKind::MonoSprite => self
                .primitives
                .mono_sprites
                .get(index)
                .map(|held| held.order),
            PrimitiveKind::SubpixelSprite => self
                .primitives
                .subpixel_sprites
                .get(index)
                .map(|held| held.order),
            PrimitiveKind::ColorSprite => self
                .primitives
                .color_sprites
                .get(index)
                .map(|held| held.order),
            PrimitiveKind::External => self.primitives.externals.get(index).map(|held| held.order),
            PrimitiveKind::Backdrop => self.primitives.backdrops.get(index).map(|held| held.order),
            PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {
                self.primitives.groups.get(index).map(|held| held.order)
            }
            PrimitiveKind::Vector => self.primitives.vectors.get(index).map(|held| held.order),
        }
    }

    /// The draw order of the primitive at draw-order `position` of `kind`, or `None` past the
    /// end.
    pub(crate) fn order_at(&self, kind: PrimitiveKind, position: usize) -> Option<DrawOrder> {
        match kind {
            PrimitiveKind::Quad => {
                let slot = self.slot(kind, position)?;
                self.primitives.quads.get(slot).map(|held| held.order)
            }
            PrimitiveKind::Shaded => {
                let slot = self.slot(kind, position)?;
                self.primitives.shaded.get(slot).map(|held| held.order)
            }
            PrimitiveKind::Shadow => {
                let slot = self.slot(kind, position)?;
                self.primitives.shadows.get(slot).map(|held| held.order)
            }
            PrimitiveKind::Decoration => {
                let slot = self.slot(kind, position)?;
                self.primitives.decorations.get(slot).map(|held| held.order)
            }
            PrimitiveKind::MonoSprite => {
                let slot = self.slot(kind, position)?;
                self.primitives
                    .mono_sprites
                    .get(slot)
                    .map(|held| held.order)
            }
            PrimitiveKind::SubpixelSprite => {
                let slot = self.slot(kind, position)?;
                self.primitives
                    .subpixel_sprites
                    .get(slot)
                    .map(|held| held.order)
            }
            PrimitiveKind::ColorSprite => {
                let slot = self.slot(kind, position)?;
                self.primitives
                    .color_sprites
                    .get(slot)
                    .map(|held| held.order)
            }
            PrimitiveKind::External => {
                let slot = self.slot(kind, position)?;
                self.primitives.externals.get(slot).map(|held| held.order)
            }
            PrimitiveKind::Backdrop => {
                let slot = self.slot(kind, position)?;
                self.primitives.backdrops.get(slot).map(|held| held.order)
            }
            PrimitiveKind::GroupStart => self.marker_order(position, true),
            PrimitiveKind::GroupEnd => self.marker_order(position, false),
            // A composite belongs above every item of its pass, not where any one of them draws.
            PrimitiveKind::Vector => self
                .pass_plan()
                .passes
                .get(position)
                .map(|pass| pass.composite_order),
        }
    }

    /// The longest run of `kind`'s array starting at `start` that still sorts below `limit`, and
    /// where the array has been consumed to.
    pub(crate) fn take_batch(
        &self,
        kind: PrimitiveKind,
        start: usize,
        limit: (DrawOrder, PrimitiveKind),
    ) -> (Batch, usize) {
        match kind {
            PrimitiveKind::Quad => {
                let end = self.run_end(kind, start, limit, None);
                (Batch::Quads(start..end), end)
            }
            // An effect binds its own pipeline and its own parameter block, so a run breaks where
            // either changes — the same rule a sprite run follows for its texture.
            PrimitiveKind::Shaded => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                let shaded = &self.primitives.shaded[slot];
                let binding = shaded_binding(shaded);
                let end = self.run_end(kind, start, limit, Some(binding));
                (
                    Batch::Shaded {
                        shader: shaded.shader_id(),
                        params: shaded.params_slot(),
                        range: start..end,
                    },
                    end,
                )
            }
            PrimitiveKind::Shadow => {
                let end = self.run_end(kind, start, limit, None);
                (Batch::Shadows(start..end), end)
            }
            PrimitiveKind::Decoration => {
                let end = self.run_end(kind, start, limit, None);
                (Batch::Decorations(start..end), end)
            }
            PrimitiveKind::MonoSprite => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                let texture = self.primitives.mono_sprites[slot].tile.texture;
                let end = self.run_end(kind, start, limit, Some(u64::from(texture)));
                (
                    Batch::MonoSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            PrimitiveKind::SubpixelSprite => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                let texture = self.primitives.subpixel_sprites[slot].tile.texture;
                let end = self.run_end(kind, start, limit, Some(u64::from(texture)));
                (
                    Batch::SubpixelSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            PrimitiveKind::ColorSprite => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                let texture = self.primitives.color_sprites[slot].tile.texture;
                let end = self.run_end(kind, start, limit, Some(u64::from(texture)));
                (
                    Batch::ColorSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            // These are drawn one at a time: an external texture and a backdrop each bind their
            // own resources, and a group marker is where a renderer changes target. The batch
            // carries the array index — resolved through the remap here — so a consumer indexes
            // the array directly.
            PrimitiveKind::Vector => (Batch::Vector(start), start + 1),
            PrimitiveKind::External => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                (Batch::External(slot), start + 1)
            }
            PrimitiveKind::Backdrop => {
                let slot = self
                    .slot(kind, start)
                    .expect("the merge peeked this position");
                (Batch::Backdrop(slot), start + 1)
            }
            PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {
                (Batch::Group(self.marker_index(start, kind)), start + 1)
            }
        }
    }

    /// Where a run of `kind` starting at `start` ends.
    ///
    /// `binding` is what every primitive of the run must be drawn with — a sprite's texture, an
    /// effect's pipeline and parameter block — so a run also breaks where a draw call would have
    /// to bind something different. `None` for kinds that bind nothing of their own.
    fn run_end(
        &self,
        kind: PrimitiveKind,
        start: usize,
        limit: (DrawOrder, PrimitiveKind),
        binding: Option<u64>,
    ) -> usize {
        let mut end = start;
        while let Some(order) = self.order_at(kind, end) {
            if (order, kind) >= limit {
                break;
            }
            if binding.is_some() && self.binding_at(kind, end) != binding {
                break;
            }
            end += 1;
        }
        end
    }

    /// What the primitive at draw-order `position` of `kind` has to be drawn with.
    fn binding_at(&self, kind: PrimitiveKind, position: usize) -> Option<u64> {
        let slot = self.slot(kind, position)?;
        match kind {
            PrimitiveKind::MonoSprite => self
                .primitives
                .mono_sprites
                .get(slot)
                .map(|sprite| u64::from(sprite.tile.texture)),
            PrimitiveKind::SubpixelSprite => self
                .primitives
                .subpixel_sprites
                .get(slot)
                .map(|sprite| u64::from(sprite.tile.texture)),
            PrimitiveKind::ColorSprite => self
                .primitives
                .color_sprites
                .get(slot)
                .map(|sprite| u64::from(sprite.tile.texture)),
            PrimitiveKind::Shaded => self.primitives.shaded.get(slot).map(shaded_binding),
            _ => None,
        }
    }

    /// The order of the `index`-th marker of the requested direction.
    ///
    /// Start and end markers share one array, so each is walked as its own stream.
    fn marker_order(&self, index: usize, is_start: bool) -> Option<DrawOrder> {
        let position = *self.markers.stream(is_start).get(index)?;
        self.primitives
            .groups
            .get(position as usize)
            .map(|group| group.order)
    }

    /// Where the `index`-th marker of `kind` sits in the shared array.
    fn marker_index(&self, index: usize, kind: PrimitiveKind) -> usize {
        let is_start = kind == PrimitiveKind::GroupStart;
        self.markers
            .stream(is_start)
            .get(index)
            .map_or(index, |position| *position as usize)
    }
}

/// What one shaded rectangle has to be drawn with: its effect, then its parameter block.
///
/// Two rectangles agreeing on both are one draw call; disagreeing on either is two, because the
/// pipeline and the block are both bound outside the instance.
fn shaded_binding(shaded: &crate::prim::ShadedQuad) -> u64 {
    (u64::from(shaded.shader) << 32) | u64::from(shaded.params)
}
