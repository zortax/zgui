//! Typing to narrow a list, and choosing one of what is left.

mod content;
mod input;
mod item;
mod style;

pub use crate::combobox::content::{
    ComboboxContent, ComboboxContentProps, ComboboxEmpty, ComboboxEmptyProps,
};
pub use crate::combobox::input::{ComboboxInput, ComboboxInputProps};
pub use crate::combobox::item::{ComboboxItem, ComboboxItemProps};
pub use crate::combobox::style::ComboboxStyle;

/// A run of options that belong together, announced as a group.
pub use crate::menu::MenuGroup as ComboboxGroup;
/// The props of [`ComboboxGroup`].
pub use crate::menu::MenuGroupProps as ComboboxGroupProps;
/// A heading over a run of options. A [`MenuLabel`](crate::MenuLabel), because it is one.
pub use crate::menu::MenuLabel as ComboboxLabel;
/// The props of [`ComboboxLabel`].
pub use crate::menu::MenuLabelProps as ComboboxLabelProps;
/// A rule between two runs of options.
pub use crate::menu::MenuSeparator as ComboboxSeparator;
/// The props of [`ComboboxSeparator`].
pub use crate::menu::MenuSeparatorProps as ComboboxSeparatorProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::{component, view};

use crate::listbox::Listbox;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// What the combobox's rules are installed under.
pub(crate) const SHEET: &str = "zui-combobox";

/// A text field that narrows a list, and a list one thing is chosen from.
///
/// A [`Select`](crate::Select) with a hundred options is a select nobody can use. A combobox is
/// the same control with a field in front of it: what is typed narrows the list, the arrows walk
/// what is left, and the caret never leaves the field — which is why the option being walked is
/// named to a reader by `active_descendant` rather than by being focused.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which country an address is in.
/// #[component]
/// fn Country() -> impl IntoView {
///     let country = RwSignal::new_local(String::new());
///     view! {
///         Combobox(value = country) {
///             ComboboxInput(placeholder = "Search countries", label = "Country")
///             ComboboxContent {
///                 ComboboxItem(value = "gb") {"United Kingdom"}
///                 ComboboxItem(value = "ie") {"Ireland"}
///                 ComboboxItem(value = "fr") {"France"}
///                 ComboboxEmpty {"No country by that name."}
///             }
///         }
///     }
/// }
/// ```
///
/// # Filtering
///
/// An option whose text does not contain what has been typed is **not mounted**, so it is not in
/// the list, not in the accessibility tree, and not somewhere an arrow key can land. Nothing is
/// hidden with a style rule, because a hidden option is one a reader still meets.
///
/// The match is a substring rather than a prefix: *kingdom* finds *United Kingdom*, which is what
/// anyone typing into a search field expects.
///
/// # Keyboard
///
/// Typing narrows and opens. <kbd>↓</kbd> and <kbd>↑</kbd> walk what is left; <kbd>Home</kbd> and
/// <kbd>End</kbd> jump; <kbd>Enter</kbd> chooses; <kbd>Escape</kbd> closes without choosing.
/// <kbd>Tab</kbd> leaves the field, as it must.
#[component]
pub fn Combobox(
    /// Which value is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which value starts chosen, when the combobox owns it itself.
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
    /// The field and the list.
    children: Children,
) -> impl IntoView {
    let surface = OverlayState::new(open, default_open, on_open_change).provide();
    Listbox::new(surface, value, default_value, on_change).provide();
    view! { {children.into_view_once()} }
}
