//! The opaque handle a view holds on a backend's node.

use core::fmt::{self, Debug};
use core::num::NonZeroU64;

use crate::id::document::DocumentId;

/// How many bits at the bottom of a handle belong to the backend.
const BACKEND_BITS: u32 = 52;

/// An opaque handle to a node in whatever tree the installed backend manages.
///
/// A view never interprets one. What it may rely on is the layout: the top twelve bits are the
/// [`DocumentId`] the handle was minted for, and the remaining fifty-two are the backend's own
/// numbering — a slot number, a generation, an index into a vector of browser nodes, whatever the
/// backend needs. Carrying the document inside the handle is what makes a two-window process safe
/// by construction: a handle from one window applied to another is detectable, in a comparison
/// the backend can make in debug builds without keeping a registry.
///
/// A handle is never zero, so `Option<NodeId>` is eight bytes.
///
/// ```
/// use zgui_view::{DocumentId, NodeId};
///
/// let node = NodeId::new(DocumentId::FIRST, 7).expect("in range");
/// assert_eq!(node.document(), DocumentId::FIRST);
/// assert_eq!(node.backend_bits(), 7);
///
/// // A second window's handles are distinguishable from the first window's.
/// let other = NodeId::new(DocumentId::new(1).expect("in range"), 7).expect("in range");
/// assert_ne!(node, other);
/// assert_eq!(core::mem::size_of::<Option<NodeId>>(), 8);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct NodeId(NonZeroU64);

impl NodeId {
    /// The largest value [`NodeId::backend_bits`] can hold.
    pub const MAX_BACKEND_BITS: u64 = (1 << BACKEND_BITS) - 1;

    /// Builds a handle for `document` from a backend's own numbering.
    ///
    /// `backend_bits` must be at most [`NodeId::MAX_BACKEND_BITS`] and, for the first document,
    /// must not be zero — a handle is never zero, and document zero contributes no bits of its
    /// own. Backends that number from one, as an arena with a non-zero generation counter does,
    /// never meet the second condition.
    pub const fn new(document: DocumentId, backend_bits: u64) -> Option<Self> {
        if backend_bits > Self::MAX_BACKEND_BITS {
            return None;
        }
        let bits = backend_bits | ((document.get() as u64) << BACKEND_BITS);
        match NonZeroU64::new(bits) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    /// Rebuilds a handle from the packed form [`NodeId::as_u64`] produces.
    pub const fn from_u64(bits: u64) -> Option<Self> {
        match NonZeroU64::new(bits) {
            Some(bits) => Some(Self(bits)),
            None => None,
        }
    }

    /// The packed form, which is what an accessibility tree or a foreign integer interface takes.
    pub const fn as_u64(self) -> u64 {
        self.0.get()
    }

    /// Which document this handle was minted for.
    pub const fn document(self) -> DocumentId {
        match DocumentId::new((self.0.get() >> BACKEND_BITS) as u16) {
            Some(document) => document,
            None => DocumentId::FIRST,
        }
    }

    /// The backend's own numbering, with the document stripped off.
    pub const fn backend_bits(self) -> u64 {
        self.0.get() & Self::MAX_BACKEND_BITS
    }

    /// Whether this handle belongs to `document`.
    ///
    /// Backends assert this at the top of every method in debug builds; it is the check that
    /// turns a cross-window mistake into a panic at the call site rather than a node of the wrong
    /// tree being quietly mutated.
    pub const fn belongs_to(self, document: DocumentId) -> bool {
        self.document().get() == document.get()
    }
}

impl Debug for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NodeId(doc {}, {})",
            self.document().get(),
            self.backend_bits()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{BACKEND_BITS, NodeId};
    use crate::id::document::{DOCUMENT_COUNT, DocumentId};

    #[test]
    fn the_document_survives_the_round_trip_for_every_document() {
        for raw in [0, 1, 2, 4095] {
            let document = DocumentId::new(raw).expect("in range");
            let node = NodeId::new(document, 12345).expect("in range");
            assert_eq!(node.document(), document);
            assert_eq!(node.backend_bits(), 12345);
            assert_eq!(NodeId::from_u64(node.as_u64()), Some(node));
        }
    }

    #[test]
    fn the_first_document_cannot_mint_a_zero_handle() {
        assert_eq!(NodeId::new(DocumentId::FIRST, 0), None);
        // Every other document can, because its own bits make the handle non-zero.
        let second = DocumentId::new(1).expect("in range");
        assert!(NodeId::new(second, 0).is_some());
    }

    #[test]
    fn backend_bits_past_the_limit_are_rejected_rather_than_truncated() {
        assert_eq!(NodeId::new(DocumentId::FIRST, 1 << BACKEND_BITS), None);
        assert!(NodeId::new(DocumentId::FIRST, NodeId::MAX_BACKEND_BITS).is_some());
    }

    #[test]
    fn two_documents_never_mint_the_same_handle() {
        let mut seen = std::collections::HashSet::new();
        for raw in 0..8u16 {
            let document = DocumentId::new(raw).expect("in range");
            for bits in 1..8u64 {
                assert!(seen.insert(NodeId::new(document, bits).expect("in range")));
            }
        }
        assert_eq!(seen.len(), 8 * 7);
    }

    #[test]
    fn belongs_to_rejects_a_handle_from_another_window() {
        let first = NodeId::new(DocumentId::FIRST, 3).expect("in range");
        let second_document = DocumentId::new(1).expect("in range");
        assert!(first.belongs_to(DocumentId::FIRST));
        assert!(!first.belongs_to(second_document));
    }

    #[test]
    fn the_layout_leaves_room_for_every_document() {
        assert_eq!(DOCUMENT_COUNT, 1 << (64 - BACKEND_BITS));
    }
}
