//! Deciding that a change cannot affect any computed style.
//!
//! Most of what a running interface writes to a document cannot change a single computed value.
//! A component library styles `:hover` and `:focus-visible`; nothing in it styles `:read-only`,
//! `:in-range` or `:indeterminate`, and nothing matches the data attributes its variants are driven
//! by. A change to something no selector depends on can be applied without recording the previous
//! value, without marking any ancestor and without entering the restyle traversal at all.
//!
//! The answer comes from the crate that owns the compiled rule set, which is two layers above this
//! one, so it arrives as a trait object handed to each call rather than as a type. The default
//! answers "yes, everything matters", which is correct and merely not cheap.
//!
//! # The cached answer, and the one way it goes wrong
//!
//! [`StyleFilter::states_for`] is answered per element rather than per document, because a
//! document-wide answer is worthless: any real stylesheet styles `:hover` somewhere, so a
//! document-wide mask has the hover bit set and every hover anywhere takes the slow path. Narrowing
//! it costs a lookup through the rule set's dependency index, so the answer is cached on the node.
//!
//! **The cached answer depends on the element's own identity, not only on the stylesheet set.** The
//! index is bucketed by the element's root-ness, its identifier, each of its classes and its local
//! name, so adding a class can widen the mask. An element under `.btn:hover { … }` has an empty
//! mask before `.btn` is added and the hover bit after it. A cache that survives a class change
//! therefore reports that hover cannot matter, the hover write is skipped, and the element keeps
//! the wrong colour — with no panic, no log and no counter to notice it by. So the cache is dropped
//! inside the write that changes the identity it was computed from, not afterwards and not on a
//! sweep:
//!
//! * a class change on that element,
//! * an identifier change on that element,
//! * a reparent that changes whether the element is the root,
//! * a change to the stylesheet set, which drops every cached answer at once.
//!
//! A local-name change is not in the list, because a node's name is written when it is created and
//! never afterwards.

use stylo_dom::ElementState;
use zgui_interned::{AttrName, ClassName};

use crate::node::handle::Node;

/// Decides whether a mutation can change any element's computed style.
///
/// Implemented by the crate that holds the compiled rule set, and **handed to the mutation API by
/// reference** rather than installed on the document. That is not a style choice: the rule set this
/// is answered from owns a font-metrics source that is shareable but not sendable, so a rule set
/// cannot be stored anywhere that a document can be moved between threads. Passing it in also keeps
/// the answer and the rule set it came from in one place, which is what stops a filter outliving the
/// stylesheet set it describes.
///
/// Every method has a default that answers "this may matter", so an implementation that gets one
/// wrong in that direction is slow and an implementation that gets one wrong in the other direction
/// is incorrect — a change that is filtered out is a change nothing ever restyles for.
pub trait StyleFilter {
    /// The interaction-state bits any selector that could match `element` depends on.
    ///
    /// Writing a state bit outside this mask cannot change what matches, so it needs no snapshot
    /// and no restyle. The answer is narrowed by the element's own identifier, classes, local name
    /// and root-ness, which is why it is cached per element and dropped when any of those change.
    fn states_for(&self, element: Node<'_>) -> ElementState {
        let _ = element;
        ElementState::all()
    }

    /// Whether any selector in the active stylesheet set mentions this class name.
    fn names_class(&self, class: ClassName) -> bool {
        let _ = class;
        true
    }

    /// Whether any selector mentions this attribute name, or matches attributes without naming
    /// one.
    fn names_attr(&self, attr: AttrName) -> bool {
        let _ = attr;
        true
    }

    /// Whether the filter's answers are currently unusable.
    ///
    /// Set for the one frame in which the stylesheet set changed, during which the dependency index
    /// still describes the previous set and every mutation must take the full path. The default is
    /// `true`, so a filter that never says otherwise never narrows anything.
    ///
    /// This is not only advice to the caller: while it holds, the per-element cache neither asks
    /// this filter nor stores what it would have said, so no answer narrowed against a stylesheet
    /// set that is no longer current can outlive the frame that changed it.
    fn is_disabled(&self) -> bool {
        true
    }
}

