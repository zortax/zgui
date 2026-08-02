//! Which document a node handle belongs to.

/// How many documents one process can address at once.
///
/// The limit exists because the identity is carried inside every [`NodeId`](crate::NodeId) rather
/// than beside it, and twelve bits is what is left over once a backend has been given a
/// comfortable fifty-two for its own numbering.
pub const DOCUMENT_COUNT: u32 = 1 << 12;

/// One document's identity.
///
/// Two windows are two documents. The identity travels inside every node handle, so a handle
/// minted for one window can be recognised as foreign by another without a registry, a lookup or
/// a lifetime.
///
/// ```
/// use zgui_view::DocumentId;
///
/// let first = DocumentId::FIRST;
/// let second = DocumentId::new(1).expect("in range");
/// assert_ne!(first, second);
/// assert_eq!(second.get(), 1);
/// assert_eq!(DocumentId::new(u16::MAX), None);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct DocumentId(u16);

impl DocumentId {
    /// The first document a process creates.
    pub const FIRST: Self = Self(0);

    /// Wraps a raw number, rejecting anything at or past [`DOCUMENT_COUNT`].
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

#[cfg(test)]
mod tests {
    use super::{DOCUMENT_COUNT, DocumentId};

    #[test]
    fn the_range_stops_exactly_at_the_count() {
        let last = u16::try_from(DOCUMENT_COUNT - 1).expect("the count fits in sixteen bits");
        assert!(DocumentId::new(last).is_some());
        let past = u16::try_from(DOCUMENT_COUNT).expect("the count fits in sixteen bits");
        assert_eq!(DocumentId::new(past), None);
    }

    #[test]
    fn the_first_document_round_trips() {
        assert_eq!(DocumentId::FIRST.get(), 0);
        assert_eq!(DocumentId::new(0), Some(DocumentId::FIRST));
    }
}
