//! What the spatial tree promises, and the two promises interning a matrix could not make.

use zgui_arena::{ArenaKind, ChunkArena, DocumentId, DomainId};
use zgui_geom::Matrix4;

use crate::id::ScrollFrameId;
use crate::spatial::{Anchoring, OwnSpace, Placements, PropertyOwner, SpatialTree};

/// Stands in for the boxes a document is made of, so that owners are minted the way they are in
/// one: from a handle carrying a slot and an occupancy counter.
struct Boxes(ChunkArena<()>);

impl Boxes {
    fn new() -> Self {
        Self(ChunkArena::new(DomainId::FIRST))
    }

    /// One more box.
    fn one(&mut self) -> PropertyOwner {
        PropertyOwner::of(self.0.insert(()))
    }
}

/// A coordinate system moved along x and scrolling like everything else.
fn moved(x: f32) -> Option<OwnSpace> {
    OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false)
}

#[test]
fn identical_untransformed_boxes_share_a_spatial_node() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let mut spaces = Vec::new();
    for _ in 0..1_000 {
        spaces.push(tree.space_of(root, boxes.one(), OwnSpace::of(None, None, false)));
    }

    assert!(spaces.iter().all(|space| *space == root));
    assert_eq!(
        tree.len(),
        1,
        "a thousand identical rows resolve to one coordinate system",
    );
}

#[test]
fn a_recycled_slot_is_not_the_name_it_was() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let departing = boxes.one();
    let before = tree.space_of(root, departing, moved(10.0));
    tree.release(departing);
    tree.recycle();

    let unrelated = boxes.one();
    let after = tree.space_of(root, unrelated, moved(-40.0));

    assert_eq!(
        before.index(),
        after.index(),
        "the slot came back, which is what makes the counter necessary",
    );
    assert_ne!(before.generation(), after.generation());
    assert_ne!(
        before, after,
        "an unrelated box holding the slot is not the node something recorded",
    );
    assert_eq!(tree.get(before), None, "the old name resolves to nothing");
    assert_eq!(
        tree.resolve(after),
        Some(Matrix4::translation(-40.0, 0.0, 0.0)),
    );
}

#[test]
fn a_released_node_is_still_readable_until_the_frame_ends() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let departing = boxes.one();
    let space = tree.space_of(root, departing, moved(10.0));
    tree.release(departing);

    assert_eq!(
        tree.resolve(space),
        Some(Matrix4::translation(10.0, 0.0, 0.0)),
        "a pass later in the same frame still resolves what it was handed",
    );
    tree.recycle();
    assert_eq!(tree.resolve(space), None);
}

#[test]
fn a_name_outlives_the_value_under_it() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let animating = boxes.one();
    let first = tree.space_of(root, animating, moved(0.0));
    let mut latest = first;
    for frame in 1..120 {
        latest = tree.space_of(root, animating, moved(frame as f32));
    }

    assert_eq!(
        first, latest,
        "the box is the box, so the space is the space"
    );
    assert_eq!(tree.len(), 2, "the root and the one that is moving");
    assert_eq!(
        tree.resolve(latest),
        Some(Matrix4::translation(119.0, 0.0, 0.0)),
    );
}

#[test]
fn a_box_that_loses_its_transform_gives_its_node_back() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let box_ = boxes.one();
    let own = tree.space_of(root, box_, moved(10.0));
    assert_ne!(own, root);

    let shared = tree.space_of(root, box_, OwnSpace::of(None, None, false));
    tree.recycle();

    assert_eq!(shared, root, "it is drawn in the space above it now");
    assert_eq!(tree.get(own), None);
    assert_eq!(tree.len(), 1);
}

#[test]
fn a_chain_through_a_released_node_resolves_to_nothing() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let outer_owner = boxes.one();
    let outer = tree.space_of(root, outer_owner, moved(10.0));
    let inner = tree.space_of(outer, boxes.one(), moved(4.0));
    assert_eq!(
        tree.resolve(inner),
        Some(Matrix4::translation(14.0, 0.0, 0.0)),
    );

    tree.release(outer_owner);
    tree.recycle();

    assert_eq!(
        tree.resolve(inner),
        None,
        "a broken chain has no answer rather than a plausible one",
    );
}

