//! What a view is.

use crate::cx::BuildCx;
use crate::view::anchor::Anchor;

/// A value that can be built into, and updated in, a backend node tree.
///
/// Building a view creates its nodes immediately and returns the retained state needed to update
/// them in place. Rebuilding never re-creates a node that did not change, which is what makes a
/// frame's backend traffic proportional to what changed rather than to the size of the tree.
///
/// Implementations of this trait ship for strings, numbers, `bool`, `char`, `Option`, `Result`,
/// tuples, `Vec`, and any closure returning a view — and a component author normally writes none
/// of them, because a component returns whichever of those its body produced.
///
/// ```
/// use zgui_reactive::{Mounted, install};
/// use zgui_interned::ElementName;
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, View};
/// use std::rc::Rc;
///
/// install().unwrap();
/// let backend = Rc::new(StubDom::new(DocumentId::FIRST));
/// let dom = DomHandle::from_rc(backend.clone());
/// let window = Mounted::new();
/// let cx = BuildCxOwned::new(
///     dom.clone(),
///     HostHandle::new(StubHost::default()),
///     window.owner().clone(),
///     DocumentId::FIRST,
/// );
/// let root = dom.create_element(ElementName::new("row"));
///
/// // A fragment is a tuple.
/// let mut state = ("a", "b").build(&mut cx.cx());
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "ab");
///
/// // Rebuilding writes only what changed.
/// ("a", "c").rebuild(&mut state, &mut cx.cx());
/// assert_eq!(backend.text_content(root), "ac");
/// ```
pub trait View: Sized + 'static {
    /// Retained state for this view between rebuilds.
    type State: Anchor;

    /// Creates this view's nodes, detached, and returns the state that updates them.
    fn build(self, cx: &mut BuildCx<'_>) -> Self::State;

    /// Updates the nodes `state` holds to say what this view says.
    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>);
}

/// Conversion into a [`View`].
///
/// This is the bound a component returns: `-> impl IntoView`. Everything that is already a view
/// converts into itself, so the bound costs nothing and names one thing rather than two.
pub trait IntoView: Sized {
    /// What this becomes.
    type Output: View;

    /// Converts.
    fn into_view(self) -> Self::Output;
}

impl<V: View> IntoView for V {
    type Output = V;

    fn into_view(self) -> V {
        self
    }
}
