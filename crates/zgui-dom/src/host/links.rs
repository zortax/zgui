//! Which elements are links.

use crate::node::handle::Node;

/// Decides which elements are links, and which of those have been visited.
///
/// `:link`, `:visited` and `:any-link` are the only selectors in CSS whose answer depends on a
/// concept no document core can define for itself: what counts as a link is a property of the
/// document language, and whether one has been visited is a property of a browsing history.
///
/// # How the answer reaches selector matching
///
/// The resolver is not consulted during matching. It is consulted when a node's attributes change,
/// and its answer is folded into that node's interaction state — the same word every other state
/// pseudo-class is answered from. That is deliberate and it is a correctness requirement rather
/// than a performance one: the style engine invalidates `:link` and `:visited` by comparing state
/// words across a mutation, so an answer that lived only inside the matcher would change without
/// invalidating anything and the old style would stay on the screen.
///
/// The consequence for an implementor is that the answer must be a pure function of what the
/// document holds. Changing what the resolver *would* say — a page finishing loading, a history
/// entry appearing — takes effect when the affected nodes are refreshed, not spontaneously.
pub trait LinkResolver: Send + Sync + 'static {
    /// Whether `element` is a link at all.
    fn is_link(&self, element: Node<'_>) -> bool;

    /// Whether `element` is a link that has been visited.
    ///
    /// Only asked of elements [`LinkResolver::is_link`] accepted. The default answer is "no",
    /// which is what a consumer with no browsing history should keep: a document that reports
    /// every link unvisited leaks nothing about where its user has been.
    fn is_visited(&self, element: Node<'_>) -> bool {
        let _ = element;
        false
    }
}

/// The resolver zgui installs by default, for which nothing is a link.
pub struct NoLinks;

impl LinkResolver for NoLinks {
    fn is_link(&self, _element: Node<'_>) -> bool {
        false
    }
}
