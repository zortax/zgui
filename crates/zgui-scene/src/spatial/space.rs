//! The spatial tree: which coordinate system a box's content is drawn in, and what it resolves to.

use zgui_geom::Matrix4;

use crate::spatial::id::{PropertyOwner, SPATIAL_DOMAIN, SpatialId};
use crate::spatial::node::{Anchoring, OwnSpace, SpatialNode};
use crate::spatial::tree::PropertyTree;

/// Every coordinate system in the document.
pub type SpatialTree = PropertyTree<SpatialId, SpatialNode>;

impl SpatialTree {
    /// A tree with the viewport in it and nothing else.
    ///
    /// The viewport is established here rather than by the box walk so that it is the first
    /// occupant of the first slot and stays so — which is what makes [`SpatialId::VIEWPORT`] a
    /// constant, and a primitive that carries no coordinate system of its own a primitive built
    /// out of a zero.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{SpatialId, SpatialTree};
    ///
    /// let tree = SpatialTree::with_viewport();
    /// assert_eq!(tree.viewport(), SpatialId::VIEWPORT);
    /// assert_eq!(tree.resolve(SpatialId::VIEWPORT), Some(Matrix4::IDENTITY));
    /// ```
    pub fn with_viewport() -> Self {
        let mut tree = Self::new(SPATIAL_DOMAIN);
        tree.root(PropertyOwner::VIEWPORT);
        tree
    }

    /// The coordinate system the document is drawn in.
    pub fn viewport(&self) -> SpatialId {
        self.of(PropertyOwner::VIEWPORT)
            .unwrap_or(SpatialId::VIEWPORT)
    }

    /// The coordinate system everything else is expressed in.
    ///
    /// Named like every other node, after the box that establishes it, so that a document which
    /// replaces its root is not left with two roots.
    pub fn root(&mut self, owner: PropertyOwner) -> SpatialId {
        self.establish(
            owner,
            SpatialNode {
                parent: None,
                local: Matrix4::IDENTITY,
                anchoring: Anchoring::Scrolling,
            },
        )
    }

    /// The coordinate system a box's own content is drawn in, and the one its children descend
    /// into.
    ///
    /// `own` is [`OwnSpace::of`]'s answer for the box. `None` there is the overwhelming majority
    /// and is the whole of the deduplication this tree provides: the box takes the name of the
    /// space above it, so a thousand identical rows resolve to one node and one matrix rather than
    /// to a thousand equal ones. A box that had a coordinate system of its own and no longer does
    /// gives it back here.
    ///
    /// Giving one back costs a lookup by owner, and asking is what finds out there is nothing to
    /// give back, so every box drawn in the space above it pays that lookup once per frame. A
    /// caller that already knows the box held no coordinate system last time — because it kept the
    /// name the box was drawn under — takes `parent` without asking and pays nothing.
    ///
    /// ```
    /// use zgui_arena::DomainId;
    /// use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::new(DomainId::FIRST);
    /// let root = tree.root(PropertyOwner::new(1).expect("a handle is never the empty word"));
    ///
    /// for row in 2..1_002 {
    ///     let owner = PropertyOwner::new(row).expect("a handle is never the empty word");
    ///     assert_eq!(tree.space_of(root, owner, None), root);
    /// }
    /// assert_eq!(tree.len(), 1, "a thousand untransformed rows name one coordinate system");
    /// ```
    pub fn space_of(
        &mut self,
        parent: SpatialId,
        owner: PropertyOwner,
        own: Option<OwnSpace>,
    ) -> SpatialId {
        match own {
            None => {
                self.release(owner);
                parent
            }
            Some(own) => self.establish(
                owner,
                SpatialNode {
                    parent: Some(parent),
                    local: own.local,
                    anchoring: own.anchoring,
                },
            ),
        }
    }

    /// A fingerprint of what a coordinate system currently resolves to.
    ///
    /// A structural name does not move when the matrix under it does — that is the whole point of
    /// naming a coordinate system after the box that establishes it — so a name alone cannot tell
    /// something that cached output whether the matrix it drew through is still that matrix. This
    /// can, at the cost of one walk to the root.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::with_viewport();
    /// let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    /// let moved = |x| OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false);
    ///
    /// let card = tree.space_of(tree.viewport(), owner, moved(4.0));
    /// let before = tree.fingerprint(card);
    /// assert_eq!(tree.space_of(tree.viewport(), owner, moved(9.0)), card, "same name");
    /// assert_ne!(tree.fingerprint(card), before, "and a different matrix under it");
    /// ```
    pub fn fingerprint(&self, id: SpatialId) -> Option<u64> {
        use crate::content::Content;
        Some(self.resolve(id)?.content_hash())
    }

