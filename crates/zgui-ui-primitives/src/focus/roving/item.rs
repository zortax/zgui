//! One item of a roving-focus group, from the item's side.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;

use crate::collection::ItemId;
use crate::focus::roving::RovingContext;

/// One item's place in its [`RovingFocus`](crate::RovingFocus) group.
///
/// `Copy`, so it can be stored in as many bindings as the item has.
#[derive(Copy, Clone)]
pub struct RovingItem {
    /// What this item is called in its group.
    id: ItemId,
    /// The group.
    context: RovingContext,
}

impl RovingItem {
    /// What to bind this item's `tabindex` to.
    ///
    /// Exactly one item in a group is sequentially focusable at a time, which is what makes the
    /// whole group one thing to tab past. Until any item has been chosen, the *first* one in tree
    /// order is the tab stop — so a group is reachable from the keyboard from the moment it has an
    /// item in it, rather than only after something has been focused.
    pub fn tabindex(&self) -> Signal<Focus, LocalStorage> {
        let item = *self;
        Signal::derive_local(move || {
            if item.is_active() {
                Focus::Sequential
            } else {
                Focus::Programmatic
            }
        })
    }

    /// Whether this item is the group's tab stop.
    pub fn is_active(&self) -> bool {
        match self.context.active() {
            Some(active) => active == self.id,
            None => {
                // Read for the subscription, so an item that becomes the first one — or becomes
                // reachable — rebinds its own `tabindex` rather than waiting for something else.
                let _ = self.context.collection().items();
                self.context
                    .collection()
                    .end(false)
                    .is_some_and(|first| first.id() == self.id)
            }
        }
    }

    /// Makes this item the group's tab stop.
    ///
    /// What an item calls when it is focused or pressed, so that tabbing away and back returns to
    /// the item the user was last on rather than to the first one.
    pub fn activate(&self) {
        self.context.set_active(self.id);
    }

    /// What this item is called in its group.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// The group this item belongs to.
    pub fn group(&self) -> RovingContext {
        self.context
    }
}

/// Registers `node` as an item of the enclosing [`RovingFocus`](crate::RovingFocus) group.
///
/// `None` outside one, and that is an ordinary answer rather than a mistake: the same component is
/// usually usable on its own, where it is an ordinary tab stop and there is no group to belong to.
/// A caller falls back to [`Focus::Sequential`] and everything else about the component is
/// unchanged.
///
/// The registration is undone when the calling scope goes away, so an item that unmounts leaves
/// the group without anything having to be told.
pub fn use_roving_item(node: NodeRef) -> Option<RovingItem> {
    use_roving_item_when(node, Signal::stored_local(true))
}

/// The same, for an item the arrow keys should pass over while `reachable` reads false.
///
/// What a disabled menu item or tab needs, and the difference between *disabled* and *absent*: the
/// item keeps its place and a reader still meets it, but one arrow key is one usable move rather
/// than one that strands the keyboard on something that refuses to be chosen.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::reactive::LocalStorage;
/// use zgui::{component, view};
/// use zgui_ui_primitives::use_roving_item_when;
///
/// /// One row of a group, which the arrows skip while it cannot be chosen.
/// #[component]
/// fn Row(
///     /// Whether it can be chosen.
///     disabled: Signal<bool, LocalStorage>,
/// ) -> impl IntoView {
///     let node = NodeRef::new();
///     let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));
///     view! {
///         control(
///             node_ref = node,
///             tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get())
///         ) {
///             "Row"
///         }
///     }
/// }
/// ```
pub fn use_roving_item_when(
    node: NodeRef,
    reachable: Signal<bool, LocalStorage>,
) -> Option<RovingItem> {
    let context = RovingContext::current()?;
    let id = context
        .collection()
        .register_reachable_when(node, reachable);
    Some(RovingItem { id, context })
}
