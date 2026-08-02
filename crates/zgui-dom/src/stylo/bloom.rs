//! The ancestor hashes selector matching filters candidates with.
//!
//! Before matching a descendant combinator the matcher asks a bloom filter of the ancestor chain
//! whether the ancestor it is looking for could be up there at all. A negative answer is certain and
//! costs one word, which is what makes a stylesheet full of `.card .title` rules affordable; a
//! positive answer means the ancestor walk runs as usual. Filling that filter is one method, and it
//! has to add the same hashes the matcher will later look for — the element's local name, its
//! identifier and each of its classes — or the filter answers "no" for an ancestor that is really
//! there and the rule silently stops applying.
//!
//! Deriving the hashes by hand is exactly the mistake that would cause: the matcher hashes through
//! the engine's own helper, so this defers to that helper rather than reproducing it.
//!
//! # The other filter, and why it is switched off
//!
//! An element may also publish one word summarising the names, classes and attribute names of its
//! whole subtree, so that a query can skip a subtree without descending into it. Maintaining it
//! means updating every ancestor of every insertion, and nothing in this document is bounded by the
//! walk it would save. The answer here is therefore "every hash may be down there", which is the
//! same answer as not having one — pinned explicitly rather than inherited, so that a future
//! release changing the default cannot quietly change what this document does.

use selectors::bloom::{BLOOM_HASH_MASK, BloomFilter};
use style::bloom::each_relevant_element_hash;

use crate::node::handle::Node;

/// The subtree summary this document publishes: all bits, meaning "anything may be below here".
pub const SUBTREE_FILTER_UNFILTERED: u64 = u64::MAX;

impl Node<'_> {
    /// Adds this element's own name, identifier and class hashes to `filter`.
    ///
    /// Returns whether the filter is usable afterwards, which it always is here: the only reason to
    /// answer otherwise is an element whose identity cannot be enumerated, and every element's can.
    pub fn add_bloom_hashes(self, filter: &mut BloomFilter) -> bool {
        each_relevant_element_hash(self, |hash| filter.insert_hash(hash & BLOOM_HASH_MASK));
        true
    }
}

#[cfg(test)]
mod tests {
    use selectors::bloom::{BLOOM_HASH_MASK, BloomFilter};
    use style::values::AtomIdent;
    use stylo_atoms::Atom;
    use zgui_interned::{ClassName, ElementName, Ident};

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    /// The masked hash of a tag name, as the matcher computes it.
    fn tag_hash(text: &str) -> u32 {
        web_atoms::LocalName::from(text).get_hash() & BLOOM_HASH_MASK
    }

    /// The masked hash of a class name, as the matcher computes it.
    fn class_hash(text: &str) -> u32 {
        AtomIdent::from(text).get_hash() & BLOOM_HASH_MASK
    }

    /// The masked hash of an identifier, as the matcher computes it.
    fn ident_hash(text: &str) -> u32 {
        Atom::from(text).get_hash() & BLOOM_HASH_MASK
    }

    #[test]
    fn every_name_the_matcher_looks_for_is_in_the_filter_this_fills() {
        let mut document = Document::new();
        let card = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("card"),
        );
        document.set_classes(card, &[ClassName::new("wide")]);
        document.set_id(card, Some(Ident::new("main")));

        let mut filter = BloomFilter::new();
        assert!(document.node(card).add_bloom_hashes(&mut filter));

        for hash in [tag_hash("card"), class_hash("wide"), ident_hash("main")] {
            assert!(
                filter.might_contain_hash(hash),
                "a name the element carries must survive the filter, or rules naming it stop \
                 applying with no diagnostic"
            );
        }
    }

    #[test]
    fn a_name_the_element_does_not_carry_is_filtered_out() {
        let mut document = Document::new();
        let card = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("card"),
        );
        let mut filter = BloomFilter::new();
        document.node(card).add_bloom_hashes(&mut filter);
        assert!(!filter.might_contain_hash(class_hash("no-rule-uses-this-name")));
    }
}
