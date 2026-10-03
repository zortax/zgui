//! A hole whose content is replaced only when a value it watches actually changes.

use core::cell::RefCell;
use std::rc::Rc;

use zgui_reactive::{Owner, RenderEffect};

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, AnyViewState, Hole, View};

/// What a gated hole retains: a scope, an effect, and the place its content sits.
///
/// The difference from a plain reactive hole is the comparison. A reactive hole rebuilds its whole
/// content whenever anything its closure read changes; this one watches a separate, small value —
/// a branch selector — and touches the content only when *that* value changes. A conditional whose
/// test reads a list and asks whether it is empty therefore swaps its branch when the list becomes
/// empty, and does nothing at all on an ordinary insertion into it.
///
/// The difference is not only about work. Rebuilding a branch runs its cleanups, cancels and
/// restarts its timers, and rebinds every handle inside it, so a branch rebuilt on an unrelated
/// write is a branch that loses whatever it was keeping.
pub(super) struct Branch<K: 'static> {
    /// Where the content sits, shared with the effect that replaces it.
    hole: Rc<RefCell<Hole<AnyViewState>>>,
    /// The scope the content belongs to.
    owner: Owner,
    /// The scope of the branch that shows, a child of `owner`, replaced with the branch.
    shown: Rc<RefCell<Option<Owner>>>,
    /// The effect. Dropping it stops the hole updating.
    effect: Option<RenderEffect<K>>,
}

impl<K: PartialEq + 'static> Branch<K> {
    /// Builds a hole watching `select`, refilled by `content` whenever the selection changes.
    pub(super) fn new(
        select: impl FnMut() -> K + 'static,
        content: impl Fn(&K) -> AnyView + 'static,
        cx: &mut BuildCx<'_>,
    ) -> Self {
        let owner = cx.owner().child();
        let hole = Rc::new(RefCell::new(Hole::new(cx.dom())));
        let shown = Rc::new(RefCell::new(None));
        let effect = Self::watch(
            Rc::clone(&hole),
            &owner,
            Rc::clone(&shown),
            select,
            content,
            cx,
            None,
            false,
        );
        Self {
            hole,
            owner,
            shown,
            effect: Some(effect),
        }
    }

    /// Replaces the closures behind this hole, keeping the content that is already there.
    ///
    /// The new effect starts from the selection the old one last computed, so a rebuild whose
    /// selection did not change moves no node. It still rebuilds that content in place: a rebuild
    /// comes from the component around the hole, which cleaned its scope first, and everything the
    /// branch made went with it.
    pub(super) fn restart(
        &mut self,
        select: impl FnMut() -> K + 'static,
        content: impl Fn(&K) -> AnyView + 'static,
        cx: &mut BuildCx<'_>,
    ) {
        let previous = self.effect.as_ref().and_then(RenderEffect::take_value);
        self.effect = None;
        let owner = self.owner.clone();
        self.effect = Some(Self::watch(
            Rc::clone(&self.hole),
            &owner,
            Rc::clone(&self.shown),
            select,
            content,
            cx,
            previous,
            true,
        ));
    }

    /// The effect that keeps one hole in line with one selector.
    ///
    /// Each branch is built in a scope of its own, a child of `owner`, and that scope goes when the
    /// branch does. The effect's own scope is cleaned every time the selector runs again, so a
    /// branch built in it would lose what it made — the default of a prop, a stored value — on
    /// every write the selector reads, while its nodes stayed on the screen.
    fn watch(
        hole: Rc<RefCell<Hole<AnyViewState>>>,
        owner: &Owner,
        shown: Rc<RefCell<Option<Owner>>>,
        mut select: impl FnMut() -> K + 'static,
        content: impl Fn(&K) -> AnyView + 'static,
        cx: &mut BuildCx<'_>,
        initial: Option<K>,
        refresh: bool,
    ) -> RenderEffect<K> {
        let scoped = cx.to_owned_cx();
        let parent = owner.clone();
        let mut refresh = refresh;
        owner.with(|| {
            RenderEffect::new_with_value(
                move |last: Option<K>| {
                    let next = select();
                    if last.as_ref() != Some(&next) {
                        let scope = parent.child();
                        let within = scoped.with_owner(scope.clone());
                        let built = scope.with(|| content(&next).build(&mut within.cx()));
                        hole.borrow_mut().set(scoped.dom(), Some(built));
                        if let Some(gone) = shown.borrow_mut().replace(scope) {
                            gone.cleanup();
                        }
                    } else if core::mem::take(&mut refresh) {
                        let scope = shown
                            .borrow_mut()
                            .get_or_insert_with(|| parent.child())
                            .clone();
                        let within = scoped.with_owner(scope.clone());
                        let mut hole = hole.borrow_mut();
                        if let Some(existing) = hole.content_mut() {
                            scope.with_cleanup(|| {
                                content(&next).rebuild(existing, &mut within.cx());
                            });
                        }
                    }
                    next
                },
                initial,
            )
        })
    }
}

/// Drops the content together with the state, for the reason given on
/// [`ReactiveState`](crate::view::ReactiveState).
impl<K: 'static> Drop for Branch<K> {
    fn drop(&mut self) {
        if let Ok(mut hole) = self.hole.try_borrow_mut() {
            drop(hole.take());
        }
    }
}

impl<K: 'static> Anchor for Branch<K> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.hole.borrow_mut().mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.effect = None;
        self.hole.borrow_mut().unmount(dom);
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        self.hole.borrow().first_node()
    }
}
