//! Pushing primitives: the clip cull, the order assignment, and the log entry.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_profile::{Counter, counter};

use crate::group::{BackdropFilter, GroupBoundary};
use crate::id::{ClipId, DrawOrder};
use crate::ops::PaintOp;
use crate::prim::{
    ColorSprite, Decoration, ExternalQuad, MonoSprite, PrimitiveKind, Quad, Shadow, SubpixelSprite,
};
use crate::scene::Scene;
use crate::spatial::SpatialId;
use crate::vector::VectorItem;

/// Appends one primitive to the open chunk capture, before the cull and before the order.
///
/// The capture is the pushing's complete content, so it happens whether or not the cull refuses
/// the primitive, and the captured copy carries no order — a replay derives one against the frame
/// it lands in. The coordinate-system name is recorded beside it when the checks are on, exactly
/// as [`Scene::record`] does for the frame's own log.
macro_rules! tee {
    ($self:ident, $kind:ident, $lane:ident, $space:expr, $prim:expr) => {
        if $self.capture.is_some() {
            // Both read before the capture is borrowed, because either would borrow the scene.
            let space = $space;
            let checking = $self.checking;
            if let Some(capture) = &mut $self.capture {
                let at = capture.$lane.len() as u32;
                capture.ops.push(PaintOp::new(PrimitiveKind::$kind, at));
                if checking {
                    capture.spaces.push(space);
                }
                capture.$lane.push($prim);
            }
        }
    };
}

impl Scene {
    /// Forces every primitive pushed until the matching [`Scene::pop_layer`] to take `order`.
    ///
    /// This is the escape hatch for content whose sequence is decided somewhere other than by
    /// geometry — an overlay layer, say — and it is deliberately narrow: inside a layer, two
    /// primitives at equal order may genuinely overlap, so the invariant the rest of the crate
    /// relies on is suspended and the caller owns the sequence.
    pub fn push_layer(&mut self, order: DrawOrder) {
        self.layer_stack.push(order);
        if !self.forced_orders.contains(&order) {
            self.forced_orders.push(order);
        }
    }

    /// Ends the innermost layer.
    pub fn pop_layer(&mut self) {
        self.layer_stack.pop();
    }

    /// Raises the lowest order anything pushed afterwards may take.
    ///
    /// Called after a group closes and before deferred content, so that a later non-overlapping
    /// sibling cannot take a low order and sort underneath what was painted before it.
    pub fn set_order_floor(&mut self, floor: DrawOrder) {
        self.order.set_order_floor(floor);
    }

    /// Pushes a rounded rectangle, returning the order it took or `None` if it was culled.
    pub fn push_quad(&mut self, mut quad: Quad) -> Option<DrawOrder> {
        tee!(self, Quad, quads, self.space_at(quad.transform), quad);
        let order = self.assign_order(quad.ink(), quad.clip_id(), quad.transform)?;
        quad.order = order;
        let space = self.space_at(quad.transform);
        self.record(PrimitiveKind::Quad, self.primitives.quads.len(), space);
        self.primitives.quads.push(quad);
        Some(order)
    }

    /// Pushes a box shadow, returning the order it took or `None` if it was culled.
    pub fn push_shadow(&mut self, mut shadow: Shadow) -> Option<DrawOrder> {
        tee!(
            self,
            Shadow,
            shadows,
            self.space_at(shadow.transform),
            shadow
        );
        let order = self.assign_order(shadow.ink(), shadow.clip_id(), shadow.transform)?;
        shadow.order = order;
        let space = self.space_at(shadow.transform);
        self.record(PrimitiveKind::Shadow, self.primitives.shadows.len(), space);
        self.primitives.shadows.push(shadow);
        Some(order)
    }

    /// Pushes a text decoration line, returning the order it took or `None` if it was culled.
    pub fn push_decoration(&mut self, mut decoration: Decoration) -> Option<DrawOrder> {
        tee!(
            self,
            Decoration,
            decorations,
            self.space_at(decoration.transform),
            decoration
        );
        let order =
            self.assign_order(decoration.ink(), decoration.clip_id(), decoration.transform)?;
        decoration.order = order;
        let space = self.space_at(decoration.transform);
        self.record(
            PrimitiveKind::Decoration,
            self.primitives.decorations.len(),
            space,
        );
        self.primitives.decorations.push(decoration);
        Some(order)
    }

