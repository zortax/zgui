//! A menu that opens where the pointer asked for one.

mod content;
mod style;
mod trigger;

pub use crate::context_menu::content::{ContextMenuContent, ContextMenuContentProps};
pub use crate::context_menu::style::ContextMenuStyle;
pub use crate::context_menu::trigger::{ContextMenuTrigger, ContextMenuTriggerProps};

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::menu::MenuContext;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// What the context menu's rules are installed under.
pub(crate) const SHEET: &str = "zui-context-menu";

/// A menu opened by asking for one over a region, at the point it was asked for.
///
/// The items, the keyboard and the submenus are [`menu`](crate::menu)'s, exactly as in a
/// [`DropdownMenu`](crate::DropdownMenu). The one thing this has that a dropdown does not is a
/// *place*: the menu is anchored to where the pointer was, not to the region it came from, so a
/// long list is not thrown across the window because the region happens to be wide.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Somewhere to ask what can be done here.
/// #[component]
/// fn Canvas() -> impl IntoView {
///     view! {
///         ContextMenu {
///             ContextMenuTrigger {
///                 text {"Right-click, or press and hold."}
///             }
///             ContextMenuContent {
///                 MenuItem {"Paste"}
///                 MenuSeparator()
///                 MenuItem(destructive = true) {"Clear"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// Once it is open it is a menu like any other: the arrows walk it, typing jumps, <kbd>Escape</kbd>
/// closes it and gives the caret back. Opening it from the keyboard is the platform's business
/// rather than this component's, and a region whose actions are *only* reachable here is a region
/// a keyboard user cannot use — so put them somewhere else as well.
#[component]
pub fn ContextMenu(
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
    /// The region and the content.
    children: Children,
) -> impl IntoView {
    let state = OverlayState::new(open, default_open, on_open_change).provide();
    MenuContext::new(state).provide();
    view! { {children.into_view_once()} }
}
