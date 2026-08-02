//! How much of each side table the frame that is starting inherited.

use zgui_profile::{Counter, counter};

use crate::scene::Scene;

/// Publishes the side tables' lengths as live counts.
///
/// At the start of a frame rather than the end of one, because the question these answer is what
/// the frame *inherited*. A table that is emptied and refilled every frame reads the same either
/// way; one that is only ever added to reads its running total here, and that is the one worth
/// finding — an id has to keep resolving to the same content across frames, so nothing in these
/// tables is cleared by a frame boundary and a value interned once is held until something gives it
/// back. Nothing gives one back today, which is a fact the counts state rather than a fact anyone
/// has to go looking for.
pub(crate) fn publish(scene: &Scene) {
    counter::set(Counter::ClipEntriesLive, scene.clips.len() as u64);
    counter::set(Counter::PaintEntriesLive, scene.paints.len() as u64);
    counter::set(
        Counter::SpatialNodesLive,
        scene.spatial.ids().count() as u64,
    );
}
