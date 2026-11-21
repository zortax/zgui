//! Clip chains mirrored as clip nodes.

use compositor_types::{ClipId, ClipNode, ClipShape, Transaction};
use rustc_hash::FxHashMap;
use zgui_space::{ChangeCoverage, ClipLink, ClipTable, SpatialTree, TableVersion};

use crate::sync::mirror::holds::Holds;
use crate::sync::mirror::spaces::Spaces;

/// The clip node of each mirrored clip chain, by zgui clip id.
///
/// A mirrored chain is held in the zgui table while its node exists, so its id names the same
/// chain. A mask link becomes a mask over the image of its tile's texture, which
/// [`ImageKey::of_texture`] names. A mask whose texture the compositor holds no image of is left
/// out of the chain.
#[derive(Debug, Default)]
pub(crate) struct Clips {
    entries: Vec<Option<Entry>>,
    nodes: FxHashMap<ClipId, u32>,
    version: Option<TableVersion>,
    changed: Vec<zgui_space::ClipId>,
}

/// One mirrored chain.
#[derive(Clone, Debug)]
struct Entry {
    id: ClipId,
    written: ClipNode,
}

/// The tables a clip node is mirrored from.
pub(crate) struct Sources<'a> {
    pub(crate) table: &'a mut ClipTable,
    pub(crate) spaces: &'a mut Spaces,
    pub(crate) tree: &'a SpatialTree,
}

impl Clips {
    /// Returns the clip node of the chain `id` for the chunk being recorded, mirroring it and its
    /// ancestors on first use.
    ///
    /// The root chain, and a chain of links the compositor cannot express, is `None`.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        sources: Sources<'_>,
        id: zgui_space::ClipId,
    ) -> Option<ClipId> {
        let mut sources = sources;
        let node = self.mirror(tx, holds, &mut sources, id)?;
        holds.name(node);
        Some(node)
    }

    /// Returns the clip node of the chain `id`, mirroring it and its ancestors on first use.
    pub(crate) fn mirror(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        sources: &mut Sources<'_>,
        id: zgui_space::ClipId,
    ) -> Option<ClipId> {
        if self.version.is_none() {
            self.version = Some(sources.table.version());
        }
        if let Some(Some(entry)) = self.entries.get(id.0 as usize) {
            return Some(entry.id);
        }
        let node = self.node(tx, holds, sources, id)?;
        let mirrored = tx.create_clip(node.clone());
        holds.created(mirrored);
        hold_links(holds, mirrored, &node);
        sources.table.retain(id);

        let index = id.0 as usize;
        if self.entries.len() <= index {
            self.entries.resize(index + 1, None);
        }
        self.entries[index] = Some(Entry {
            id: mirrored,
            written: node,
        });
        self.nodes.insert(mirrored, id.0);
        Some(mirrored)
    }

    /// Rewrites the mirrored chains whose value the table changed since the last call.
    ///
    /// Returns the number of nodes written.
    pub(crate) fn sync(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        mut sources: Sources<'_>,
    ) -> usize {
        let Some(version) = self.version else {
            return 0;
        };
        let mut changed = std::mem::take(&mut self.changed);
        changed.clear();
        let coverage = sources.table.changes_since(version, &mut changed);
        self.version = Some(sources.table.version());
        if coverage == ChangeCoverage::All {
            changed.clear();
            changed.extend(self.nodes.values().map(|&raw| zgui_space::ClipId(raw)));
        }
        let mut written = 0;
        for &id in &changed {
            written += self.rewrite(tx, holds, &mut sources, id);
        }
        self.changed = changed;
        written
    }

    /// Forgets the node `id`, which was removed, and gives its chain's hold back to `table`.
    pub(crate) fn removed(&mut self, id: ClipId, table: &mut ClipTable) {
        if let Some(raw) = self.nodes.remove(&id) {
            self.entries[raw as usize] = None;
            table.release(zgui_space::ClipId(raw));
        }
    }

    /// Returns the zgui chains of every mirrored node.
    pub(crate) fn mirrored(&self) -> impl Iterator<Item = zgui_space::ClipId> + '_ {
        self.nodes.values().map(|&raw| zgui_space::ClipId(raw))
    }

    /// Writes the node of `id` if its value changed, and returns the number of nodes written.
    fn rewrite(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        sources: &mut Sources<'_>,
        id: zgui_space::ClipId,
    ) -> usize {
        let Some(Some(entry)) = self.entries.get(id.0 as usize).cloned() else {
            return 0;
        };
        let Some(node) = self.node(tx, holds, sources, id) else {
            return 0;
        };
        if node == entry.written {
            return 0;
        }
        hold_links(holds, entry.id, &node);
        unhold_links(holds, entry.id, &entry.written);
        tx.set_clip(entry.id, node.clone());
        self.entries[id.0 as usize] = Some(Entry {
            id: entry.id,
            written: node,
        });
        1
    }

    /// Returns the clip node `id` is mirrored as, or `None` for the root.
    fn node(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        sources: &mut Sources<'_>,
        id: zgui_space::ClipId,
    ) -> Option<ClipNode> {
        let zgui_space::ClipNode::Link { link, parent, .. } = *sources.table.get(id)? else {
            return None;
        };
        let parent = self.mirror(tx, holds, sources, parent);
        let ClipLink::RoundedRect {
            rect,
            radii,
            shape,
            space,
        } = link;
        let space = sources
            .spaces
            .mirror(tx, holds, sources.tree, space.index());
        Some(ClipNode {
            parent,
            space,
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
        })
    }
}

/// Makes the clip node `id` hold the parent, the space and the mask image `node` names.
fn hold_links(holds: &mut Holds, id: ClipId, node: &ClipNode) {
    if let Some(parent) = node.parent {
        holds.hold(id, parent);
    }
    holds.hold(id, node.space);
    if let ClipShape::Mask { image, .. } = node.shape {
        holds.hold(id, image);
    }
}

/// Ends the holds of the clip node `id` on the parent, the space and the mask image `node` names.
fn unhold_links(holds: &mut Holds, id: ClipId, node: &ClipNode) {
    if let Some(parent) = node.parent {
        holds.unhold(id, parent);
    }
    holds.unhold(id, node.space);
    if let ClipShape::Mask { image, .. } = node.shape {
        holds.unhold(id, image);
    }
}
