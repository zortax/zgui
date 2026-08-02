//! The identity of everything a node's accessible description depends on.
//!
//! The same idea as the paint key one level over, with one extra part. A style key can be a set of
//! addresses because computed values are shared and immutable. An accessible name is not: editing a
//! text node changes what a screen reader would say without changing any style group and without
//! changing any semantics record, so identity alone would report "nothing changed" for the one
//! change a user would most notice. Hence the content hash.

/// Identity of what a node's accessible description is derived from.
///
/// Written once per node per restyle, and compared against the previous frame's value to decide
/// whether the accessibility projection has to be rebuilt for this node.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct A11yKey {
    /// The group of computed values the projection reads — visibility, direction, and the rest of
    /// what decides whether a node is exposed at all.
    pub style: usize,
    /// The node's semantics record, or zero when it has none.
    pub semantics: usize,
    /// A hash of the text the projection would read out.
    ///
    /// The one part that is a hash rather than an address, because text is edited in place: without
    /// it, retyping a label changes what is announced and nothing observes that it did.
    pub content: u64,
}

impl A11yKey {
    /// The key of a node whose accessible description has never been computed.
    pub const UNPROJECTED: Self = Self {
        style: 0,
        semantics: 0,
        content: 0,
    };
}

#[cfg(test)]
mod tests {
    use super::A11yKey;

    #[test]
    fn the_unprojected_key_is_the_default() {
        assert_eq!(A11yKey::default(), A11yKey::UNPROJECTED);
    }

    #[test]
    fn a_text_edit_changes_the_key_even_when_nothing_else_moves() {
        let before = A11yKey {
            style: 0x40,
            semantics: 0x80,
            content: 0xdead,
        };
        let after = A11yKey {
            content: 0xbeef,
            ..before
        };
        assert_ne!(before, after);
    }
}
