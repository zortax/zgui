//! zgui's property tables mirrored into compositor nodes and slots.
//!
//! Coordinate systems become transform nodes, clip chains become clip nodes and paints become
//! paint slots. Each is mirrored the first time a recorded chunk refers to it, and the tables'
//! change journals then turn into node and slot writes before each frame's walk.

pub(crate) mod clips;
pub(crate) mod color;
pub(crate) mod paints;
pub(crate) mod spaces;

use compositor_types::Transaction;
use zgui_scene::Scene;

/// The mirrored nodes and slots of every table.
#[derive(Debug, Default)]
pub(crate) struct Mirror {
    pub(crate) spaces: spaces::Spaces,
    pub(crate) clips: clips::Clips,
    pub(crate) paints: paints::Paints,
}

impl Mirror {
    /// Writes what the tables changed since the last call, and returns the number of writes.
    pub(crate) fn sync(&mut self, tx: &mut Transaction, scene: &mut Scene) -> usize {
        let spaces = self.spaces.sync(tx, &scene.spatial);
        let clips = self
            .clips
            .sync(tx, &mut scene.clips, &mut self.spaces, &scene.spatial);
        let paints = self.paints.sync(tx, &scene.paints);
        spaces + clips + paints
    }
}