    /// Pushes a single-channel coverage sprite, returning the order it took or `None` if it was
    /// culled.
    pub fn push_mono_sprite(&mut self, mut sprite: MonoSprite) -> Option<DrawOrder> {
        tee!(
            self,
            MonoSprite,
            mono_sprites,
            self.space_at(sprite.transform),
            sprite
        );
        let order = self.assign_order(sprite.ink(), sprite.clip_id(), sprite.transform)?;
        sprite.order = order;
        let space = self.space_at(sprite.transform);
        self.record(
            PrimitiveKind::MonoSprite,
            self.primitives.mono_sprites.len(),
            space,
        );
        self.note_resource(
            PrimitiveKind::MonoSprite,
            self.primitives.mono_sprites.len(),
            sprite.tile,
        );
        self.primitives.mono_sprites.push(sprite);
        Some(order)
    }

    /// Pushes a three-channel coverage sprite, returning the order it took or `None` if it was
    /// culled.
    ///
    /// The caller decides whether a run is subpixel at all: the device may not support it, and a
    /// run landing inside a group whose target is not opaque has to be a
    /// [`MonoSprite`] instead, because subpixel coverage against a transparent destination is
    /// meaningless.
    pub fn push_subpixel_sprite(&mut self, mut sprite: SubpixelSprite) -> Option<DrawOrder> {
        tee!(
            self,
            SubpixelSprite,
            subpixel_sprites,
            self.space_at(sprite.transform),
            sprite
        );
        let order = self.assign_order(sprite.ink(), sprite.clip_id(), sprite.transform)?;
        sprite.order = order;
        let space = self.space_at(sprite.transform);
        self.record(
            PrimitiveKind::SubpixelSprite,
            self.primitives.subpixel_sprites.len(),
            space,
        );
        self.note_resource(
            PrimitiveKind::SubpixelSprite,
            self.primitives.subpixel_sprites.len(),
            sprite.tile,
        );
        self.primitives.subpixel_sprites.push(sprite);
        Some(order)
    }

    /// Pushes a full-colour sprite, returning the order it took or `None` if it was culled.
    pub fn push_color_sprite(&mut self, mut sprite: ColorSprite) -> Option<DrawOrder> {
        tee!(
            self,
            ColorSprite,
            color_sprites,
            self.space_at(sprite.transform),
            sprite
        );
        let order = self.assign_order(sprite.ink(), sprite.clip_id(), sprite.transform)?;
        sprite.order = order;
        let space = self.space_at(sprite.transform);
        self.record(
            PrimitiveKind::ColorSprite,
            self.primitives.color_sprites.len(),
            space,
        );
        self.note_resource(
            PrimitiveKind::ColorSprite,
            self.primitives.color_sprites.len(),
            sprite.tile,
        );
        self.primitives.color_sprites.push(sprite);
        Some(order)
    }

    /// Pushes vector content, returning the order it took or `None` if it was culled.
    ///
    /// A vector item is logged like everything else and, unlike everything else, is not re-emitted
    /// by [`Scene::replay`] — it is planned into a rasterisation pass instead. So pushing one puts
    /// the log out of step with what was drawn, which is what
    /// [`Scene::unreplayable`](Scene::unreplayable) counts.
    pub fn push_vector(&mut self, mut item: VectorItem) -> Option<DrawOrder> {
        tee!(self, Vector, vectors, item.transform, item.clone());
        self.note_unreplayable();
        // The order and the cull read the ink measured in the subtree's own space, exactly as they
        // do for every other primitive: `item.ink` has the item's transform applied, and testing it
        // against neighbours recorded untransformed decides overlap in two different spaces at
        // once. A held placement showed the failure — the drawing ordered against nothing, painted
        // first, and covered by the surface drawn over it.
        let order = self.assign_order(
            item.local_ink,
            item.clip,
            item.transform.unwrap_or(SpatialId::VIEWPORT).index(),
        )?;
        item.order = order;
        let space = item.transform;
        self.record(PrimitiveKind::Vector, self.primitives.vectors.len(), space);
        self.primitives.vectors.push(item);
        Some(order)
    }

    /// Pushes an external texture, returning the order it took or `None` if it was culled.
    pub fn push_external(&mut self, mut external: ExternalQuad) -> Option<DrawOrder> {
        tee!(
            self,
            External,
            externals,
            Some(external.transform),
            external
        );
        let order = self.assign_order(external.ink(), external.clip, external.transform.index())?;
        external.order = order;
        let space = Some(external.transform);
        self.record(
            PrimitiveKind::External,
            self.primitives.externals.len(),
            space,
        );
        self.primitives.externals.push(external);
        Some(order)
    }

