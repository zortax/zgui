//! The surface a menu opens, and everything that keeps it honest.

use core::cell::Cell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;
use zgui_ui_primitives::{Orientation, Placement, RovingContext};

use crate::menu::MenuTypeaheadProps;
use crate::menubar::arrows::MenubarArrowsProps;
use crate::menubar::style::MenubarStyle;
use crate::menubar::{MenubarMenuContext, SHEET};

/// The surface a [`MenubarMenu`](crate::MenubarMenu) opens.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Menubar {MenubarMenu(value = "file") {
///         MenubarTrigger {"File"}
///         MenubarContent {
///             MenubarItem {"New"}
///             MenubarSeparator()
///             MenubarItem(disabled = true) {"Print"}
///         }
///     }}
/// }
/// # }
/// ```
///
/// # What it is made of
///
/// Five behaviours from `zgui_ui_primitives`, none of them written here:
///
/// | Primitive | What it does for the menu |
/// |---|---|
/// | `Portal` | puts the surface on the overlay band, so nothing the bar sits inside can clip it |
/// | `Presence` | keeps it mounted until its exit animation has actually finished |
/// | `DismissableLayer` | closes it on <kbd>Escape</kbd>, or on a press that belongs to something else |
/// | `Popper` | places it under its name on the bar and keeps it on the screen |
/// | `RovingFocus` | one tab stop, and <kbd>↑</kbd> and <kbd>↓</kbd> between the items |
///
/// On top of those, the surface takes the keyboard to its first item as it appears — a menu that
/// left the caret on the name is a menu the arrow keys never reach — and answers a typed letter by
/// moving to the next item that reads as beginning with it, which is
/// [`MenuTypeahead`](crate::MenuTypeahead), the same one a dropdown menu is built on.
#[component]
pub fn MenubarContent(
    /// Where the surface is asked to go, before it is kept on screen.
    #[prop(default = Placement::BOTTOM)]
    placement: Placement,
    /// Classes merged after the surface's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);
    // The overlay is built inside three nested closures that may each run again, so what the
    // caller handed in is stored rather than captured: a value moved into the first of them is a
    // value the second one cannot have.
    let class = StoredValue::new_local(class);
    let attrs = StoredValue::new_local(attrs);
    let children = StoredValue::new_local(children);
    let menu = MenubarMenuContext::current();
    // The bar's own roving-focus group, read here — outside the menu's — because this is the last
    // scope that is still inside the bar. The surface below is portalled onto an overlay band and
    // publishes a group of its own, so from in there the bar is unreachable in both directions.
    let bar = RovingContext::current();
    let surface = menu
        .as_ref()
        .map_or_else(NodeRef::new, MenubarMenuContext::content);
    let anchor = menu
        .as_ref()
        .map_or_else(NodeRef::new, MenubarMenuContext::trigger);
    let open = {
        let menu = menu.clone();
        Signal::derive_local(move || menu.as_ref().is_some_and(MenubarMenuContext::is_open))
    };
    // Escape and an outside press both land here, and both do the same two things: close the menu
    // and put the focus back where it came from.
    let dismiss = UnsyncCallback::new(move |_: DismissReason| {
        if let Some(menu) = &menu {
            menu.dismiss();
        }
    });

    view! {
        Portal {
            Presence(present = open, surface = surface) {
                DismissableLayer(on_dismiss = dismiss) {
                    Popper(anchor = anchor, placement = placement) {
                        MenubarSurface(
                            element_ref = surface,
                            bar = bar,
                            class = class.get_value(),
                            {..attrs.get_value()}
                        ) {
                            {children.get_value().view()}
                        }
                    }
                }
            }
        }
    }
}

/// The element the items actually sit on.
///
/// Its own component, because what it binds — the presence state a style sheet animates on — is
/// published to the scopes *inside* the presence, and a component's body runs outside the view it
/// returns.
#[component]
fn MenubarSurface(
    /// Where to record the surface's element.
    element_ref: NodeRef,
    /// The bar's own roving-focus group, for the arrows that move along it.
    bar: Option<RovingContext>,
    /// Classes merged after the surface's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    let presence = use_presence();
    let menu = MenubarMenuContext::current();
    let semantics = A11yBinding::new(Role::Menu).orientation(zgui::vocab::Orientation::Vertical);
    let semantics = match &menu {
        Some(menu) => semantics.popup_for(menu.trigger()),
        None => semantics,
    };

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menubar__content"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(
                presence
                    .map_or("open", |presence| presence.state_name())
                    .to_owned(),
            )
        })
        .a11y_from(semantics);

    view! {
        RovingFocus(
            orientation = Orientation::Vertical,
            element_ref = element_ref,
            class = class,
            {..own},
            {..attrs}
        ) {
            MenuTypeahead {
                MenubarOpeningFocus()
                MenubarArrows(bar = bar) {{children.into_view_once()}}
            }
        }
    }
}

/// Puts the keyboard on the menu's first item as the menu appears.
///
/// A menu opened from the keyboard that leaves the focus on the name is a menu that cannot be
/// walked: the next arrow key belongs to the bar, and the items below are unreachable without the
/// mouse. Written as a component with no element of its own because it has to run *inside* the
/// roving-focus group — that is the only scope the group of items is published to — and because it
/// has to wait for the items to register, which happens after this component is built.
#[component]
fn MenubarOpeningFocus() -> impl IntoView {
    let group = RovingContext::current();
    // Once, per opening. The menu's surface is built afresh each time it opens, so this scope is
    // too, and a flag here is a flag with the same lifetime as the menu it belongs to.
    let done = Rc::new(Cell::new(false));
    let watching = RenderEffect::new(move |_| {
        if done.get() {
            return;
        }
        let Some(group) = group else {
            return;
        };
        // Read tracked: the items register as they are built, which is after this effect first
        // runs, and this subscription is what brings it back when they have.
        if group.collection().items().is_empty() {
            return;
        }
        done.set(true);
        group.go_to_end(false);
    });
    on_cleanup_local(move || drop(watching));
}
