//! A menu that drops out of a button.

mod content;
mod trigger;

pub use crate::dropdown_menu::content::{DropdownMenuContent, DropdownMenuContentProps};
pub use crate::dropdown_menu::trigger::{DropdownMenuTrigger, DropdownMenuTriggerProps};

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::menu::MenuContext;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// A menu opened by a button, anchored under it.
///
/// Everything about how it behaves is [`menu`](crate::menu): the items, the keyboard, the
/// typeahead, the submenus. What this adds is the one thing that makes it a *dropdown* — a button
/// that opens it, and the surface anchored under that button.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// What can be done to the account.
/// #[component]
/// fn AccountMenu() -> impl IntoView {
///     view! {
///         DropdownMenu {
///             DropdownMenuTrigger(variant = ButtonVariant::Outline) {"Account"}
///             DropdownMenuContent {
///                 MenuLabel {"Signed in as ada"}
///                 MenuSeparator()
///                 MenuItem(shortcut = "⌘,") {"Settings"}
///                 MenuItem {"Billing"}
///                 MenuSeparator()
///                 MenuItem(destructive = true) {"Sign out"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Enter</kbd> or <kbd>Space</kbd> on the trigger opens it and moves the caret into it;
/// <kbd>↑</kbd> and <kbd>↓</kbd> walk the items; typing a letter jumps; <kbd>Escape</kbd> closes it
/// and returns the caret to the trigger.
#[component]
pub fn DropdownMenu(
    /// Whether it is open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open, when it owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    let state = OverlayState::new(open, default_open, on_open_change).provide();
    // This is the surface a chosen item closes, however many submenus deep it was chosen from.
    MenuContext::new(state).provide();
    view! { {children.into_view_once()} }
}
