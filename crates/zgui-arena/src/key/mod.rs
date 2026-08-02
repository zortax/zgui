//! Handles: what a stored value is named by, and what makes the name safe to keep.

pub mod domain;
pub mod generation;

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::num::NonZeroU64;

pub use crate::key::domain::{ARENA_KIND_COUNT, ArenaKind, DOCUMENT_COUNT, DocumentId, DomainId};
pub use crate::key::generation::Generation;

/// A stable, generation-checked handle into an arena.
///
/// The index identifies a slot, the generation distinguishes successive occupants of that slot,
/// and the domain identifies the arena the slot belongs to. A key from a removed value never
/// resolves to its replacement, and a key from one document never resolves inside another.
///
/// A key is eight bytes and never all-zero, so [`Option`] of a key is eight bytes too: an absent
/// parent, a missing sibling or an unstyled node costs nothing to represent.
///
/// The type parameter is what the key names, and it may be unsized. It appears only in a
/// [`PhantomData`], so a key is [`Copy`], [`Send`] and [`Sync`] whatever it names, and two keys of
/// different types cannot be compared or passed for one another.
///
/// ```
/// use zgui_arena::{ChunkArena, DomainId};
///
/// let mut arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
/// let key = arena.insert(7);
/// assert_eq!(key.domain(), DomainId::FIRST);
/// assert_eq!(arena.get(key), Some(&7));
/// ```
#[repr(transparent)]
pub struct Key<T: ?Sized>(NonZeroU64, PhantomData<fn() -> T>);

impl<T: ?Sized> Key<T> {
    /// Builds a key from its three parts.
    ///
    /// Minting a key by hand is for tests and for bridges that carry a key through a foreign
    /// integer type; ordinary code receives keys from the arena that stores the value.
    pub const fn new(index: u32, generation: Generation, domain: DomainId) -> Self {
        let bits =
            index as u64 | ((generation.get() as u64) << 32) | ((domain.as_u16() as u64) << 48);
        match NonZeroU64::new(bits) {
            Some(bits) => Self(bits, PhantomData),
            None => panic!("a generation is never zero, so the packed form is never zero"),
        }
    }

    /// Slot number within the domain.
    pub const fn index(self) -> u32 {
        self.0.get() as u32
    }

    /// Occupancy counter for the slot.
    ///
    /// A slot that runs out of counters is retired rather than reused, so a key never outlives
    /// the distinction its counter draws.
    pub const fn generation(self) -> Generation {
        match Generation::new(((self.0.get() >> 32) & 0xffff) as u16) {
            Some(generation) => generation,
            None => panic!("a key's generation is never zero"),
        }
    }

    /// The arena this key belongs to: which document owns it, and which of that document's arenas.
    pub const fn domain(self) -> DomainId {
        DomainId::from_u16((self.0.get() >> 48) as u16)
    }

    /// The document this key belongs to.
    pub const fn document(self) -> DocumentId {
        self.domain().document()
    }

    /// The packed form: index in the low 32 bits, generation in the next 16, domain in the top 16.
    ///
    /// This is the form a key takes when it has to cross an interface that only speaks integers.
    pub const fn as_u64(self) -> u64 {
        self.0.get()
    }

    /// Unpacks the form [`Key::as_u64`] produces, rejecting bits that no key can have.
    ///
    /// The generation field is zero for a slot that was never issued and for one that has been
    /// retired, so those bit patterns are not keys and are rejected here rather than resolving to
    /// whatever happens to occupy the slot.
    pub const fn from_u64(bits: u64) -> Option<Self> {
        if ((bits >> 32) & 0xffff) == 0 {
            return None;
        }
        match NonZeroU64::new(bits) {
            Some(bits) => Some(Self(bits, PhantomData)),
            None => None,
        }
    }
}

// The derives would demand `T: Clone`, `T: Ord` and so on, which is wrong twice over: a key is a
// number whatever it names, and `T` need not even be sized.
impl<T: ?Sized> Copy for Key<T> {}

impl<T: ?Sized> Clone for Key<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> PartialEq for Key<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: ?Sized> Eq for Key<T> {}

