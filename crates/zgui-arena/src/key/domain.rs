//! Which arena, of which document, a handle names.

/// How many documents a process can address at once.
pub const DOCUMENT_COUNT: u32 = 1 << 12;

/// How many arenas one document can own.
pub const ARENA_KIND_COUNT: u8 = 1 << 4;

/// One document's identity.
///
/// Two windows are two documents. Because the identity travels inside every handle, a handle
/// minted by one document can never be mistaken for a handle of another, which is what makes a
/// multi-document process expressible without a global registry.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct DocumentId(u16);

impl DocumentId {
    /// The first document a process creates.
    pub const FIRST: Self = Self(0);

    /// Wraps a raw number, rejecting anything past [`DOCUMENT_COUNT`].
    pub const fn new(value: u16) -> Option<Self> {
        if (value as u32) < DOCUMENT_COUNT {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The raw number, which is always below [`DOCUMENT_COUNT`].
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Which of a document's arenas a handle names.
///
/// A document keeps several arenas — one per kind of thing it stores — and they index
/// independently, so the same slot number and the same occupancy counter occur in all of them at
/// once. Recording the kind in the handle is what keeps those from colliding.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct ArenaKind(u8);

impl ArenaKind {
    /// The first arena kind, for a document that only keeps one.
    pub const FIRST: Self = Self(0);

    /// Wraps a raw number, rejecting anything past [`ARENA_KIND_COUNT`].
    pub const fn new(value: u8) -> Option<Self> {
        if value < ARENA_KIND_COUNT {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The raw number, which is always below [`ARENA_KIND_COUNT`].
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The identity of one arena: which document owns it, and which of that document's arenas it is.
///
/// A handle carries its domain, so resolving a handle is also a proof of provenance — a handle is
/// only ever read by the arena that minted it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct DomainId(u16);

impl DomainId {
    /// The first arena of the first document.
    pub const FIRST: Self = Self::new(DocumentId::FIRST, ArenaKind::FIRST);

    /// Pairs a document with one of its arenas.
    pub const fn new(document: DocumentId, arena: ArenaKind) -> Self {
        Self((document.get() << 4) | arena.get() as u16)
    }

    /// The document that owns this arena.
    pub const fn document(self) -> DocumentId {
        DocumentId(self.0 >> 4)
    }

    /// Which of that document's arenas this is.
    pub const fn arena(self) -> ArenaKind {
        ArenaKind((self.0 & 0xf) as u8)
    }

    /// The packed form: the document in the high twelve bits, the arena kind in the low four.
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Unpacks the form [`DomainId::as_u16`] produces. Every bit pattern is a valid domain.
    pub const fn from_u16(bits: u16) -> Self {
        Self(bits)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{ARENA_KIND_COUNT, ArenaKind, DOCUMENT_COUNT, DocumentId, DomainId};

    #[test]
    fn out_of_range_identities_are_rejected() {
        assert_eq!(DocumentId::new(DOCUMENT_COUNT as u16), None);
        assert_eq!(ArenaKind::new(ARENA_KIND_COUNT), None);
        assert!(DocumentId::new(DOCUMENT_COUNT as u16 - 1).is_some());
        assert!(ArenaKind::new(ARENA_KIND_COUNT - 1).is_some());
    }

    #[test]
    fn the_first_domain_is_all_zero_bits() {
        assert_eq!(DomainId::FIRST.as_u16(), 0);
    }

    proptest! {
        #![proptest_config(crate::proptest_config::config())]

        /// Packing a document with an arena kind loses neither.
        #[test]
        fn packing_round_trips(
            document in 0_u16..DOCUMENT_COUNT as u16,
            arena in 0_u8..ARENA_KIND_COUNT,
        ) {
            let document = DocumentId::new(document).expect("in range");
            let arena = ArenaKind::new(arena).expect("in range");
            let domain = DomainId::new(document, arena);
            prop_assert_eq!(domain.document(), document);
            prop_assert_eq!(domain.arena(), arena);
            prop_assert_eq!(DomainId::from_u16(domain.as_u16()), domain);
        }

        /// Every packed domain unpacks into a pair that repacks to the same bits.
        #[test]
        fn every_bit_pattern_is_a_domain(bits in any::<u16>()) {
            let domain = DomainId::from_u16(bits);
            prop_assert_eq!(DomainId::new(domain.document(), domain.arena()), domain);
        }
    }
}
