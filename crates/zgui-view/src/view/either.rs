//! One of two views, chosen at build time and swappable afterwards.

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::hole::Hole;
use crate::view::view::View;

/// One of two views.
///
/// What a conditional produces: the two branches have different types, and this is the type that
/// holds either of them. Swapping branches unmounts one and mounts the other in the same place;
/// staying on a branch rebuilds it, so a conditional whose condition did not change does no more
/// backend work than the branch itself did.
///
/// ```
/// use zgui_view::view::Either;
///
/// let shown: Either<&str, &str> = Either::Left("open");
/// assert!(matches!(shown, Either::Left(_)));
/// ```
pub enum Either<L, R> {
    /// The first branch.
    Left(L),
    /// The second branch.
    Right(R),
}

/// What an [`Either`] retains.
pub enum EitherState<L, R> {
    /// The first branch is showing.
    Left(Hole<L>),
    /// The second branch is showing.
    Right(Hole<R>),
}

impl<L: Anchor, R: Anchor> EitherState<L, R> {
    /// The hole, whichever branch is showing.
    fn hole_mut(&mut self) -> &mut dyn Anchor {
        match self {
            Self::Left(hole) => hole,
            Self::Right(hole) => hole,
        }
    }
}

impl<L: Anchor, R: Anchor> Anchor for EitherState<L, R> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.hole_mut().mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.hole_mut().unmount(dom);
    }

    fn first_node(&self) -> Option<NodeId> {
        match self {
            Self::Left(hole) => hole.first_node(),
            Self::Right(hole) => hole.first_node(),
        }
    }
}

impl<L: View, R: View> View for Either<L, R> {
    type State = EitherState<L::State, R::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        match self {
            Self::Left(view) => {
                let mut hole = Hole::new(cx.dom());
                let built = view.build(cx);
                hole.fill(cx.dom(), built);
                EitherState::Left(hole)
            }
            Self::Right(view) => {
                let mut hole = Hole::new(cx.dom());
                let built = view.build(cx);
                hole.fill(cx.dom(), built);
                EitherState::Right(hole)
            }
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        match (self, &mut *state) {
            (Self::Left(view), EitherState::Left(hole)) => match hole.content_mut() {
                Some(existing) => view.rebuild(existing, cx),
                None => {
                    let built = view.build(cx);
                    hole.set(cx.dom(), Some(built));
                }
            },
            (Self::Right(view), EitherState::Right(hole)) => match hole.content_mut() {
                Some(existing) => view.rebuild(existing, cx),
                None => {
                    let built = view.build(cx);
                    hole.set(cx.dom(), Some(built));
                }
            },
            (view, _) => {
                // The branch changed. The new one is mounted in front of the old one's marker
                // *before* the old one is taken out, because a detached marker is no longer a
                // position anything can be inserted before.
                let (parent, before) = position(state);
                let mut replacement = view.build(cx);
                if let Some(parent) = parent {
                    replacement.mount(cx.dom(), parent, before);
                }
                state.unmount(cx.dom());
                *state = replacement;
            }
        }
    }
}

/// Where a state currently sits, so its replacement can go in the same place.
fn position<L: Anchor, R: Anchor>(state: &EitherState<L, R>) -> (Option<NodeId>, Option<NodeId>) {
    match state {
        EitherState::Left(hole) => (hole.parent(), Some(hole.marker())),
        EitherState::Right(hole) => (hole.parent(), Some(hole.marker())),
    }
}

#[cfg(test)]
mod tests {
    use super::Either;
    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn swapping_branches_keeps_the_place_in_the_sibling_order() {
        let f = Fixture::new();
        let before = f.dom.create_text("[");
        let after = f.dom.create_text("]");
        f.dom.insert(f.root, before, None);
        f.dom.insert(f.root, after, None);

        let view: Either<&'static str, u32> = Either::Left("left");
        let mut state = view.build(&mut f.cx());
        state.mount(&f.dom, f.root, Some(after));
        assert_eq!(f.text(), "[left]");

        Either::<&'static str, u32>::Right(7u32).rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "[7]");

        Either::<&'static str, u32>::Left("back").rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "[back]");

        state.unmount(&f.dom);
        assert_eq!(f.text(), "[]");
        f.window.unmount();
    }

    #[test]
    fn staying_on_a_branch_rebuilds_it_in_place() {
        let f = Fixture::new();
        let view: Either<&'static str, u32> = Either::Left("a");
        let mut state = view.build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        Either::<&'static str, u32>::Left("b").rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "b");
        assert_eq!(state.first_node(), node);
        f.window.unmount();
    }
}
