//! Scrollbar thumbs mirrored as transform nodes that a scroll offset moves along their track.

use iceblink::{Anchor, Matrix4, ScrollId, ThumbAnchor, Transaction, TransformId, TransformNode};
use zgui_space::spatial::SpatialTree;

use crate::sync::mirror::holds::Holds;
use crate::sync::mirror::spaces::Spaces;

/// One mirrored thumb node and what it was last written with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Thumb {
    pub(super) id: TransformId,
    written: Written,
}

/// The value of a thumb node.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Written {
    /// The node of the coordinate system the thumb is drawn in.
    parent: TransformId,
    /// The node of the scroll container's content.
    content: TransformId,
    /// The scroll node whose offset moves the thumb.
    scroll: ScrollId,
    /// How far the thumb moves over the whole scroll range, as `[x, y]`.
    track: [f32; 2],
}

impl Written {
    fn node(&self) -> TransformNode {
        TransformNode {
            parent: Some(self.parent),
            local: Matrix4::IDENTITY,
            scroll: None,
            anchor: Anchor::Thumb(ThumbAnchor {
                scroll: self.scroll,
                track: self.track,
            }),
        }
    }

    /// The ids the node holds.
    fn held(&self) -> [iceblink::AnyId; 3] {
        [self.parent.into(), self.content.into(), self.scroll.into()]
    }
}

impl Spaces {
    /// Returns the node a thumb is recorded in, for the chunk being recorded.
    ///
    /// The thumb is drawn in the coordinate system in `slot` and moves `free` pixels along `axis`
    /// (0 for x, 1 for y) over the scroll range of the content in the slot `content`. It is
    /// recorded where it sits at offset zero, and the compositor moves it with the offset it
    /// shows.
    ///
    /// Returns `None` when the content applies no scroll node.
    pub(crate) fn thumb(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        tree: &SpatialTree,
        (slot, content): (u32, u32),
        axis: usize,
        free: f32,
    ) -> Option<TransformId> {
        let parent = self.mirror(tx, holds, tree, slot);
        let content_node = self.mirror(tx, holds, tree, content);
        let scroll = self.slots.get(content as usize)?.as_ref()?.scroll?;
        let mut track = [0.0; 2];
        track[axis] = free;
        let next = Written {
            parent,
            content: content_node,
            scroll,
            track,
        };

        let key = (content, axis as u8);
        let id = match self.thumbs.get(&key).copied() {
            Some(thumb) if thumb.written == next => thumb.id,
            Some(thumb) => {
                for id in thumb.written.held() {
                    holds.unhold(thumb.id, id);
                }
                for id in next.held() {
                    holds.hold(thumb.id, id);
                }
                tx.set_transform_node(thumb.id, next.node());
                self.thumbs.insert(
                    key,
                    Thumb {
                        id: thumb.id,
                        written: next,
                    },
                );
                thumb.id
            }
            None => {
                let id = tx.create_transform(next.node());
                holds.created(id);
                for held in next.held() {
                    holds.hold(id, held);
                }
                self.thumbs.insert(key, Thumb { id, written: next });
                self.thumb_nodes.insert(id, key);
                id
            }
        };
        holds.name(id);
        Some(id)
    }

    /// Forgets the thumb node `id`, which was removed, and reports whether it was one.
    pub(super) fn removed_thumb(&mut self, id: TransformId) -> bool {
        let Some(key) = self.thumb_nodes.remove(&id) else {
            return false;
        };
        self.thumbs.remove(&key);
        true
    }
}
