//! Namespace identity, as an index rather than a name.

/// Which namespace an element or attribute belongs to, as an index into the document language's
/// namespace table.
///
/// A namespace is a URI, and a document uses a handful of them at most, so storing the URI per
/// node would be eight bytes of pointer for information that fits in one byte of index. The table
/// the index refers to belongs to whatever defines the document language: this type is only the
/// index, and it carries no opinion about what any particular index means.
///
/// [`NamespaceId::NONE`] is reserved for "no namespace", which every language has and which is
/// therefore the default.
///
/// ```
/// use zgui_interned::NamespaceId;
///
/// assert_eq!(NamespaceId::default(), NamespaceId::NONE);
/// assert!(NamespaceId::NONE.is_none());
///
/// let first = NamespaceId::from_index(1);
/// assert_eq!(first.index(), 1);
/// assert!(!first.is_none());
/// assert_eq!(size_of::<NamespaceId>(), 1);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct NamespaceId(u8);

impl NamespaceId {
    /// No namespace, which is where an unqualified name lives.
    pub const NONE: Self = Self(0);

    /// How many distinct namespaces one document language can name.
    pub const CAPACITY: usize = u8::MAX as usize + 1;

    /// The identifier for entry `index` of the namespace table.
    pub const fn from_index(index: u8) -> Self {
        Self(index)
    }

    /// The table entry this identifier refers to.
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Whether this is [`NamespaceId::NONE`].
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }
}

#[cfg(test)]
mod tests {
    use super::NamespaceId;

    #[test]
    fn the_default_is_no_namespace() {
        assert_eq!(NamespaceId::default(), NamespaceId::NONE);
        assert_eq!(NamespaceId::NONE.index(), 0);
        assert!(NamespaceId::NONE.is_none());
    }

    #[test]
    fn an_index_round_trips() {
        for index in 0..=u8::MAX {
            assert_eq!(NamespaceId::from_index(index).index(), index);
        }
    }

    #[test]
    fn identifiers_are_one_byte_and_order_by_index() {
        assert_eq!(size_of::<NamespaceId>(), 1);
        assert!(NamespaceId::from_index(1) < NamespaceId::from_index(2));
    }
}