    /// Pushes a backdrop filter, returning the order it took or `None` if it was culled.
    pub fn push_backdrop(&mut self, mut backdrop: BackdropFilter) -> Option<DrawOrder> {
        tee!(self, Backdrop, backdrops, None, backdrop.clone());
        let order =
            self.assign_order(backdrop.bounds, backdrop.clip, SpatialId::VIEWPORT.index())?;
        backdrop.order = order;
        // A backdrop names no coordinate system: what it reads is a rectangle of the target as it
        // already stands, in the device's own coordinates.
        self.record(
            PrimitiveKind::Backdrop,
            self.primitives.backdrops.len(),
            None,
        );
        self.primitives.backdrops.push(backdrop);
        Some(order)
    }

    /// Pushes a group marker, which is **never** culled.
    ///
    /// Markers are matched pairs: dropping one because its clip admits nothing, or because nothing
    /// damaged reaches it, leaves a target open or composites one that was never begun. They take
    /// their order from above everything already pushed, so that unrelated content elsewhere cannot
    /// reuse an order inside the group's range and be swept into its target.
    ///
    /// A closing marker raises the floor to **one above** its own order, not to it. Equal draw
    /// order is settled by [`PrimitiveKind`], which puts a closing marker last, so anything left
    /// free to take the closing marker's own order would sort ahead of it — inside a group that had
    /// already finished. That is a whole primitive drawn into somebody else's target, clipped away
    /// by its bounds, and it happens precisely when the next thing drawn does not overlap the group
    /// it follows.
    pub fn push_group(&mut self, mut boundary: GroupBoundary) -> DrawOrder {
        let order = self.order.insert_above_all(boundary.bounds);
        let is_end = !boundary.is_start;
        boundary.order = order;
        let kind = if boundary.is_start {
            PrimitiveKind::GroupStart
        } else {
            PrimitiveKind::GroupEnd
        };
        let space = boundary.transform;
        self.record(kind, self.primitives.groups.len(), space);
        self.primitives.groups.push(boundary);
        if is_end {
            self.order.set_order_floor(order + 1);
        }
        order
    }

    /// The order a primitive of `ink` under `clip` takes, or `None` when the clip admits none of it.
    ///
    /// The clip cull is one intersection and it is not optional: without it, every off-screen row of
    /// a thousand-row table inside a scrollport still costs a spatial-index query and an array push.
    ///
    /// Both rectangles are read as they were recorded, before any transform: `ink` is the
    /// primitive's own bounds, which for content inside a transformed subtree are measured in that
    /// subtree's space, and the clip imposed on it from inside the same subtree is measured there
    /// too. Resolving only one of them onto the device would cull a field's letters against a
    /// rectangle the transform moved out from under them.
    ///
    /// `space` is the slot of the coordinate system the primitive draws under, and the cull reads
    /// only the links of the chain that were measured in it. A link from anywhere else states its
    /// rectangle in another system's coordinates — the window's clip against a panel the panel's
    /// own transform moved — and is left to the shader, which resolves every link against the
    /// frame's matrices. See [`ClipTable::bounds_in`](crate::ClipTable::bounds_in).
    fn assign_order(
        &mut self,
        ink: Rect<DevicePx, Device>,
        clip: ClipId,
        space: u32,
    ) -> Option<DrawOrder> {
        let admitted = self.clips.bounds_in(clip, space);
        let Some(clipped) = ink.intersection(admitted) else {
            counter::bump(Counter::PrimitivesCulled);
            self.note_unreplayable();
            return None;
        };
        Some(match self.layer_stack.last() {
            Some(order) => *order,
            None => self.order.insert(clipped),
        })
    }

    /// The name occupying a primitive's slot, for a primitive that carries only the slot.
    ///
    /// `None` when the names are not being kept, which is every run that did not ask for them.
    pub(crate) fn space_at(&self, slot: u32) -> Option<SpatialId> {
        if !self.checking {
            return None;
        }
        self.spatial.at(slot)
    }

    /// Appends the log entry for a primitive about to be pushed, and the coordinate system it is
    /// being pushed under.
    ///
    /// The name is recorded rather than looked up again later because the counter in it is the
    /// whole of what a later lookup cannot recover: the slot resolves either way, and it resolves
    /// to the wrong box exactly when something has gone wrong. See [`mod@crate::scene::depends`].
    fn record(&mut self, kind: PrimitiveKind, index: usize, space: Option<SpatialId>) {
        self.ops.push(PaintOp::new(kind, index as u32));
        if self.checking {
            self.spaces.push(space);
        }
        counter::bump(Counter::PrimitivesEmitted);
    }
}
