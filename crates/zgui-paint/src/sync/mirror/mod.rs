//! zgui's property tables mirrored into compositor nodes and slots.
//!
//! Coordinate systems become transform and scroll nodes, clip chains become clip nodes, paints
//! become paint slots, images become compositor images, and application shaders and their
//! parameters become compositor shaders and parameter slots. Each is mirrored the first time a
//! recorded chunk refers to it, and the tables' change journals then turn into node and slot
//! writes before each frame's walk.
//!
//! A mirrored node or slot exists while a recorded chunk or another mirrored node refers to it.
//! [`Mirror::settle`] removes the rest.

pub(crate) mod clips;
pub(crate) mod color;
pub(crate) mod holds;
pub(crate) mod images;
pub(crate) mod paints;
pub(crate) mod params;
pub(crate) mod shaders;
pub(crate) mod spaces;

use crate::Tables;
use compositor_types::{AnyId, ChunkKey, Transaction};

use crate::sync::mirror::clips::Sources;

/// The mirrored nodes and slots of every table, and the holds on them.
#[derive(Debug, Default)]
pub(crate) struct Mirror {
    pub(crate) spaces: spaces::Spaces,
    pub(crate) clips: clips::Clips,
    pub(crate) paints: paints::Paints,
    pub(crate) images: images::Images,
    pub(crate) shaders: shaders::Shaders,
    pub(crate) params: params::Params,
    pub(crate) holds: holds::Holds,
    removed: Vec<AnyId>,
    /// Whether the chunk being recorded draws a shader that reads the clock.
    recording_timed: bool,
    /// The chunks that draw a shader that reads the clock.
    timed_chunks: rustc_hash::FxHashSet<ChunkKey>,
    /// The effect nodes that filter with a shader that reads the clock.
    pub(crate) timed_effects: rustc_hash::FxHashSet<compositor_types::EffectId>,
}

/// The zgui ids a mirror held when it was forgotten, to give back to the tables.
#[derive(Debug, Default)]
pub(crate) struct Held {
    clips: Vec<zgui_space::ClipId>,
    paints: Vec<zgui_space::PaintId>,
}

impl Mirror {
    /// Writes what the tables changed since the last call, and returns the number of writes.
    pub(crate) fn sync(&mut self, tx: &mut Transaction, tables: &mut Tables) -> usize {
        let spaces = self.spaces.sync(tx, &mut self.holds, &tables.spatial);
        let clips = self.clips.sync(
            tx,
            &mut self.holds,
            Sources {
                table: &mut tables.clips,
                spaces: &mut self.spaces,
                images: &self.images,
                tree: &tables.spatial,
            },
        );
        let paints = self.paints.sync(tx, &mut self.holds, &tables.paints);
        spaces + clips + paints
    }

    /// Returns the node of the coordinate system in `slot`, for the chunk being recorded.
    pub(crate) fn space(
        &mut self,
        tx: &mut Transaction,
        tree: &zgui_space::SpatialTree,
        slot: u32,
    ) -> compositor_types::TransformId {
        self.spaces.ensure(tx, &mut self.holds, tree, slot)
    }

    /// Returns the node of the clip chain `id`, for the chunk being recorded.
    ///
    /// The root chain, and a chain of links the compositor cannot express, is `None`.
    pub(crate) fn clip(
        &mut self,
        tx: &mut Transaction,
        tables: &mut Tables,
        id: zgui_space::ClipId,
    ) -> Option<compositor_types::ClipId> {
        let sources = Sources {
            table: &mut tables.clips,
            spaces: &mut self.spaces,
            images: &self.images,
            tree: &tables.spatial,
        };
        self.clips.ensure(tx, &mut self.holds, sources, id)
    }

    /// Returns the node of the clip chain `id`, for a node that holds it.
    pub(crate) fn held_clip(
        &mut self,
        tx: &mut Transaction,
        tables: &mut Tables,
        id: zgui_space::ClipId,
    ) -> Option<compositor_types::ClipId> {
        let mut sources = Sources {
            table: &mut tables.clips,
            spaces: &mut self.spaces,
            images: &self.images,
            tree: &tables.spatial,
        };
        self.clips.mirror(tx, &mut self.holds, &mut sources, id)
    }

