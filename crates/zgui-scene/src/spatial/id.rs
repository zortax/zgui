//! What names a property node, and what the name is minted from.

use core::hash::Hash;
use core::num::NonZeroU64;

use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};

use crate::spatial::node::SpatialNode;

/// The arena a scene's coordinate systems are minted in.
///
/// One constant rather than one per window, because a name is only ever resolved against the tree
/// of the scene that minted it: a scene is a value, two windows never exchange one, and a name that
/// crossed between them would be resolved against the wrong document's boxes whatever it carried.
pub const SPATIAL_DOMAIN: DomainId = DomainId::new(DocumentId::FIRST, ArenaKind::FIRST);

/// The name of one coordinate system.
///
/// A slot number, an occupancy counter and the arena that minted it, packed into eight bytes that
/// are never all zero — so an absent space costs nothing to represent. The slot number is what a
/// dense buffer of resolved matrices is indexed by; the counter is what keeps the name honest once
/// slots come back.
///
/// ```
/// use zgui_arena::DomainId;
/// use zgui_geom::Matrix4;
/// use zgui_scene::{PropertyOwner, SpatialTree};
///
/// let mut tree = SpatialTree::new(DomainId::FIRST);
/// let viewport = PropertyOwner::new(1).expect("not the empty word");
/// let root = tree.root(viewport);
/// assert_eq!(tree.resolve(root), Some(Matrix4::IDENTITY));
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct SpatialId(Key<SpatialNode>);

impl SpatialId {
    /// The name the viewport's coordinate system always takes.
    ///
    /// A scene's tree is made with the viewport already in it and never gives it back, so the
    /// viewport is the first occupant of the first slot for as long as the scene exists. That is
    /// what lets a primitive that names no coordinate system of its own be built with nothing but
    /// a zero in it — the slot a dense buffer resolves to the identity — without the builder
    /// having a tree to ask.
    pub const VIEWPORT: Self = Self(Key::new(0, Generation::FIRST, SPATIAL_DOMAIN));

    /// The slot the node occupies, which is what a dense buffer of resolved values is indexed by.
    pub const fn index(self) -> u32 {
        self.0.index()
    }

    /// Which occupant of that slot this name refers to.
    ///
    /// Two names with the same slot and different counters are two different coordinate systems
    /// that happened to be stored in the same place, one after the other.
    pub const fn generation(self) -> Generation {
        self.0.generation()
    }

    /// The arena that minted it: which document, and which of that document's arenas.
    pub const fn domain(self) -> DomainId {
        self.0.domain()
    }
}

impl PropertyId for SpatialId {
    type Node = SpatialNode;

    fn from_key(key: Key<SpatialNode>) -> Self {
        Self(key)
    }

    fn key(self) -> Key<SpatialNode> {
        self.0
    }
}

/// What makes a handle usable as a [`PropertyTree`](crate::PropertyTree)'s name for a node.
///
/// It exists so that one tree implementation serves several kinds of property node without any of
/// their names being interchangeable: the tree is generic over this trait, and a name for a
/// coordinate system is not a name for anything else even though both are one word wide.
pub trait PropertyId: Copy + Eq + Hash {
    /// What a tree holding these names stores.
    type Node;

    /// Wraps the handle the storage minted.
    fn from_key(key: Key<Self::Node>) -> Self;

    /// The handle the storage minted.
    fn key(self) -> Key<Self::Node>;
}

/// The box a property node is named after.
///
/// Naming a node after the box that establishes it is what makes the name outlive the value: the
/// box can be relaid out, restyled and have its fragments rebuilt, and it is still the same box, so
/// it is still given back the same name for the coordinate system it establishes.
///
/// A box's own handle belongs to the tree that owns the boxes, which is not this crate's to name,
/// so the owner is held as the packed form a handle takes when it crosses an interface that speaks
/// only integers. That form carries the box's own occupancy counter as well as its slot, so a box
/// that goes away and a new box that takes its slot are two owners and never share a node.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct PropertyOwner(NonZeroU64);

impl PropertyOwner {
    /// The owner of the viewport's coordinate system.
    ///
    /// The viewport is not a box, so it is named after the word with every bit set — which a box
    /// handle would have to be the four-billionth slot of the last arena of the last document, at
    /// the sixty-five-thousandth occupant of that slot, to collide with.
    pub const VIEWPORT: Self = match NonZeroU64::new(u64::MAX) {
        Some(packed) => Self(packed),
        None => unreachable!(),
    };

    /// The owner a handle names.
    ///
    /// ```
    /// use zgui_arena::{ChunkArena, DomainId};
    /// use zgui_scene::PropertyOwner;
    ///
    /// let mut boxes: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
    /// let one = boxes.insert("a box");
    /// let two = boxes.insert("another");
    /// assert_ne!(PropertyOwner::of(one), PropertyOwner::of(two));
    /// ```
    pub fn of<T: ?Sized>(key: Key<T>) -> Self {
        let packed =
            NonZeroU64::new(key.as_u64()).expect("a handle packs to a word that is never all zero");
        Self(packed)
    }

    /// Wraps the packed form of a handle directly, rejecting the empty word no handle packs to.
    pub const fn new(packed: u64) -> Option<Self> {
        match NonZeroU64::new(packed) {
            Some(packed) => Some(Self(packed)),
            None => None,
        }
    }

    /// The packed form.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}