    /// The matrix the coordinate system occupying `slot` maps onto the device.
    ///
    /// What a slot number alone can be turned into: a primitive carries the slot and nothing else,
    /// so anything reading a primitive back — a transcript, a check that a drawn thing was drawn
    /// where it is — comes in through here.
    pub fn resolve_at(&self, slot: u32) -> Option<Matrix4> {
        self.resolve(self.at(slot)?)
    }

    /// The matrix mapping a coordinate system's own coordinates onto the device.
    ///
    /// The product of the node's own transform with every transform above it, in that order, which
    /// is the same matrix composing the box against its parent arrives at. `None` if any node in
    /// the chain has been given back: a name that no longer resolves has no answer rather than a
    /// plausible one.
    ///
    /// ```
    /// use zgui_arena::DomainId;
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{Anchoring, OwnSpace, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::new(DomainId::FIRST);
    /// let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
    /// let root = tree.root(owner(1));
    ///
    /// let moved = |x| OwnSpace { local: Matrix4::translation(x, 0.0, 0.0), anchoring: Anchoring::Scrolling };
    /// let outer = tree.space_of(root, owner(2), Some(moved(10.0)));
    /// let inner = tree.space_of(outer, owner(3), Some(moved(4.0)));
    ///
    /// assert_eq!(tree.resolve(inner), Some(Matrix4::translation(14.0, 0.0, 0.0)));
    /// ```
    pub fn resolve(&self, id: SpatialId) -> Option<Matrix4> {
        self.fold_up(id, Matrix4::IDENTITY, |matrix, node| {
            matrix.then(&node.local)
        })
    }

    /// The matrix mapping `id`'s own coordinates into `ancestor`'s.
    ///
    /// The identity when the two are the same node, and `None` when `ancestor` is not above `id` —
    /// which is also how "is this coordinate system inside that one" is asked, because the walk
    /// that answers it is the walk that produces the matrix and doing both at once costs one of
    /// them.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::with_viewport();
    /// let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
    /// let slid = |x| OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false);
    /// let card = tree.space_of(tree.viewport(), owner(2), slid(4.0));
    /// let label = tree.space_of(card, owner(3), slid(1.0));
    ///
    /// assert_eq!(tree.relative(label, card), Some(Matrix4::translation(1.0, 0.0, 0.0)));
    /// assert_eq!(tree.relative(card, card), Some(Matrix4::IDENTITY));
    /// assert_eq!(tree.relative(card, label), None, "the label is below the card, not above it");
    /// ```
    pub fn relative(&self, id: SpatialId, ancestor: SpatialId) -> Option<Matrix4> {
        let mut matrix = Matrix4::IDENTITY;
        let mut next = Some(id);
        while let Some(current) = next {
            if current == ancestor {
                return Some(matrix);
            }
            let node = self.get(current)?;
            matrix = matrix.then(&node.local);
            next = node.parent;
        }
        None
    }

    /// Writes where one coordinate system sits within the one above it.
    ///
    /// The single route by which an established coordinate system's own matrix changes, whether an
    /// animation is moving it or a restyle gave the box a different `transform`. Two routes would
    /// be two answers to "has this box moved", and everything downstream that caches output under a
    /// structural name compares one of them.
    ///
    /// Reports whether the value stored is not the one that was there, which is what a caller owes
    /// damage on. Writing the matrix already held is recorded as no write at all, so
    /// [`written_since_recycle`](crate::PropertyTree::written_since_recycle) stays false for a
    /// transform that is holding still.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    /// use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};
    ///
    /// let mut tree = SpatialTree::with_viewport();
    /// let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    /// let card = tree.space_of(tree.viewport(), owner, OwnSpace::of(Some(Matrix4::IDENTITY), None, false));
    ///
    /// assert!(tree.place(card, Matrix4::translation(8.0, 0.0, 0.0)));
    /// assert!(!tree.place(card, Matrix4::translation(8.0, 0.0, 0.0)), "the same matrix again");
    /// assert_eq!(tree.resolve(card), Some(Matrix4::translation(8.0, 0.0, 0.0)));
    /// ```
    pub fn place(&mut self, id: SpatialId, local: Matrix4) -> bool {
        let Some(node) = self.get_mut(id) else {
            return false;
        };
        if node.local == local {
            return false;
        }
        node.local = local;
        self.note_written();
        true
    }
}