#[test]
fn each_of_the_three_cases_establishes_a_space_and_says_which_it_is() {
    let matrix = Matrix4::translation(1.0, 0.0, 0.0);
    let port = ScrollFrameId(7);

    assert_eq!(
        OwnSpace::of(Some(matrix), None, false),
        Some(OwnSpace {
            local: matrix,
            anchoring: Anchoring::Scrolling,
        }),
    );
    assert_eq!(
        OwnSpace::of(None, Some(port), false),
        Some(OwnSpace {
            local: Matrix4::IDENTITY,
            anchoring: Anchoring::Sticky { port },
        }),
    );
    assert_eq!(
        OwnSpace::of(None, None, true),
        Some(OwnSpace {
            local: Matrix4::IDENTITY,
            anchoring: Anchoring::Fixed,
        }),
    );
    assert_eq!(
        OwnSpace::of(None, None, false),
        None,
        "everything else moves by the vector the box above it moved by",
    );
}

#[test]
fn a_transformed_sticky_box_keeps_both_answers() {
    let matrix = Matrix4::scale(2.0, 2.0, 1.0);
    let port = ScrollFrameId(3);

    assert_eq!(
        OwnSpace::of(Some(matrix), Some(port), false),
        Some(OwnSpace {
            local: matrix,
            anchoring: Anchoring::Sticky { port },
        }),
        "the matrix is the space and the anchoring is how it takes the scrolling; neither is the \
         other",
    );
}

#[test]
fn a_name_from_one_documents_tree_does_not_resolve_in_anothers() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());
    let space = tree.space_of(root, boxes.one(), moved(10.0));

    let second = DomainId::new(DocumentId::new(1).expect("in range"), ArenaKind::FIRST);
    let mut other = SpatialTree::new(second);
    let mut others = Boxes::new();
    let other_root = other.root(others.one());
    let same_slot = other.space_of(other_root, others.one(), moved(10.0));

    assert_eq!(space.index(), same_slot.index());
    assert_eq!(space.generation(), same_slot.generation());
    assert_ne!(space, same_slot, "the document is part of the name");
    assert_eq!(other.get(space), None);
}

#[test]
fn a_resolved_answer_is_refused_to_the_name_the_slot_no_longer_holds() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let departing = boxes.one();
    let before = tree.space_of(root, departing, moved(10.0));
    tree.release(departing);
    tree.recycle();
    let after = tree.space_of(root, boxes.one(), moved(-40.0));

    let placements = Placements::of(&tree);
    assert_eq!(before.index(), after.index(), "the slot came back");
    assert_eq!(
        placements.get(after),
        Some(&Matrix4::translation(-40.0, 0.0, 0.0)),
    );
    assert_eq!(
        placements.get(before),
        None,
        "a name from before the slot changed hands is answered with nothing, not with the \
         stranger's matrix that is sitting in it",
    );
}

#[test]
fn a_slot_nothing_occupies_reads_as_the_matrix_that_moves_nothing() {
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());

    let departing = boxes.one();
    let gone = tree.space_of(root, departing, moved(10.0));
    tree.release(departing);
    tree.recycle();

    let placements = Placements::of(&tree);
    let dense: Vec<Matrix4> = placements.matrices().collect();
    assert_eq!(
        dense.get(gone.index() as usize),
        Some(&Matrix4::IDENTITY),
        "a shader indexes this array with whatever a stale primitive carries, so a hole has to \
         draw content where it was laid out rather than wherever the bytes fell",
    );
}

#[test]
fn re_establishing_the_same_coordinate_system_with_the_same_matrix_is_not_a_write() {
    // What lets a reader skip looking for movement. Every frame re-establishes every coordinate
    // system the document has, so a flag that counted those would be set on every frame of every
    // document and would say nothing at all — and the comparison it exists to avoid is one per
    // coordinate system per frame, paid by a window with a screen reader attached and nothing
    // moving in it.
    let mut boxes = Boxes::new();
    let mut tree = SpatialTree::new(DomainId::FIRST);
    let root = tree.root(boxes.one());
    let card = boxes.one();
    tree.space_of(root, card, moved(10.0));
    assert!(
        tree.written_since_recycle(),
        "establishing a coordinate system that did not exist is a write"
    );

    tree.recycle();
    assert!(!tree.written_since_recycle(), "the frame ended");
    tree.space_of(root, card, moved(10.0));
    assert!(
        !tree.written_since_recycle(),
        "the document was composed again and nothing about where anything is drawn changed"
    );

    tree.space_of(root, card, moved(11.0));
    assert!(
        tree.written_since_recycle(),
        "one pixel of movement is movement, and a reader told nothing moved would answer with the \
         matrix from before it"
    );

    tree.recycle();
    tree.release(card);
    assert!(
        tree.written_since_recycle(),
        "a coordinate system given back stops resolving, which every name below it depends on"
    );
}
