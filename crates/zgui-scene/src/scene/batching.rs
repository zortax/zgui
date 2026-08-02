//! What the batch iterator asks the scene.

use zgui_geom::{Device, DevicePx, Rect};

use crate::batch::Batch;
use crate::id::DrawOrder;
use crate::ops::PaintOp;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;
use crate::spatial::SpatialId;

impl Scene {
    /// What the primitive one log entry names paints.
    ///
    /// Group markers report what the group writes; a vector item is never asked, because it is
    /// handled as a pass rather than as a primitive.
    pub(crate) fn ink_of(&self, op: PaintOp) -> Rect<DevicePx, Device> {
        let index = op.index as usize;
        match op.kind {
            PrimitiveKind::Quad => self.primitives.quads[index].ink(),
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

    /// The draw order of the primitive at `index` of `kind`'s array, or `None` past its end.
    pub(crate) fn order_at(&self, kind: PrimitiveKind, index: usize) -> Option<DrawOrder> {
        match kind {
            PrimitiveKind::Quad => self.primitives.quads.get(index).map(|held| held.order),
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
            PrimitiveKind::GroupStart => self.marker_order(index, true),
            PrimitiveKind::GroupEnd => self.marker_order(index, false),
            // A composite belongs above every item of its pass, not where any one of them draws.
            PrimitiveKind::Vector => self
                .pass_plan()
                .passes
                .get(index)
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
            PrimitiveKind::Shadow => {
                let end = self.run_end(kind, start, limit, None);
                (Batch::Shadows(start..end), end)
            }
            PrimitiveKind::Decoration => {
                let end = self.run_end(kind, start, limit, None);
                (Batch::Decorations(start..end), end)
            }
            PrimitiveKind::MonoSprite => {
                let texture = self.primitives.mono_sprites[start].tile.texture;
                let end = self.run_end(kind, start, limit, Some(texture));
                (
                    Batch::MonoSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            PrimitiveKind::SubpixelSprite => {
                let texture = self.primitives.subpixel_sprites[start].tile.texture;
                let end = self.run_end(kind, start, limit, Some(texture));
                (
                    Batch::SubpixelSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            PrimitiveKind::ColorSprite => {
                let texture = self.primitives.color_sprites[start].tile.texture;
                let end = self.run_end(kind, start, limit, Some(texture));
                (
                    Batch::ColorSprites {
                        texture,
                        range: start..end,
                    },
                    end,
                )
            }
            // These are drawn one at a time: an external texture and a backdrop each bind their own
            // resources, and a group marker is where a renderer changes target.
            PrimitiveKind::Vector => (Batch::Vector(start), start + 1),
            PrimitiveKind::External => (Batch::External(start), start + 1),
            PrimitiveKind::Backdrop => (Batch::Backdrop(start), start + 1),
            PrimitiveKind::GroupStart | PrimitiveKind::GroupEnd => {
                (Batch::Group(self.marker_index(start, kind)), start + 1)
            }
        }
    }

    /// Where a run of `kind` starting at `start` ends.
    ///
    /// `texture` is the texture every sprite of the run must read, so a run also breaks where a
    /// draw call would have to bind a different one. `None` for kinds that bind no texture.
    fn run_end(
        &self,
        kind: PrimitiveKind,
        start: usize,
        limit: (DrawOrder, PrimitiveKind),
        texture: Option<u32>,
    ) -> usize {
        let mut end = start;
        while let Some(order) = self.order_at(kind, end) {
            if (order, kind) >= limit {
                break;
            }
            if texture.is_some() && self.texture_at(kind, end) != texture {
                break;
            }
            end += 1;
        }
        end
    }

    /// Which texture the sprite at `index` of `kind`'s array reads.
    fn texture_at(&self, kind: PrimitiveKind, index: usize) -> Option<u32> {
        match kind {
            PrimitiveKind::MonoSprite => self
                .primitives
                .mono_sprites
                .get(index)
                .map(|sprite| sprite.tile.texture),
            PrimitiveKind::SubpixelSprite => self
                .primitives
                .subpixel_sprites
                .get(index)
                .map(|sprite| sprite.tile.texture),
            PrimitiveKind::ColorSprite => self
                .primitives
                .color_sprites
                .get(index)
                .map(|sprite| sprite.tile.texture),
            _ => None,
        }
    }

    /// The order of the `index`-th marker of the requested direction.
    ///
    /// Start and end markers share one array, so each is walked as its own stream.
    fn marker_order(&self, index: usize, is_start: bool) -> Option<DrawOrder> {
        self.primitives
            .groups
            .iter()
            .filter(|group| group.is_start == is_start)
            .nth(index)
            .map(|group| group.order)
    }

    /// Where the `index`-th marker of `kind` sits in the shared array.
    fn marker_index(&self, index: usize, kind: PrimitiveKind) -> usize {
        let is_start = kind == PrimitiveKind::GroupStart;
        self.primitives
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.is_start == is_start)
            .nth(index)
            .map(|(position, _)| position)
            .unwrap_or(index)
    }
}
