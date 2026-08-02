//! A view that may not be there.

use crate::cx::BuildCx;
use crate::view::anchor::Anchor;
use crate::view::hole::Hole;
use crate::view::view::View;

/// What an optional view retains: a place, and whatever is currently in it.
pub struct OptionState<S>(Hole<S>);

impl<S: Anchor> Anchor for OptionState<S> {
    fn mount(
        &mut self,
        dom: &crate::dom::DomHandle,
        parent: crate::id::NodeId,
        before: Option<crate::id::NodeId>,
    ) {
        self.0.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &crate::dom::DomHandle) {
        self.0.unmount(dom);
    }

    fn first_node(&self) -> Option<crate::id::NodeId> {
        self.0.first_node()
    }
}

impl<V: View> View for Option<V> {
    type State = OptionState<V::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let mut hole = Hole::new(cx.dom());
        if let Some(view) = self {
            let state = view.build(cx);
            hole.fill(cx.dom(), state);
        }
        OptionState(hole)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        match (self, state.0.content_mut()) {
            (Some(view), Some(existing)) => view.rebuild(existing, cx),
            (Some(view), None) => {
                let built = view.build(cx);
                state.0.set(cx.dom(), Some(built));
            }
            (None, Some(_)) => state.0.set(cx.dom(), None),
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn none_renders_nothing_and_some_fills_the_same_place() {
        let f = Fixture::new();
        let mut state = None::<&'static str>.build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        f.dom.insert(f.root, f.dom.create_text("|end"), None);
        assert_eq!(f.text(), "|end");

        Some("hello").rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "hello|end");

        None::<&'static str>.rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "|end");

        state.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn a_present_value_is_rebuilt_in_place_rather_than_replaced() {
        let f = Fixture::new();
        let mut state = Some("a").build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        Some("b").rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "b");
        assert_eq!(state.first_node(), node, "the text node was reused");

        state.unmount(&f.dom);
        f.window.unmount();
    }
}
