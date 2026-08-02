//! Reading the style engine's per-element output back off the tree.
//!
//! The engine writes its answer into the record, in a form only it can interpret. Everything above
//! this crate wants three things out of it — the computed style, the style of the two generated
//! pseudo-elements, and how much of the frame the change invalidated — and every one of them has to
//! be *copied out* rather than borrowed, because the borrow is only valid while the element is not
//! being restyled.
//!
//! # Why the answers are copied and not borrowed
//!
//! The engine leaves its per-element data behind a borrow discipline it owns, and holding a borrow
//! across anything else is how a later stage ends up reading data a traversal is rewriting. So the
//! accessors here take a copy — a reference-counted style is one pointer, damage is two bytes — and
//! nothing above this crate is ever handed a live borrow.

use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;
use style::selector_parser::{PseudoElement, RestyleDamage};

use crate::node::atomics;
use crate::node::handle::Node;

impl Node<'_> {
    /// Whether this element's style data has been established and not cleared.
    ///
    /// The data itself is stored inline in every record, so its presence answers nothing: the naive
    /// answer is "always", and the engine bounds one of its subtree walks on exactly this question.
    /// The bit is the real answer, written by the same two calls that establish and clear the data.
    pub fn is_styled(self) -> bool {
        self.record().has_atomic(atomics::STYLED)
    }

    /// This element's computed style, if it has one.
    pub fn primary_style(self) -> Option<ServoArc<ComputedValues>> {
        self.is_styled()
            .then(|| self.record().data().borrow().styles.get_primary().cloned())
            .flatten()
    }

    /// The computed style of one of this element's eagerly cascaded pseudo-elements.
    ///
    /// [`None`] means the pseudo-element generates nothing — either no rule matched it, or the rule
    /// that did leaves it with no content to place. That is the answer box construction keys off:
    /// a pseudo-element with no style has no box.
    pub fn pseudo_style(self, pseudo: &PseudoElement) -> Option<ServoArc<ComputedValues>> {
        self.is_styled()
            .then(|| {
                self.record()
                    .data()
                    .borrow()
                    .styles
                    .pseudos
                    .get(pseudo)
                    .cloned()
            })
            .flatten()
    }

    /// The style of the box this element generates *before* its content, if it generates one.
    ///
    /// [`None`] covers both ways of generating nothing: no rule matched the pseudo-element at all,
    /// and a rule matched but left it with a `display` of `none` or with no content to place. A
    /// caller therefore builds a box exactly when this is [`Some`] and needs no further test.
    pub fn before_style(self) -> Option<ServoArc<ComputedValues>> {
        self.existing_pseudo_style(&PseudoElement::Before)
    }

    /// The style of the box this element generates *after* its content, if it generates one.
    ///
    /// The same rule as [`Node::before_style`]: [`Some`] means a box, and nothing else does.
    pub fn after_style(self) -> Option<ServoArc<ComputedValues>> {
        self.existing_pseudo_style(&PseudoElement::After)
    }

    /// The stored style of one pseudo-element, dropped unless the pseudo-element would exist.
    fn existing_pseudo_style(self, pseudo: &PseudoElement) -> Option<ServoArc<ComputedValues>> {
        let style = self.pseudo_style(pseudo)?;
        pseudo.should_exist(&style).then_some(style)
    }

    /// How much of the frame this element's last restyle invalidated.
    pub fn restyle_damage(self) -> RestyleDamage {
        if !self.is_styled() {
            return RestyleDamage::empty();
        }
        self.record().data().borrow().damage
    }

    /// Whether the engine *re*styled this element in the last traversal.
    ///
    /// Restyled, not styled: the flag is set only for an element that already had a style, so the
    /// first pass over a fresh document reports none however much work it did. A budget written
    /// against this number has to say which of the two it means.
    pub fn was_restyled(self) -> bool {
        self.is_styled() && self.record().data().borrow().is_restyle()
    }

    /// Whether this element came out of a traversal with a computed style at all.
    pub fn has_styles(self) -> bool {
        self.is_styled() && self.record().data().borrow().has_styles()
    }
}
