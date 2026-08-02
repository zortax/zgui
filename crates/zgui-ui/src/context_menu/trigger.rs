//! The region a context menu is asked for over.

use zgui::geom::CssPx;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::vocab::{HasPopup, PointerButton};
use zgui::{component, view};

use crate::context_menu::SHEET;
use crate::context_menu::style::ContextMenuStyle;
use crate::overlay::OverlayState;

/// The region a [`ContextMenu`](crate::ContextMenu) is opened over.
///
/// It renders a wrapper and, inside it, a point of zero size that the menu is anchored to. The
/// point is what makes the menu appear *where the pointer asked*: anchoring to the region itself
/// would put a menu asked for in the middle of a wide table against that table's edge.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ContextMenu {
///         ContextMenuTrigger {text {"A row"}}
///         ContextMenuContent {MenuItem {"Copy"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ContextMenuTrigger(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the menu is about.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ContextMenuStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let area = NodeRef::new();
    // Where in the region the pointer asked, in CSS pixels from its top-left corner. Two numbers
    // rather than a point, because they are bound to two style properties.
    let left = RwSignal::new_local(0.0f32);
    let top = RwSignal::new_local(0.0f32);

    let ask = move |position: zgui::geom::Point<CssPx, zgui::geom::Css>| {
        // The region's box is in device pixels and a pointer position is in CSS pixels, so one of
        // them has to be converted — and getting that wrong is a menu that lands further from the
        // pointer the higher the display's scale is.
        //
        // The *window's* box and not the region's own, for the same reason: a pointer reports where
        // it is in the window, so subtracting where the region sits inside its parent leaves the
        // difference between those two spaces in the answer — which is a menu that opens further
        // from the pointer the deeper in the page the region is, and further still once anything
        // has been scrolled.
        let scale = area.scale();
        let origin = area
            .window_bounds()
            .map_or((0.0, 0.0), |box_| (box_.origin.x.0, box_.origin.y.0));
        left.set(position.x.0 - origin.0 / scale);
        top.set(position.y.0 - origin.1 / scale);
        state.open();
    };

    let on_context_menu = handler(
        events::CONTEXT_MENU,
        move |ev: &mut EventCx<'_, events::ContextMenu>| {
            ev.prevent_default();
            ask(ev.position);
        },
    );
    // The secondary button as well as the request event, because a request is what a long press
    // produces and a right-click on this platform does not: a region that only answered the
    // request would have no context menu on a mouse at all.
    let on_pointer_down = handler(
        events::POINTER_DOWN,
        move |ev: &mut EventCx<'_, events::PointerDown>| {
            if ev.button == Some(PointerButton::Secondary) {
                ev.prevent_default();
                ask(ev.position);
            }
        },
    );

    let own = state.trigger_attrs(HasPopup::Menu);

    view! {
        box(
            node_ref = area,
            class = {ContextMenuStyle::CLASS},
            class = "zui-context-menu__area",
            on:context_menu = on_context_menu,
            on:pointer_down = on_pointer_down,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            box(
                node_ref = {state.trigger()},
                class = "zui-context-menu__anchor",
                style:left = move || Some(format!("{}px", left.get())),
                style:top = move || Some(format!("{}px", top.get())),
                a11y:hidden = true
            )
        }
    }
}
