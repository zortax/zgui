//! The item a submenu opens out of.

use std::cell::Cell;
use std::rc::Rc;

use zgui::geom::{Css, CssPx, Device, DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::AttrName;
use zgui::vocab::{HasPopup, Key, NamedKey, UiState};
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_primitives::use_roving_item_when;

use crate::menu::SHEET;
use crate::menu::content::MenuSurface;
use crate::menu::item::defocus_on_leave;
use crate::menu::style::MenuStyle;
use crate::menu::sub::triangle::heading_toward;
use crate::menu::sub::{CLOSE_DELAY, OPEN_DELAY, SubIntent};
use crate::overlay::{Delayed, OverlayState};

/// The menu item a [`MenuSub`](crate::MenuSub) opens out of.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuSub {
///             MenuSubTrigger {"Export as"}
///             MenuSubContent {MenuItem {"PDF"}}
///         }
///     }}
/// }
/// # }
/// ```
///
/// # Why leaving it does not always close the submenu
///
/// The path from this item to the submenu crosses the items below it, because that is the shape of
/// the diagonal a hand actually draws. A pointer travelling along it is *on its way there*, and
/// closing on the way would make the submenu reachable only by a perfect right angle. See
/// [`heading_toward`].
#[component]
pub fn MenuSubTrigger(
    /// Whether the submenu can be opened.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether it is indented to the column the ticks and bullets leave for a label.
    #[prop(default = false)]
    inset: bool,
    /// Classes merged after the item's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let node = state.trigger();
    let item = use_roving_item_when(node, Signal::derive_local(move || !disabled.get()));
    let opening = Delayed::new();
    // The shared one when there is a `MenuSub` above to share it with, so the surface can cancel
    // a close the pointer's arrival has disproved. A private one otherwise, for a trigger used
    // bare.
    let closing = SubIntent::current().map_or_else(Delayed::new, |intent| intent.closing());
    // Where the pointer was while it was still over this item. It is the apex of the corridor the
    // submenu is reached through, and without it a leaving pointer has a position but no
    // direction — which is exactly what the decision needs.
    let last: Rc<Cell<Point<DevicePx, Device>>> =
        Rc::new(Cell::new(Point::new(DevicePx(0.0), DevicePx(0.0))));

    let track = Rc::clone(&last);
    let on_move = handler(
        events::POINTER_MOVE,
        move |ev: &mut EventCx<'_, events::PointerMove>| {
            track.set(in_device_pixels(ev.position, node.scale()));
        },
    );

    let leaving = Rc::clone(&last);
    let leave_closing = closing.clone();
    let leave_opening = opening.clone();
    let defocus = defocus_on_leave(node, MenuSurface::current().map(MenuSurface::node));
    let on_leave = handler(
        events::POINTER_LEAVE,
        move |ev: &mut EventCx<'_, events::PointerLeave>| {
            leave_opening.cancel();
            if !state.is_open_untracked() {
                // Shut, this is an ordinary item and the pointer takes the highlight with it.
                // Open, the focus stays: the row's `data-state` keeps it lit as the visible trail
                // to the surface beside it, and blurring it would erase the trail mid-crossing.
                defocus(ev);
                return;
            }
            let now = in_device_pixels(ev.position, node.scale());
            // The corridor, not the box: leaving the item is not the question, and closing on it
            // would make the submenu reachable only by a perfect right angle. The surface's box is
            // asked for in the *window's* space, because that is the space the pointer answers in;
            // its parent-relative bounds would draw the corridor somewhere the pointer never goes,
            // and the submenu would close on the very path that leads to it.
            if let Some(surface) = state.content().window_bounds()
                && heading_toward(leaving.get(), now, surface)
            {
                return;
            }
            leave_closing.after(CLOSE_DELAY, move || state.close());
        },
    );

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let opens = matches!(
                ev.key,
                Key::Named(NamedKey::ArrowRight | NamedKey::Enter | NamedKey::Space)
            );
            if opens && !disabled.get_untracked() {
                ev.prevent_default();
                ev.stop_propagation();
                state.open();
            }
        },
    );

    let enter_opening = opening.clone();
    let enter_closing = closing.clone();
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menu__item"), true)
        .class_toggle(zgui::view::ClassName::new("zui-menu__item--inset"), inset)
        .attribute(AttrName::new("data-state"), move || {
            Some(state.state_name().to_owned())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::MenuItem)
                .has_popup(HasPopup::Menu)
                .expanded(move || state.is_open())
                .controls(state.content())
                .disabled(move || disabled.get()),
        );

    view! {
        control(
            node_ref = node,
            tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
            on:pointer_enter = move |_| {
                if disabled.get_untracked() {
                    return;
                }
                enter_closing.cancel();
                if let Some(item) = item {
                    item.activate();
                }
                node.focus();
                enter_opening.after(OPEN_DELAY, move || state.open());
            },
            on:pointer_move = on_move,
            on:pointer_leave = on_leave,
            on:key_down = on_key_down,
            on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            Icon(icon = CHEVRON_RIGHT, class = "zui-menu__chevron")
        }
    }
}

/// A pointer position, which arrives in CSS pixels, in the space a border box is measured in.
fn in_device_pixels(position: Point<CssPx, Css>, scale: f32) -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(position.x.0 * scale),
        DevicePx(position.y.0 * scale),
    )
}
