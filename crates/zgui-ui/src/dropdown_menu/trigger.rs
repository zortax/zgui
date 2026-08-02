//! The button a dropdown menu drops out of.

use zgui::prelude::*;
use zgui::vocab::{HasPopup, Key, NamedKey};
use zgui::{component, view};

use crate::button::{ButtonProps, ButtonSize, ButtonVariant};
use crate::overlay::OverlayState;

/// The button that opens the enclosing [`DropdownMenu`](crate::DropdownMenu).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {
///         DropdownMenuTrigger(variant = ButtonVariant::Ghost) {"…"}
///         DropdownMenuContent {MenuItem {"Open"}}
///     }
/// }
/// # }
/// ```
///
/// # Keyboard
///
/// <kbd>Enter</kbd> and <kbd>Space</kbd> open the menu, because it is a button and that is what a
/// button does. <kbd>↓</kbd> and <kbd>↑</kbd> open it too, and they are the reason this handles
/// keys at all: reaching for the arrow key is what everyone does to a control that says it drops
/// something down, and a menu button that ignored them would answer only half the gestures a
/// keyboard user brings to it.
#[component]
pub fn DropdownMenuTrigger(
    /// How it looks.
    #[prop(default = ButtonVariant::Default)]
    variant: ButtonVariant,
    /// How big it is.
    #[prop(default = ButtonSize::Md)]
    size: ButtonSize,
    /// Classes merged after the button's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label.
    children: Children,
) -> impl IntoView {
    let state = OverlayState::current();
    let own = state.map_or_else(Attrs::new, |state| state.trigger_attrs(HasPopup::Menu));
    let node = state.map_or_else(NodeRef::new, |state| state.trigger());

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let opens = matches!(ev.key, Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp));
            if let Some(state) = state
                && opens
                && !state.is_open_untracked()
            {
                state.open();
                // Only the keys this claims. Everything else — Tab above all — belongs to whatever
                // is around the button.
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    view! {
        Button(
            node_ref = node,
            variant = variant,
            size = size,
            on:key_down = on_key_down,
            on:click = move |_| {
                if let Some(state) = state {
                    state.toggle();
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
