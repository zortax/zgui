//! Clip chains mirrored as clip nodes.

use compositor_types::{ClipId, ClipNode, ClipShape, Transaction};
use zgui_scene::{ChangeCoverage, ClipLink, ClipTable, SpatialTree, TableVersion};

use crate::sync::mirror::spaces::Spaces;

/// The clip node of each mirrored clip chain, by zgui clip id.
///
/// A mirrored chain is held in the zgui table, so its id names the same chain until the mirror
/// is reset. A link the compositor cannot express, such as a mask, is left out of the chain.
#[derive(Debug, Default)]
pub(crate) struct Clips {
    nodes: Vec<Option<Option<ClipId>>>,
    written: Vec<Option<ClipNode>>,
    version: Option<TableVersion>,
    changed: Vec<zgui_scene::ClipId>,
}

impl Clips {
    /// Returns the clip node of the chain `id`, mirroring it and its ancestors on first use.
    ///
    /// The root chain, and a chain of links the compositor cannot express, is `None`.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        table: &mut ClipTable,
        spaces: &mut Spaces,
        tree: &SpatialTree,
        id: zgui_scene::ClipId,
    ) -> Option<ClipId> {
        if self.version.is_none() {
            self.version = Some(table.version());
        }
        let index = id.0 as usize;
        if let Some(Some(mirrored)) = self.nodes.get(index) {
            return *mirrored;
        }
        let (mirrored, written) = match self.node(tx, table, spaces, tree, id) {
            Some(Ok(node)) => (Some(tx.create_clip(node.clone())), Some(node)),
            Some(Err(parent)) => (parent, None),
            None => (None, None),
        };
        table.retain(id);
        if self.nodes.len() <= index {
            self.nodes.resize(index + 1, None);
            self.written.resize(index + 1, None);
        }
        self.nodes[index] = Some(mirrored);
        self.written[index] = written;
        mirrored
    }

    /// Rewrites the mirrored chains whose value the table changed since the last call.
    ///
    /// Returns the number of nodes written.
    pub(crate) fn sync(
        &mut self,
        tx: &mut Transaction,
        table: &mut ClipTable,
        spaces: &mut Spaces,
        tree: &SpatialTree,
    ) -> usize {
        let Some(version) = self.version else {
            return 0;
        };
        self.changed.clear();
        let coverage = table.changes_since(version, &mut self.changed);
        self.version = Some(table.version());
        if coverage == ChangeCoverage::All {
            self.changed.clear();
            let nodes = &self.nodes;
            self.changed.extend(
                (0..nodes.len() as u32)
                    .map(zgui_scene::ClipId)
                    .filter(|id| matches!(nodes.get(id.0 as usize), Some(Some(Some(_))))),
            );
        }
        let changed = std::mem::take(&mut self.changed);
        let mut written = 0;
        for &id in &changed {
            let Some(Some(Some(mirrored))) = self.nodes.get(id.0 as usize).copied() else {
                continue;
            };
            if let Some(Ok(node)) = self.node(tx, table, spaces, tree, id)
                && self.written[id.0 as usize].as_ref() != Some(&node)
            {
                tx.set_clip(mirrored, node.clone());
                self.written[id.0 as usize] = Some(node);
                written += 1;
            }
        }
        self.changed = changed;
        written
    }

    /// Returns the clip node `id` is mirrored as, or the parent's node when its link cannot be
    /// expressed, or `None` for the root.
    fn node(
        &mut self,
        tx: &mut Transaction,
        table: &mut ClipTable,
        spaces: &mut Spaces,
        tree: &SpatialTree,
        id: zgui_scene::ClipId,
    ) -> Option<Result<ClipNode, Option<ClipId>>> {
        let zgui_scene::ClipNode::Link { link, parent, .. } = *table.get(id)? else {
            return None;
        };
        let parent = self.ensure(tx, table, spaces, tree, parent);
        let ClipLink::RoundedRect {
            rect,
            radii,
            shape,
            space,
        } = link
        else {
            // Mask links need a compositor image of the coverage tile.
            return Some(Err(parent));
        };
        Some(Ok(ClipNode {
            parent,
            space: spaces.ensure(tx, tree, space.index()),
            shape: ClipShape::RoundedRect {
                rect: [
                    rect.origin.x.0,
                    rect.origin.y.0,
                    rect.size.width.0,
                    rect.size.height.0,
                ],
                radii: [
                    radii.top_left.x.0,
                    radii.top_left.y.0,
                    radii.top_right.x.0,
                    radii.top_right.y.0,
                    radii.bottom_right.x.0,
                    radii.bottom_right.y.0,
                    radii.bottom_left.x.0,
                    radii.bottom_left.y.0,
                ],
                shapes: [shape.0; 4],
            },
        }))
    }
}
