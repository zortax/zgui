//! The one field a palette is driven from.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::ui::SEARCH;

use crate::combobox::ComboboxInputProps;
use crate::command::SHEET;
use crate::command::style::CommandStyle;

/// The field a [`Command`](crate::Command) is searched with.
///
/// A band across the top of the palette rather than a box inside one: a magnifier, the text being
/// typed, and a rule underneath dividing the question from the answers. What the eye reads as the
/// search box is the whole band, and a bordered field drawn inside it would be a box in a box.
///
/// The typing itself is a [`ComboboxInput`](crate::ComboboxInput)'s — narrowing a list and letting
/// the arrows walk what is left is the same question a combobox answers, and it is answered once.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Command {
///         CommandInput(placeholder = "Type a command…", label = "Command")
///         CommandList {CommandItem(value = "new") {"New invoice"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn CommandInput(
    /// What to show while it is empty.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What the field is called, for a reader.
    ///
    /// "Search" unless something better is written, because a palette has no visible heading over
    /// its field — the field *is* the heading — and an unnamed one would be announced as an
    /// anonymous combo box.
    #[prop(into, default = String::from("Search"))]
    label: String,
    /// The element whose text names it.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Classes merged after the field's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the field itself.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, CommandStyle::CSS);
    view! {
        row(class = "zui-command__field") {
            Icon(icon = SEARCH)
            ComboboxInput(
                placeholder = placeholder.unwrap_or_default(),
                disabled = disabled,
                label = label,
                labelled_by = labelled_by,
                class = "zui-command__input",
                {..attrs},
                class = class
            )
        }
    }
}
