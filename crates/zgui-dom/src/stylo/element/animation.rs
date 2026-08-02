//! The animation questions the cascade asks, answered from the document's own animation set.
//!
//! A running animation reaches the cascade the same way a stylesheet declaration does: as a
//! declaration block at its own cascade origin, rebuilt for the time the frame is being styled at.
//! The set those blocks are generated from is the one carried on the shared style context, so
//! nothing here is stored on the node — an element's answer is a lookup by its slot number in a
//! table the frame's driver owns.
//!
//! Most animations never take this path at all. One that moves only what a box is painted in, and
//! one that moves only where it is drawn, are both interpolated outside the cascade and written
//! into a table of the element's own, because re-cascading a document sixty times a second to move
//! one opacity or slide one bar is the cost that split exists to avoid. What is left — anything the
//! cascade must see, because a descendant inherits it, because a size is computed from it, or
//! because it changes what kind of box the element is at all — arrives here.

use servo_arc::Arc as ServoArc;
use style::animation::AnimationSetKey;
use style::context::SharedStyleContext;
use style::dom::TNode;
use style::properties::PropertyDeclarationBlock;
use style::selector_parser::PseudoElement;
use style::shared_lock::Locked;

use crate::node::handle::Node;

impl Node<'_> {
    /// The declaration block this element's running animations contribute.
    pub fn engine_animation_rule(
        self,
        context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        context.animations.get_animation_declarations(
            &self.animation_key(None),
            context.current_time_for_animations,
            self.store().lock(),
        )
    }

    /// The declaration block this element's running transitions contribute.
    pub fn engine_transition_rule(
        self,
        context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        context.animations.get_transition_declarations(
            &self.animation_key(None),
            context.current_time_for_animations,
            self.store().lock(),
        )
    }

    /// Whether anything at all is animating on this element.
    pub fn has_engine_animations(self, context: &SharedStyleContext) -> bool {
        self.has_engine_css_animations(context, None)
            || self.has_engine_css_transitions(context, None)
    }

    /// Whether a CSS animation is running on this element or one of its pseudo-elements.
    pub fn has_engine_css_animations(
        self,
        context: &SharedStyleContext,
        pseudo: Option<PseudoElement>,
    ) -> bool {
        context
            .animations
            .has_active_animations(&self.animation_key(pseudo))
    }

    /// Whether a CSS transition is running on this element or one of its pseudo-elements.
    pub fn has_engine_css_transitions(
        self,
        context: &SharedStyleContext,
        pseudo: Option<PseudoElement>,
    ) -> bool {
        context
            .animations
            .has_active_transitions(&self.animation_key(pseudo))
    }

    /// This element's row in the document's animation set.
    fn animation_key(self, pseudo: Option<PseudoElement>) -> AnimationSetKey {
        AnimationSetKey::new(self.opaque(), pseudo)
    }
}
