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
        let effect = Self::watch(Rc::clone(&hole), &owner, select, content, cx, None);
        Self {
            hole,
            owner,
            effect: Some(effect),
        }
    }

    /// Replaces the closures behind this hole, keeping the content that is already there.
    ///
    /// The new effect starts from the selection the old one last computed, so a rebuild whose
    /// selection did not change moves no node and runs no cleanup.
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
            select,
            content,
            cx,
            previous,
        ));
    }

    /// The effect that keeps one hole in line with one selector.
    fn watch(
        hole: Rc<RefCell<Hole<AnyViewState>>>,
        owner: &Owner,
        mut select: impl FnMut() -> K + 'static,
        content: impl Fn(&K) -> AnyView + 'static,
        cx: &mut BuildCx<'_>,
        initial: Option<K>,
    ) -> RenderEffect<K> {
        let scoped = cx.to_owned_cx().with_owner(owner.clone());
        owner.with(|| {
            RenderEffect::new_with_value(
                move |last: Option<K>| {
                    let next = select();
                    if last.as_ref() != Some(&next) {
                        let built = content(&next).build(&mut scoped.cx());
                        hole.borrow_mut().set(scoped.dom(), Some(built));
                    }
                    next
                },
                initial,
            )
        })
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
