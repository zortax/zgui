//! Choosing one of a list of things, from a control that shows the choice.

mod content;
mod item;
mod style;
mod trigger;

pub use crate::select::content::{SelectContent, SelectContentProps};
pub use crate::select::item::{SelectItem, SelectItemProps};
pub use crate::select::style::SelectStyle;
pub use crate::select::trigger::{
    SelectTrigger, SelectTriggerProps, SelectTriggerSize, SelectTriggerVariants, SelectValue,
    SelectValueProps,
};

/// A run of options that belong together, announced as a group.
pub use crate::menu::MenuGroup as SelectGroup;
/// The props of [`SelectGroup`].
pub use crate::menu::MenuGroupProps as SelectGroupProps;
/// A heading over a run of options. A [`MenuLabel`](crate::MenuLabel), because it is one.
pub use crate::menu::MenuLabel as SelectLabel;
/// The props of [`SelectLabel`].
pub use crate::menu::MenuLabelProps as SelectLabelProps;
/// A rule between two runs of options.
pub use crate::menu::MenuSeparator as SelectSeparator;
/// The props of [`SelectSeparator`].
pub use crate::menu::MenuSeparatorProps as SelectSeparatorProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::listbox::Listbox;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// What the select's rules are installed under.
pub(crate) const SHEET: &str = "zui-select";

/// One choice out of a list, from a control that shows which one it is.
///
/// The keyboard never leaves the trigger. The arrow keys walk the list *while the caret stays
/// where it is*, and the option being walked is named to a reader through `active_descendant` —
/// which is what an accessible listbox is, and why a select cannot be a menu with a different
/// style sheet.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which currency an invoice is in.
/// #[component]
/// fn Currency() -> impl IntoView {
///     let currency = RwSignal::new_local("gbp".to_owned());
///     view! {
///         Select(value = currency) {
///             SelectTrigger {SelectValue(placeholder = "Choose one")}
///             SelectContent {
///                 SelectItem(value = "gbp") {"Pound sterling"}
///                 SelectItem(value = "eur") {"Euro"}
///                 SelectItem(value = "usd", disabled = true) {"US dollar"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>↓</kbd> and <kbd>↑</kbd> open the list and walk it; <kbd>Home</kbd> and <kbd>End</kbd>
/// jump; the page keys move by ten; <kbd>Enter</kbd> chooses; <kbd>Escape</kbd> closes without
/// choosing. Opening lands on whatever is already chosen rather than at the top, so one press of
/// the down arrow moves on rather than back.
///
/// <kbd>Tab</kbd> is left alone, and that is not an omission: a select in a form that swallowed
/// Tab would be a form nobody can get out of.
#[component]
pub fn Select(
    /// Which value is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which value starts chosen, when the select owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told whenever the choice changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// Whether the list is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts showing.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever the list opens or closes.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// The trigger and the list.
    children: Children,
) -> impl IntoView {
    let surface = OverlayState::new(open, default_open, on_open_change).provide();
    Listbox::new(surface, value, default_value, on_change).provide();
    view! { {children.into_view_once()} }
}
