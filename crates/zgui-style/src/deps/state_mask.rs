//! Which interaction-state bits any active selector could match one element on.
//!
//! Answered per element rather than per document, because a document-wide answer is worthless: any
//! real stylesheet styles `:hover` somewhere, so a document-wide mask has the hover bit set and
//! every hover anywhere takes the slow path.
//!
//! The narrowing is what makes the answer per element: the dependency index is bucketed by the
//! element's root-ness, its identifier, each of its classes and its local name, and a lookup only
//! visits the buckets that element falls in. That is also why the answer stops being an answer the
//! moment any of those change — which the document handles, by dropping its cached answer inside
//! the write that changes the identity it was narrowed from.

use selectors::matching::QuirksMode;
use style::stylist::Stylist;
use stylo_dom::ElementState;
use zgui_dom::Node;

/// The state bits any selector that could match `element` depends on.
pub(crate) fn states_for(stylist: &Stylist, element: Node<'_>) -> ElementState {
    let mut states = ElementState::empty();
    for (data, _origin) in stylist.iter_origins() {
        data.invalidation_map().state_affecting_selectors.lookup(
            element,
            QuirksMode::NoQuirks,
            None,
            |dependency| {
                states |= dependency.state;
                true
            },
        );
    }
    states
}
