//! The panel one section opens, which floats above the page rather than inside its section.

use zgui::prelude::*;
use zgui::reactive::{StoredValue, UnsyncCallback};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::navigation_menu::SHEET;
use crate::navigation_menu::item::NavigationMenuItemContext;
use crate::navigation_menu::style::NavigationMenuStyle;
use crate::overlay::{ElevatedProps, SurfaceElevation};

/// The panel one section opens.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     NavigationMenu {NavigationMenuList {NavigationMenuItem(value = "products") {
///         NavigationMenuTrigger {"Products"}
///         NavigationMenuContent {
///             NavigationMenuLink {"Editor"}
///             NavigationMenuLink {"Runtime"}
///         }
///     }}}
/// }
/// # }
/// ```
///
/// The panel's own element is always there, because it is what the trigger's `controls` relation
/// names; what comes and goes is the content inside it. That is the same bargain
/// [`TabsContent`](crate::TabsContent) makes, and for the same reason.
///
/// An element that is always there is an element that would otherwise always be *placed*, and a
/// placement is made from measurements the frame delivers. So the positioner is told whether the
/// panel is showing, and while it is not it watches nothing and computes no position — which is
/// the difference between a closed menu that costs a frame nothing and one that re-places itself
/// every time the page moves under it.
///
/// # Why it is portalled
///
/// A panel positioned inside its own section belongs to that section, and a navigation bar is
/// written inside whatever else the page has — a card, a toolbar, a header, with a sidebar under
/// it. So the panel is cut off at the first ancestor that clips and covered by whatever is painted
/// after it, and a menu of three links shows one. Every other floating surface in this library
/// escapes that by being portalled, and so does this one: it goes on an overlay band and is placed
/// against its own trigger from there. It rises with
/// [`SurfaceElevation`](crate::overlay::SurfaceElevation) too, so a navigation menu inside a dialog
/// opens over the dialog rather than under it.
#[component]
pub fn NavigationMenuContent(
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The links.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, NavigationMenuStyle::CSS);
    let section = NavigationMenuItemContext::current();
    let node = section
        .as_ref()
        .map_or_else(NodeRef::new, NavigationMenuItemContext::content);
    let open = {
        let section = section.clone();
        move || {
            section
                .as_ref()
                .is_some_and(NavigationMenuItemContext::is_open)
        }
    };

    let mut semantics = A11yBinding::new(Role::Group).hidden({
        let open = open.clone();
        move || !open()
    });
    if let Some(section) = &section {
        semantics = semantics.labelled_by(section.trigger());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let open = open.clone();
            move || Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .a11y_from(semantics);

    // The panel is portalled, so it is nowhere near the section in the tree and a press inside it
    // never reaches the section's own listener. Escape on a link in the open panel is the ordinary
    // way out of it, so the panel answers it where it is.
    let dismissing = StoredValue::new_local(section.clone());
    let on_key = move |ev: &mut EventCx<'_, events::KeyDown>| {
        if matches!(ev.key, Key::Named(NamedKey::Escape))
            && let Some(section) = dismissing.get_value()
            && section.is_open()
        {
            section.dismiss();
            ev.prevent_default();
            ev.stop_propagation();
        }
    };

    let at = SurfaceElevation::raise(OverlayLayer::Popover);
    at.publish();
    let anchor = section
        .as_ref()
        .map_or_else(NodeRef::new, NavigationMenuItemContext::trigger);

    // A press past the panel closes it, and Escape closes it from anywhere — the same layer every
    // floating surface here dismisses through. Only Escape sends the focus back to the trigger:
    // it ends a keyboard detour, while an outside press has already put the focus on the thing it
    // landed on.
    let dismiss = {
        let section = section.clone();
        UnsyncCallback::new(move |reason: DismissReason| {
            let Some(section) = &section else { return };
            if !section.is_open() {
                return;
            }
            match reason {
                DismissReason::EscapeKey => section.dismiss(),
                _ => section.close(),
            }
        })
    };

    // A portal's content is built again every time it is rebuilt, so nothing the panel is made of
    // may be moved out of the closure on the first build.
    let showing = Signal::derive_local(open);
    let own = StoredValue::new_local(own);
    let attrs = StoredValue::new_local(attrs);
    let class = StoredValue::new_local(class);
    let children = StoredValue::new_local(children);

    view! {
        Portal(layer = {at.layer()}) {
            Elevated(at = at) {
                Popper(
                    anchor = anchor,
                    placement = {Placement::new(Side::Bottom, Align::Start)},
                    offset = 6.0,
                    active = showing,
                    class = "zui-navigation-menu__positioner"
                ) {
                    box(
                        class = "zui-navigation-menu__content",
                        node_ref = node,
                        on:key_down = on_key,
                        {..own.get_value()},
                        {..attrs.get_value()},
                        class = {class.get_value()}
                    ) {
                        if move || showing.get() {
                            // The layer lives exactly as long as the panel is open, because the
                            // panel's own element never unmounts: a permanently registered layer
                            // would sit on the dismissal stack shut, above whichever section
                            // actually opened, and claim the topmost spot from it. It wraps the
                            // panel's *inside* — padding and all, see the style sheet — with the
                            // trigger excluded, so the only presses that dismiss are the ones
                            // that are genuinely past the menu.
                            DismissableLayer(
                                layer = {OverlayLayer::Modal},
                                class = "zui-navigation-menu__dismiss",
                                on_dismiss = dismiss,
                                exclude = anchor
                            ) {
                                {children.get_value().view()}
                            }
                        }
                    }
                }
            }
        }
    }
}
