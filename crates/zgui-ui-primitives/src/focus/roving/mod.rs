//! One tab stop for a whole group, and the arrow keys inside it.

mod item;
mod keys;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::{component, view};

pub use crate::focus::roving::item::{RovingItem, use_roving_item, use_roving_item_when};
pub use crate::focus::roving::keys::Orientation;

use crate::collection::{Collection, ItemId};

/// What a [`RovingFocus`] group publishes to its items.
#[derive(Copy, Clone)]
pub struct RovingContext {
    /// The items, in tree order.
    collection: Collection,
    /// Which item is the group's one tab stop.
    active: RwSignal<Option<ItemId>, LocalStorage>,
    /// Which arrow keys move within the group.
    orientation: Orientation,
    /// Whether stepping past an end goes round.
    wrap: bool,
}

impl RovingContext {
    /// The group's items, in tree order.
    pub fn collection(&self) -> Collection {
        self.collection
    }

    /// Which item is the group's tab stop, when one has been decided.
    ///
    /// Nothing is active until the group has an item: the first item to register becomes the tab
    /// stop, so a group is tabbable from the moment it has anything in it.
    ///
    /// An item can outlive its group by a moment. A menu that closes disposes the group while its
    /// items are still coming down, and each one asks who holds the tab stop as it goes — so the
    /// answer for a group that is gone is "nobody" rather than a panic.
    pub fn active(&self) -> Option<ItemId> {
        self.active.try_get().flatten()
    }

    /// Makes `id` the group's tab stop.
    ///
    /// Does nothing once the group is gone. See [`RovingContext::active`].
    pub fn set_active(&self, id: ItemId) {
        if self.active.try_get_untracked().flatten() != Some(id) {
            self.active.try_set(Some(id));
        }
    }

    /// Which arrow keys move within this group.
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Which item the tab stop is on, counting the one that holds it by default.
    ///
    /// Until something has been focused the group's tab stop is its **first** item, so that the
    /// group is reachable from the keyboard from the moment it has anything in it. Stepping has to
    /// count from the same place — a step that started from "nothing" would answer the first arrow
    /// key with the item that already had the tab stop, and the group would appear to ignore it.
    fn effective_active(&self) -> Option<ItemId> {
        self.active
            .try_get_untracked()
            .flatten()
            .or_else(|| self.collection.end(false).map(|first| first.id()))
    }

    /// Moves the tab stop and the focus by `steps`, and reports whether anything moved.
    pub fn step(&self, steps: isize) -> bool {
        let from = self.effective_active();
        let Some(next) = self.collection.step(from, steps, self.wrap) else {
            return false;
        };
        self.active.try_set(Some(next.id()));
        next.focus();
        true
    }

    /// Moves the tab stop and the focus to the first or last item the keyboard may land on.
    pub fn go_to_end(&self, last: bool) -> bool {
        let Some(target) = self.collection.end(last) else {
            return false;
        };
        self.active.try_set(Some(target.id()));
        target.focus();
        true
    }

    /// The group this scope is inside, when it is inside one.
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }
}

/// Gives a group of controls one tab stop between them, moved by the arrow keys.
///
/// A toolbar of twelve buttons is one thing to tab past, not twelve. That is what a roving
/// tabindex is: exactly one item in the group is sequentially focusable at a time, every other one
/// is focusable only when something focuses it, and the arrow keys move which is which. It is the
/// interaction pattern behind toolbars, tab bars, menus, radio groups and listboxes, and every one
/// of those gets it wrong differently when each writes it itself.
///
/// The group renders its own element and listens for the arrow keys on it. Items register
/// themselves with [`use_roving_item`], which hands back the `tabindex` to bind and the call to
/// make when the item is focused or clicked.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui_primitives::prelude::*;
///
/// #[component]
/// fn ToolbarButton(children: Children) -> impl IntoView {
///     let node = NodeRef::new();
///     let item = use_roving_item(node);
///     let tabindex = item.map(|item| item.tabindex());
///     view! {
///         control(
///             node_ref = node,
///             tabindex = move || tabindex.map_or(Focus::Sequential, |index| index.get()),
///             on:focus_in = move |_| { if let Some(item) = item { item.activate() } }
///         ) {
///             {children.into_view_once()}
///         }
///     }
/// }
///
/// #[component]
/// fn Toolbar() -> impl IntoView {
///     view! {
///         RovingFocus(orientation = Orientation::Horizontal) {
///             box {
///                 ToolbarButton {"Bold"}
///                 ToolbarButton {"Italic"}
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn RovingFocus(
    /// Which arrow keys move within the group.
    #[prop(default = Orientation::Horizontal)]
    orientation: Orientation,
    /// Whether stepping past the last item goes back to the first.
    #[prop(default = true)]
    wrap: bool,
    /// Where to record the group's own element.
    #[prop(optional)]
    element_ref: Option<NodeRef>,
    /// Extra classes on the group's own element.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else to put on the group's own element.
    ///
    /// A roving group is always *something* — a radio group, a toolbar, a tab list — and what it
    /// is belongs on the element that carries the keys, not on a box inside it. Without this the
    /// role and the key handling would be two elements, and a reader would meet an anonymous
    /// container wrapping the thing it was looking for.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    let context = RovingContext {
        collection: Collection::provide(),
        active: RwSignal::new_local(None),
        orientation,
        wrap,
    };
    provide_local_context(context);

    let root = element_ref.unwrap_or_default();
    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let Some(action) = keys::action(&ev.key, orientation) else {
                return;
            };
            let moved = match action {
                keys::Action::Step(steps) => context.step(steps),
                keys::Action::End(last) => context.go_to_end(last),
            };
            if moved {
                // Only when something moved: an arrow key at the end of a group that does not wrap
                // belongs to whatever is outside the group, and swallowing it there is how a page
                // stops scrolling for no visible reason.
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    view! {
        box(
            class = class,
            node_ref = root,
            attr:data-orientation = orientation.name(),
            on:key_down = on_key_down,
            {..attrs}
        ) {
            {children.into_view_once()}
        }
    }
}
