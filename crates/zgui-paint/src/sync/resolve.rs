//! zgui's tables resolved into compositor nodes and slots while a chunk's items are built.

use compositor_types::{ClipId, PaintId, PaintKind, Transaction, TransformId};
use zgui_scene::Scene;

use crate::sync::mirror::Mirror;

/// The open transaction and the mirror of zgui's tables.
///
/// Each method mirrors what it is asked for on first use, creating the node or the slot in the
/// transaction.
#[derive(Debug)]
pub(crate) struct Resolver<'a> {
    tx: &'a mut Transaction,
    mirror: &'a mut Mirror,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(tx: &'a mut Transaction, mirror: &'a mut Mirror) -> Self {
        Self { tx, mirror }
    }

    /// Returns the transform node of the coordinate system in `slot`.
    pub(crate) fn space(&mut self, scene: &Scene, slot: u32) -> TransformId {
        self.mirror.spaces.ensure(self.tx, &scene.spatial, slot)
    }

    /// Returns the clip node of the chain `clip`, or `None` for a chain that clips nothing the
    /// compositor can express.
    pub(crate) fn clip(&mut self, scene: &mut Scene, clip: zgui_scene::ClipId) -> Option<ClipId> {
        self.mirror.clips.ensure(
            self.tx,
            &mut scene.clips,
            &mut self.mirror.spaces,
            &scene.spatial,
            clip,
        )
    }

    /// Returns the slot and kind of the paint `paint`, or `None` for a paint with no slot kind.
    pub(crate) fn paint(
        &mut self,
        scene: &mut Scene,
        paint: zgui_scene::PaintId,
    ) -> Option<(PaintId, PaintKind)> {
        self.mirror.paints.ensure(self.tx, &mut scene.paints, paint)
    }
}
