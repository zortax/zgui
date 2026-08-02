//! Interaction state, language and custom states — everything the state pseudo-classes read.
//!
//! Thirty-three of the thirty-six state pseudo-classes are one bit of one word, and that word is
//! the only place any of them is stored: there is no second hover set and no second focus set
//! anywhere in this framework. Input routing writes the bits, selector matching reads them, and the
//! style engine invalidates by comparing the word across a mutation — which is exactly why nothing
//! that answers a state pseudo-class may live outside it. An answer computed on the fly during
//! matching would change without the word changing, so nothing would be invalidated and the old
//! style would stay on the screen.
//!
//! That is the reason the link pseudo-classes are folded into the word rather than asked of the
//! host at match time; the folding happens when a node's attributes change.

use style::selector_parser::{AttrValue, Lang};
use stylo_dom::ElementState;

use crate::node::handle::Node;

impl Node<'_> {
    /// This element's interaction state.
    pub fn element_state(self) -> ElementState {
        self.record().state()
    }

    /// This element's declared language, if it declares one.
    ///
    /// Always [`None`]: a language is a document-language attribute, and this document has no
    /// document language. A consumer that wants `:lang()` to answer gives its elements a language
    /// through the same hook that gives them presentational hints, and this becomes its reader.
    pub fn lang_attribute(self) -> Option<AttrValue> {
        None
    }

    /// Whether this element's resolved language matches `value`.
    ///
    /// Always `false`, and not `true` by accident: with no language anywhere, `:lang(en)` and
    /// `:lang(fr)` must both fail rather than both succeed.
    pub fn matches_lang(self, override_lang: Option<Option<AttrValue>>, value: &Lang) -> bool {
        let _ = (override_lang, value);
        false
    }

    /// Whether this element carries the author-defined state `name`.
    ///
    /// Author-defined states are not part of the interaction-state word: the word is a closed set
    /// of bits with a fixed meaning each, and an author's own vocabulary has neither. They live in
    /// a column of their own, and the invalidation they need is supplied by whatever set them
    /// rather than by the engine's snapshot — see
    /// [`Edit::set_custom_state`](crate::Edit::set_custom_state), which is the only way one is
    /// written.
    pub fn has_custom_state_named(self, name: &style::values::AtomIdent) -> bool {
        self.store()
            .columns()
            .custom_states
            .get(self.key())
            .is_some_and(|states| states.contains(name))
    }

    /// Runs `callback` over every author-defined state this element carries.
    pub fn each_custom_state(self, mut callback: impl FnMut(&style::values::AtomIdent)) {
        let Some(states) = self.store().columns().custom_states.get(self.key()) else {
            return;
        };
        for name in states.iter() {
            callback(name);
        }
    }
}
