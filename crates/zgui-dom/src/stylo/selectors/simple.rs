//! Identity and state: the simple selectors, and the thirty-six pseudo-classes.
//!
//! Everything here is unreachable on a node that is not an element, and the assertion at the head of
//! each function is what keeps that true: the matcher reaches an element only through the parent
//! link, the element-only sibling chain or the matching context's roots, and every one of those
//! already filters to elements. Widening one of those entry points without noticing would produce a
//! wrong match rather than a failure, so each of these says so out loud in a debug build.
//!
//! # Why the state pseudo-classes are one line
//!
//! Thirty-three of the thirty-six map onto a bit of the element's interaction state, and the engine
//! itself supplies the mapping — so the implementation is a mask test and not a thirty-three-arm
//! match that could disagree with the engine's own invalidation about which bit means what.
//!
//! Three do not map onto a bit and need arms of their own: the language pseudo-class, the custom
//! state pseudo-class, and one engine-internal legacy class about table borders.

use selectors::attr::CaseSensitivity;
use selectors::matching::MatchingContext;
use style::CaseSensitivityExt;
use style::selector_parser::{NonTSPseudoClass, PseudoElement, SelectorImpl};
use style::values::AtomIdent;

use crate::node::handle::Node;
use crate::stylo::selectors::expect_element;

impl Node<'_> {
    /// Whether this element's tag name is `name`.
    pub fn matches_local_name(self, name: &web_atoms::LocalName) -> bool {
        expect_element(self);
        &self.tag_name().0 == name
    }

    /// Whether this element is in namespace `namespace`.
    pub fn matches_namespace(self, namespace: &web_atoms::Namespace) -> bool {
        expect_element(self);
        self.namespace_uri() == namespace
    }

    /// Whether this element and `other` have the same tag name and namespace.
    pub fn matches_same_type(self, other: Node<'_>) -> bool {
        expect_element(self);
        self.tag_name() == other.tag_name()
            && self.record().namespace_id() == other.record().namespace_id()
    }

    /// Whether this element's identifier is `id`.
    pub fn matches_id(self, id: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        expect_element(self);
        self.record()
            .id_attr()
            .and_then(|ident| self.store().idents().resolve(ident))
            .is_some_and(|own| case_sensitivity.eq_atom(own, id))
    }

    /// Whether this element carries the class `name`.
    ///
    /// A scan over a pre-split, pre-interned run: nothing here splits a class attribute or interns
    /// a name, because both happened once when the classes were written.
    pub fn matches_class(self, name: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        expect_element(self);
        self.store()
            .classes_of(self.index())
            .iter()
            .any(|own| case_sensitivity.eq_atom(own, name))
    }

    /// Whether this element matches the state pseudo-class `class`.
    pub fn matches_pseudo_class(
        self,
        class: &NonTSPseudoClass,
        context: &mut MatchingContext<SelectorImpl>,
    ) -> bool {
        expect_element(self);
        let _ = context;
        let flag = class.state_flag();
        if !flag.is_empty() {
            return self.element_state().intersects(flag);
        }
        match class {
            NonTSPseudoClass::Lang(lang) => self.matches_lang(None, lang),
            NonTSPseudoClass::CustomState(state) => self.has_custom_state_named(&state.0),
            // The remaining arm is the engine's own legacy class for tables with a non-zero border
            // attribute, which is a document-language notion this document does not have.
            _ => false,
        }
    }

    /// Whether this element *is* a pseudo-element being matched as one.
    ///
    /// Always `false`, and it is the design rather than a stub: `::before` and `::after` are boxes
    /// built from the originating element's style, so there is no node for a pseudo-element
    /// selector to be matched against.
    pub fn matches_pseudo_element(
        self,
        pseudo: &PseudoElement,
        context: &mut MatchingContext<SelectorImpl>,
    ) -> bool {
        let _ = (pseudo, context);
        false
    }

    /// Whether this element is a link.
    ///
    /// Read from the element's interaction state, which is where the installed link resolver's
    /// answer was folded when the element's attributes were last written. Consulting the resolver
    /// here instead would give the answer a second home, and a home the style engine's invalidation
    /// cannot see.
    pub fn matches_link(self) -> bool {
        expect_element(self);
        self.element_state()
            .intersects(stylo_dom::ElementState::VISITED_OR_UNVISITED)
    }
}