/// The filter installed by default, for which every change may matter.
///
/// Every answer is the trait's own default, which is the safe direction: a document with no rule
/// set behind it cannot prove that anything is irrelevant.
pub struct EverythingMatters;

impl StyleFilter for EverythingMatters {}

/// One element's cached [`StyleFilter::states_for`] answer.
///
/// [`None`] means the answer has not been computed, or has been dropped because the element's
/// identity changed. This is the value type of a sparse column, so an element nothing ever writes
/// to costs nothing at all.
pub type StateMaskSlot = Option<ElementState>;

#[cfg(test)]
mod tests {
    use stylo_dom::ElementState;
    use zgui_interned::{AttrName, ClassName, ElementName};

    use super::{EverythingMatters, StyleFilter};
    use crate::arena::document::Document;
    use crate::node::handle::Node;
    use crate::node::kind::NodeKind;

    /// A filter for which only hover can ever matter, and only on an element carrying `chosen`.
    struct OnlyChosenHovers;

    impl StyleFilter for OnlyChosenHovers {
        fn is_disabled(&self) -> bool {
            false
        }

        fn states_for(&self, element: Node<'_>) -> ElementState {
            let chosen = element
                .store()
                .classes_of(element.index())
                .iter()
                .any(|class| class.as_ref() == "chosen");
            if chosen {
                ElementState::HOVER
            } else {
                ElementState::empty()
            }
        }
    }

    #[test]
    fn the_default_filter_proves_nothing_irrelevant() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let filter = EverythingMatters;
        assert_eq!(
            document.store_mut().states_for(root, &filter),
            ElementState::all()
        );
        assert!(filter.names_class(ClassName::new("anything")));
        assert!(filter.names_attr(AttrName::new("anything")));
        assert!(
            filter.is_disabled(),
            "a document with no rule set behind it must never narrow anything"
        );
    }

    #[test]
    fn a_class_write_drops_the_cached_answer_and_the_next_read_is_recomputed() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let filter = OnlyChosenHovers;

        assert_eq!(
            document.store_mut().states_for(root, &filter),
            ElementState::empty()
        );
        document.set_classes(root, &[ClassName::new("chosen")]);
        assert_eq!(
            document.store_mut().states_for(root, &filter),
            ElementState::HOVER,
            "the write that changed the identity the answer was narrowed by dropped it"
        );
    }

    #[test]
    fn a_cached_answer_is_returned_without_asking_again() {
        /// A filter that refuses to answer twice, so a second lookup is a visible failure.
        struct OnceOnly(std::cell::Cell<bool>);

        impl StyleFilter for OnceOnly {
            fn is_disabled(&self) -> bool {
                false
            }

            fn states_for(&self, _element: Node<'_>) -> ElementState {
                assert!(!self.0.replace(true), "the cached answer was not used");
                ElementState::HOVER
            }
        }

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let filter = OnceOnly(std::cell::Cell::new(false));
        assert_eq!(
            document.store_mut().states_for(root, &filter),
            ElementState::HOVER
        );
        assert_eq!(
            document.store_mut().states_for(root, &filter),
            ElementState::HOVER
        );
    }

    #[test]
    fn a_filter_that_reports_itself_unusable_is_neither_asked_nor_cached() {
        /// A filter whose answers are stale, and which counts being asked for one anyway.
        struct Stale(std::cell::Cell<usize>);

        impl StyleFilter for Stale {
            fn states_for(&self, _element: Node<'_>) -> ElementState {
                self.0.set(self.0.get() + 1);
                ElementState::empty()
            }
        }

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );

        let stale = Stale(std::cell::Cell::new(0));
        assert_eq!(
            document.store_mut().states_for(root, &stale),
            ElementState::all(),
            "an index that describes rules which are no longer the rules narrows nothing"
        );
        assert_eq!(stale.0.get(), 0, "and it is not consulted at all");

        // The answer that was not given must also not have been stored: an element carrying
        // `chosen` narrows to the hover bit, and it would report the empty set if the disabled read
        // had left one behind.
        document.set_classes(root, &[ClassName::new("chosen")]);
        assert_eq!(
            document.store_mut().states_for(root, &OnlyChosenHovers),
            ElementState::HOVER
        );
    }
}