    /// Returns the slot and kind of the paint `id`, for the chunk being recorded.
    ///
    /// An image paint, which the compositor has no slot for, is `None`.
    pub(crate) fn paint(
        &mut self,
        tx: &mut Transaction,
        tables: &mut Tables,
        id: zgui_space::PaintId,
    ) -> Option<(compositor_types::PaintId, compositor_types::PaintKind)> {
        self.paints
            .ensure(tx, &mut self.holds, &mut tables.paints, id)
    }

    /// Returns the image of `key` for the chunk being recorded, creating or updating it from
    /// `pixels`.
    ///
    /// Returns the image and its description, or `None` when nothing is mirrored for `key` and
    /// `pixels` is `None`.
    pub(crate) fn image(
        &mut self,
        tx: &mut Transaction,
        key: crate::emit::record::ImageKey,
        pixels: Option<crate::emit::record::ImagePixels<'_>>,
    ) -> Option<(compositor_types::ImageId, compositor_types::ImageDesc)> {
        self.images.ensure(tx, &mut self.holds, key, pixels)
    }

    /// Returns the compositor shader of the declared shader `id`, or `None` for a handle no
    /// program was declared under.
    pub(crate) fn shader(
        &mut self,
        tx: &mut Transaction,
        id: zgui_css::shader::ShaderId,
    ) -> Option<compositor_types::ShaderId> {
        let shader = self.shaders.ensure(tx, id)?;
        self.recording_timed |= self.shaders.reads_time(shader);
        Some(shader)
    }

    /// Returns the slot of `params`, for the chunk being recorded.
    pub(crate) fn shared_params(
        &mut self,
        tx: &mut Transaction,
        params: &crate::content::shader::SharedParams,
    ) -> compositor_types::ParamsId {
        self.params.shared(tx, &mut self.holds, params)
    }

    /// Returns the parameter slot holding `block`, for the chunk being recorded.
    pub(crate) fn params(
        &mut self,
        tx: &mut Transaction,
        block: compositor_types::ParamsBlock,
    ) -> compositor_types::ParamsId {
        self.params.ensure(tx, &mut self.holds, block)
    }

    /// Makes the chunk `key` hold what was mirrored for it since the last recording.
    pub(crate) fn recorded(&mut self, key: ChunkKey) {
        self.holds.recorded(key);
        if std::mem::take(&mut self.recording_timed) {
            self.timed_chunks.insert(key);
        } else {
            self.timed_chunks.remove(&key);
        }
    }

    /// Ends the holds of the chunk `key`, which was removed.
    pub(crate) fn chunk_removed(&mut self, key: ChunkKey) {
        self.holds.removed(key);
        self.timed_chunks.remove(&key);
    }

    /// Reports whether a chunk or an effect node draws a shader that reads the clock.
    pub(crate) fn reads_time(&self) -> bool {
        !self.timed_chunks.is_empty() || !self.timed_effects.is_empty()
    }

    /// Removes every node and slot nothing holds, and returns how many were removed.
    pub(crate) fn settle(&mut self, tx: &mut Transaction, tables: &mut Tables) -> usize {
        let mut removed = std::mem::take(&mut self.removed);
        self.holds.settle(tx, &mut removed);
        for &id in &removed {
            match id {
                AnyId::Transform(id) => self.spaces.removed(id),
                AnyId::Scroll(id) => self.spaces.removed_scroll(id),
                AnyId::Clip(id) => self.clips.removed(id, &mut tables.clips),
                AnyId::Paint(id) => self.paints.removed(id, &mut tables.paints),
                AnyId::Image(id) => self.images.removed(id),
                AnyId::Params(id) => self.params.removed(id),
                _ => {}
            }
        }
        let count = removed.len();
        removed.clear();
        self.removed = removed;
        count
    }

    /// Returns the zgui ids the mirror holds in the tables.
    pub(crate) fn held(&self) -> Held {
        Held {
            clips: self.clips.mirrored().collect(),
            paints: self.paints.mirrored().collect(),
        }
    }
}

impl Held {
    /// Adds the ids `other` holds.
    pub(crate) fn extend(&mut self, other: Self) {
        self.clips.extend(other.clips);
        self.paints.extend(other.paints);
    }

    /// Gives every hold back to `tables`.
    pub(crate) fn release(self, tables: &mut Tables) {
        for id in self.clips {
            tables.clips.release(id);
        }
        for id in self.paints {
            tables.paints.release(id);
        }
    }
}
