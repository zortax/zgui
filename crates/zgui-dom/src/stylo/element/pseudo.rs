//! Which pseudo-elements an element may generate.
//!
//! There is no pseudo-element *node* in this document. `::before` and `::after` are boxes, built
//! from the originating element's own style data, so nothing here has to answer questions about a
//! pseudo-element's position among its siblings — a design where a pseudo-element were a node would
//! shift `:nth-child` and `+` for every element in the document that generates content.
//!
//! What is left is one question the engine asks per eager pseudo-element per restyled element:
//! is it worth resolving a style for this one at all? The answer is per variant, and getting it
//! wrong in the cheap direction is not an optimisation but a deletion.
//!
//! * **`::before` and `::after` — yes.** Their style is not a second cascade competing with the
//!   element's own, because there is no node for it to be the style *of*. Saying no here
//!   short-circuits the match, leaves the pseudo-element style permanently absent, and makes
//!   generated content impossible to render at all. The cost of saying yes is one applicable-rule
//!   collection per element per restyle, and for the overwhelmingly common element that generates
//!   nothing the engine throws the resolved style away before storing it.
//! * **`::selection` — yes.** It stays on the originating element and something reads it.
//! * **`::first-letter` — no, for now.** Inline layout generates no first-letter box, so answering
//!   yes computes and stores a style nothing reads, on every element, on every restyle. This one
//!   arm is the whole reason the question is answered here rather than left at the engine's
//!   default, and it flips in the same change that adds the box.
//!
//! There is no `::first-line` arm, because the engine's servo build has no such variant: the
//! selector does not parse, so a rule using it is dropped whole, and naming the variant here would
//! not compile.

use style::selector_parser::PseudoElement;

use crate::node::handle::Node;

impl Node<'_> {
    /// Whether resolving a style for `pseudo` on this element is worth doing.
    ///
    /// Asked only about eagerly cascaded pseudo-elements.
    pub fn may_generate(self, pseudo: &PseudoElement) -> bool {
        match pseudo {
            PseudoElement::Before | PseudoElement::After | PseudoElement::Selection => true,
            PseudoElement::FirstLetter => false,
            // Every other variant is lazily cascaded, so the engine does not ask about it; if a
            // future release makes one eager, the safe answer is the engine's own default.
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use style::selector_parser::PseudoElement;
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn the_generated_content_pseudo_elements_are_resolved_and_the_first_letter_is_not() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let node = document.node(root);
        assert!(node.may_generate(&PseudoElement::Before));
        assert!(node.may_generate(&PseudoElement::After));
        assert!(node.may_generate(&PseudoElement::Selection));
        assert!(!node.may_generate(&PseudoElement::FirstLetter));
    }

    #[test]
    fn the_four_eager_pseudo_elements_are_the_ones_this_answers_about() {
        // The engine keeps its eagerly cascaded variants first in its own enumeration so that each
        // one has a fixed index; if that set ever changes, the answers above stop covering it.
        for pseudo in [
            PseudoElement::After,
            PseudoElement::Before,
            PseudoElement::Selection,
            PseudoElement::FirstLetter,
        ] {
            assert!(pseudo.is_eager());
        }
    }
}
