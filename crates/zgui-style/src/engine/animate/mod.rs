//! The frame's animation tick, and the one mark only this crate can write.
//!
//! Two entry points, and the split between them is the whole architecture of the animation stage.
//! [`StyleEngine::animation_tick`] does the mechanical work — advance the clock, move every running
//! animation on, sample what it now evaluates to — and reports. [`StyleEngine::mark_animation_restyle`]
//! is what a caller uses once it has *decided* that an element's animation cannot be expressed as a
//! repaint: it tells the engine to run that element's cascade again, replacing only the animation
//! and transition declarations rather than matching any selector.
//!
//! Neither decides which elements are which. That decision needs no engine at all, and keeping it
//! out of here is what lets it be made and tested somewhere that names none.
//!
//! | Module | Contents |
//! |---|---|
//! | [`descent`] | the flag that gets the animation-only traversal from the root to the element |

pub mod descent;

use style::dom::{TElement, TNode};
use style::invalidation::element::restyle_hints::RestyleHint;
use style::selector_parser::SnapshotMap;
use style::traversal_flags::TraversalFlags;
use zgui_dom::{Document, NodeIndex};

use crate::driver::animations::tick;
use crate::driver::animations::{AnimationReport, AnimationTime};
use crate::engine::StyleEngine;
use crate::engine::guards;

impl StyleEngine {
    /// Advances every running animation to `now` and reports what is still running.
    ///
    /// Returns an empty report immediately when nothing is animating, which is every frame of a
    /// document at rest.
    pub fn animation_tick(&mut self, document: &Document, now: AnimationTime) -> AnimationReport {
        if self.animations.is_empty() {
            self.animations.set_now(now);
            return AnimationReport::default();
        }
        let snapshots = SnapshotMap::new();
        let read = self.lock.read();
        let context = crate::driver::context::build(
            &self.stylist,
            guards::guards(&read),
            &snapshots,
            self.animations.shared(),
            now,
            TraversalFlags::empty(),
        );
        tick::advance(&mut self.animations, document, &context, now.0)
    }

    /// Asks for one element's cascade to be run again for its animations.
    ///
    /// The hint asks for the animation and transition declarations to be replaced and for nothing
    /// else, so the element's selector matches are kept and the frame costs a cascade rather than a
    /// match. Only the animation-only traversal processes it, so the call records the element for
    /// the next restyle, which raises the descent flag that traversal reads on every ancestor of
    /// wherever the element stands by then. The flag is how the traversal gets from the root to an
    /// element that could be anywhere, and the hint alone reaches nothing.
    pub fn mark_animation_restyle(&mut self, document: &Document, index: NodeIndex) {
        let node = document.node(index);
        let Some(element) = node.as_element() else {
            return;
        };
        // The borrow is scoped rather than dropped by name. What it guards is a borrow flag that
        // exists only in an unoptimised build, so a `drop` call here would be a no-op the optimiser
        // sees through and a lint reports — while a block ends the borrow in both builds and says
        // why: nothing below this point may hold the element's data.
        {
            let mut data = element.ensure_style_data();
            data.hint
                .insert(RestyleHint::RESTYLE_CSS_ANIMATIONS | RestyleHint::RESTYLE_CSS_TRANSITIONS);
        }
        self.animation_marks.push(node.key());
    }

    /// How many marked elements wait out of the document for a restyle that finds them in it.
    #[must_use]
    pub fn animation_waiting(&self) -> usize {
        self.animation_waiting.len()
    }

    /// Raises the descent flags from where every marked element stands now, and answers whether
    /// the animation-only traversal has an element to reach.
    ///
    /// An element moved since its mark has flags on the ancestors it left, and one taken out of
    /// the document has none the traversal can follow. The first is reached from its new place;
    /// the second waits, hint and all, for a restyle that finds it in the document again. Left
    /// unreached, its hint meets the ordinary traversal, which refuses it.
    pub(crate) fn settle_animation_marks(&mut self, document: &Document) -> bool {
        let mut marked = std::mem::take(&mut self.animation_marks);
        marked.append(&mut self.animation_waiting);
        let root = document.root().map(|root| root.index());
        let mut reachable = false;
        for key in marked {
            let Some(index) = document.store().index_of(key) else {
                continue;
            };
            let node = document.node(index);
            let Some(element) = node.as_element() else {
                continue;
            };
            let pending = element
                .borrow_data()
                .is_some_and(|data| data.hint.has_animation_hint());
            if !pending {
                continue;
            }
            if descent::top_of(node).is_some_and(|top| Some(top) == root) {
                descent::raise_to_root(node);
                reachable = true;
            } else if !self.animation_waiting.contains(&key) {
                self.animation_waiting.push(key);
            }
        }
        reachable
    }
}
