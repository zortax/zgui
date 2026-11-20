//! zgui's tables resolved into compositor nodes and slots while a chunk's items are built.

use compositor_types::{
    ClipId, Filter, ImageDesc, ImageId, ImageWrite, PaintId, PaintKind, ParamsBlock, ParamsId,
    ShaderId, Transaction, TransformId,
};
use zgui_scene::Scene;

use crate::sync::mirror::Mirror;

/// The open transaction and the mirror of zgui's tables.
///
/// Each method mirrors what it is asked for on first use, creating the node or the slot in the
/// transaction, and notes it as named by the chunk being recorded.
#[derive(Debug)]
pub(crate) struct Resolver<'a> {
    tx: &'a mut Transaction,
    mirror: &'a mut Mirror,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(tx: &'a mut Transaction, mirror: &'a mut Mirror) -> Self {
        Self { tx, mirror }
    }

    /// Returns a resolver over the same transaction and mirror for a shorter time.
    pub(crate) fn reborrow(&mut self) -> Resolver<'_> {
        Resolver {
            tx: self.tx,
            mirror: self.mirror,
        }
    }

    /// Returns the transform node of the coordinate system in `slot`.
    pub(crate) fn space(&mut self, scene: &Scene, slot: u32) -> TransformId {
        self.mirror.space(self.tx, &scene.spatial, slot)
    }

    /// Returns the clip node of the chain `clip`, or `None` for a chain that clips nothing the
    /// compositor can express.
    pub(crate) fn clip(&mut self, scene: &mut Scene, clip: zgui_scene::ClipId) -> Option<ClipId> {
        self.mirror.clip(self.tx, scene, clip)
    }

    /// Returns the slot and kind of the paint `paint`, or `None` for a paint with no slot kind.
    pub(crate) fn paint(
        &mut self,
        scene: &mut Scene,
        paint: zgui_scene::PaintId,
    ) -> Option<(PaintId, PaintKind)> {
        self.mirror.paint(self.tx, scene, paint)
    }

    /// Returns the image of `key` and its description, creating or updating it from `pixels`, or
    /// `None` when nothing is mirrored for `key` and `pixels` is `None`.
    pub(crate) fn image(
        &mut self,
        key: crate::emit::record::ImageKey,
        pixels: Option<crate::emit::record::ImagePixels<'_>>,
    ) -> Option<(ImageId, ImageDesc)> {
        self.mirror.image(self.tx, key, pixels)
    }

    /// Writes `write` into the image of `key`, which then holds the texels of `revision`, and
    /// reports whether one is mirrored.
    pub(crate) fn write_image(
        &mut self,
        key: crate::emit::record::ImageKey,
        write: ImageWrite,
        revision: u64,
    ) -> bool {
        self.mirror.images.write(self.tx, key, write, revision)
    }

    /// Returns the compositor shader of the declared shader `id`, or `None` for a handle no
    /// program was declared under.
    pub(crate) fn shader(&mut self, id: zgui_scene::ShaderId) -> Option<ShaderId> {
        self.mirror.shader(self.tx, id)
    }

    /// Returns the parameter slot holding `block`.
    pub(crate) fn params(&mut self, block: ParamsBlock) -> ParamsId {
        self.mirror.params(self.tx, block)
    }

    /// Returns the slot of `params`.
    pub(crate) fn shared_params(
        &mut self,
        params: &crate::content::shader::SharedParams,
    ) -> ParamsId {
        self.mirror.shared_params(self.tx, params)
    }

    /// Returns the compositor filter that runs `step`, or `None` for a shader no program was
    /// declared under.
    pub(crate) fn shader_filter(
        &mut self,
        step: crate::emit::shader::ShaderStep,
    ) -> Option<Filter> {
        let shader = self.shader(step.shader)?;
        Some(Filter::Shader {
            shader,
            params: self.params(step.params),
            reach: step.reach,
        })
    }
}