impl<T: ?Sized> PartialOrd for Key<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: ?Sized> Ord for Key<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T: ?Sized> Hash for Key<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: ?Sized> fmt::Debug for Key<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Key({}:{}@{}.{})",
            self.index(),
            self.generation().get(),
            self.document().get(),
            self.domain().arena().get()
        )
    }
}

/// What a side table needs of a key to index by it.
///
/// A side table stores one value per slot of one arena, so it needs the slot number and — to
/// catch a key that came from somewhere else — the domain. It deliberately does not check the
/// generation: that check belongs to the arena, which resolves the key to its value once, and
/// repeating it per column would double the cost of every column read for no new information.
///
/// Implement this for a handle newtype to key side tables by it directly.
pub trait ArenaKey: Copy + Eq {
    /// Slot number within the domain.
    fn index(self) -> u32;

    /// Occupancy counter for the slot.
    fn generation(self) -> Generation;

    /// The arena the slot belongs to.
    fn domain(self) -> DomainId;
}

impl<T: ?Sized> ArenaKey for Key<T> {
    fn index(self) -> u32 {
        Key::index(self)
    }

    fn generation(self) -> Generation {
        Key::generation(self)
    }

    fn domain(self) -> DomainId {
        Key::domain(self)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{ArenaKind, DOCUMENT_COUNT, DocumentId, DomainId, Generation, Key};

    /// A stand-in for whatever an arena stores.
    struct Value;

    #[test]
    fn the_very_first_slot_of_the_very_first_arena_is_representable() {
        let key = Key::<Value>::new(0, Generation::FIRST, DomainId::FIRST);
        assert_eq!(Key::<Value>::from_u64(key.as_u64()), Some(key));
    }

    #[test]
    fn a_key_is_one_word_and_so_is_an_optional_key() {
        assert_eq!(size_of::<Key<Value>>(), size_of::<u64>());
        assert_eq!(size_of::<Option<Key<Value>>>(), size_of::<u64>());
    }

    #[test]
    fn an_unissued_or_retired_generation_is_not_a_key() {
        assert_eq!(Key::<Value>::from_u64(0), None);
        // Index 7, domain 3, generation 0: a plausible-looking word that names no occupant.
        assert_eq!(Key::<Value>::from_u64(7 | (3 << 48)), None);
    }

    #[test]
    fn the_debug_form_names_all_four_parts() {
        let document = DocumentId::new(2).expect("in range");
        let arena = ArenaKind::new(3).expect("in range");
        let key = Key::<Value>::new(9, Generation::FIRST, DomainId::new(document, arena));
        assert_eq!(format!("{key:?}"), "Key(9:1@2.3)");
    }

    proptest! {
        #![proptest_config(crate::proptest_config::config())]

        /// Every key round-trips through its packed form, and none of them encode to zero.
        #[test]
        fn packing_round_trips_and_never_encodes_to_zero(
            index in any::<u32>(),
            generation in 1_u16..=u16::MAX,
            document in 0_u16..DOCUMENT_COUNT as u16,
            arena in 0_u8..16,
        ) {
            let domain = DomainId::new(
                DocumentId::new(document).expect("in range"),
                ArenaKind::new(arena).expect("in range"),
            );
            let generation = Generation::new(generation).expect("non-zero");
            let key = Key::<Value>::new(index, generation, domain);

            prop_assert_ne!(key.as_u64(), 0);
            prop_assert_eq!(key.index(), index);
            prop_assert_eq!(key.generation(), generation);
            prop_assert_eq!(key.domain(), domain);
            prop_assert_eq!(Key::<Value>::from_u64(key.as_u64()), Some(key));
        }

        /// The three fields occupy disjoint bits: changing one changes no other.
        #[test]
        fn the_fields_do_not_overlap(index in any::<u32>(), other in any::<u32>()) {
            let one = Key::<Value>::new(index, Generation::FIRST, DomainId::FIRST);
            let two = Key::<Value>::new(other, Generation::FIRST, DomainId::FIRST);
            prop_assert_eq!(one.generation(), two.generation());
            prop_assert_eq!(one.domain(), two.domain());
            prop_assert_eq!(one == two, index == other);
        }
    }
}
