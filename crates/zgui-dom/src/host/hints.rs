//! Declarations a document language derives from markup attributes.

use selectors::matching::VisitedHandlingMode;
use selectors::sink::Push;
use style::applicable_declarations::ApplicableDeclarationBlock;

use crate::node::handle::Node;

/// Supplies the declarations a document language derives from attributes other than `style`.
///
/// A document language such as HTML gives `width`, `height`, `bgcolor` and `align` a meaning that
/// the cascade has to honour, at a level of its own below every author rule and above the
/// user-agent origin. Nothing in this document knows that those attributes exist, so the
/// declarations they stand for are contributed from outside, once per restyled element.
///
/// The blocks pushed here are the *presentational hint* origin, so an author rule of any
/// specificity overrides them and an `!important` author rule cannot be overridden by them.
///
/// # Implementing this
///
/// `element` is the element being restyled. It is always an element and never a text node, because
/// the only caller is the style engine's per-element hook. `visited` says which half of a link's
/// style is being computed, so an implementation whose hints depend on link state — a
/// `link`/`vlink`/`alink` attribute, say — can answer differently for the two.
///
/// Push order is cascade order within the origin: later blocks win.
pub trait PresentationalHints: Send + Sync + 'static {
    /// Contributes `element`'s attribute-derived declarations to `out`.
    fn hints_for(
        &self,
        element: Node<'_>,
        visited: VisitedHandlingMode,
        out: &mut dyn Push<ApplicableDeclarationBlock>,
    );
}

/// The source zgui installs by default, which contributes nothing.
///
/// This is not a placeholder: a document with no markup language on top of it has no legacy
/// attributes, so the correct contribution is none at all.
pub struct NoPresentationalHints;

impl PresentationalHints for NoPresentationalHints {
    fn hints_for(
        &self,
        _element: Node<'_>,
        _visited: VisitedHandlingMode,
        _out: &mut dyn Push<ApplicableDeclarationBlock>,
    ) {
    }
}
