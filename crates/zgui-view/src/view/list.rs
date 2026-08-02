//! A list of views with no keys.

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::view::View;

/// What an unkeyed list retains.
///
/// Rebuilding walks position by position: the shared prefix is rebuilt in place, anything extra is
/// built and mounted, anything left over is unmounted. That is the right behaviour for a list
/// whose items are identified by *where they are* — a set of columns, a fixed row of controls.
///
/// It is the wrong behaviour for a collection whose items have identities, because inserting at
/// the front rebuilds every item. Reach for [`For`](crate::For) there, which is keyed and moves
/// nodes rather than rewriting them.
pub struct ListState<S> {
    /// The position marker every item is inserted before.
    marker: NodeId,
    /// The parent, when mounted.
    parent: Option<NodeId>,
    /// The items, in order.
    items: Vec<S>,
}

impl<S: Anchor> Anchor for ListState<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.marker, before);
        self.parent = Some(parent);
        for item in &mut self.items {
            item.mount(dom, parent, Some(self.marker));
        }
    }

    fn unmount(&mut self, dom: &DomHandle) {
        for item in &mut self.items {
            item.unmount(dom);
        }
        dom.detach(self.marker);
        self.parent = None;
    }

    fn first_node(&self) -> Option<NodeId> {
        self.items
            .iter()
            .find_map(Anchor::first_node)
            .or(Some(self.marker))
    }
}

impl<V: View> View for Vec<V> {
    type State = ListState<V::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let marker = cx.dom().create_marker();
        let items = self.into_iter().map(|view| view.build(cx)).collect();
        ListState {
            marker,
            parent: None,
            items,
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let wanted = self.len();
        let mut views = self.into_iter();

        for existing in state.items.iter_mut().take(wanted) {
            let view = views
                .next()
                .expect("the prefix is no longer than the new list");
            view.rebuild(existing, cx);
        }

        for view in views {
            let mut built = view.build(cx);
            if let Some(parent) = state.parent {
                built.mount(cx.dom(), parent, Some(state.marker));
            }
            state.items.push(built);
        }

        for mut surplus in state.items.drain(wanted.min(state.items.len())..) {
            surplus.unmount(cx.dom());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn a_longer_list_appends_and_a_shorter_one_drops_the_surplus() {
        let f = Fixture::new();
        let mut state = vec!["a", "b"].build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "ab");

        vec!["a", "b", "c"].rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "abc");

        vec!["x"].rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "x");

        state.unmount(&f.dom);
        assert_eq!(f.text(), "");
        f.window.unmount();
    }
}
