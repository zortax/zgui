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

use style::dom::TNode;
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
    /// match. Only the animation-only traversal will process it, so the same call records that the
    /// next restyle owes one, and raises the descent flag that traversal reads on every ancestor —
    /// the flag is how the traversal gets from the root to an element that could be anywhere, and
    /// the hint alone reaches nothing.
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
        descent::raise_to_root(node);
        self.animation_restyle_owed = true;
    }
}
