//! Everything that can be done, searched by typing.

mod input;
mod list;
mod style;

/// What a palette says when the search finds nothing.
pub use crate::combobox::ComboboxEmpty as CommandEmpty;
/// The props of [`CommandEmpty`].
pub use crate::combobox::ComboboxEmptyProps as CommandEmptyProps;
/// One command. A [`ComboboxItem`](crate::ComboboxItem), whose `on_select` is what running it is.
pub use crate::combobox::ComboboxItem as CommandItem;
/// The props of [`CommandItem`].
pub use crate::combobox::ComboboxItemProps as CommandItemProps;
pub use crate::command::input::{CommandInput, CommandInputProps};
pub use crate::command::list::{
    CommandDialog, CommandDialogProps, CommandGroup, CommandGroupProps, CommandList,
    CommandListProps,
};
pub use crate::command::style::CommandStyle;
/// A rule between two groups of commands.
pub use crate::menu::MenuSeparator as CommandSeparator;
/// The props of [`CommandSeparator`].
pub use crate::menu::MenuSeparatorProps as CommandSeparatorProps;
/// The keystroke that runs a command without opening the palette.
pub use crate::menu::MenuShortcut as CommandShortcut;
/// The props of [`CommandShortcut`].
pub use crate::menu::MenuShortcutProps as CommandShortcutProps;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::view::ClassName;
use zgui::{component, view};

use crate::listbox::Listbox;
use crate::overlay::OverlayState;
use zgui_ui_primitives::Binding;

/// What the palette's rules are installed under.
pub(crate) const SHEET: &str = "zui-command";

/// A searchable list of everything that can be done.
///
/// It is the same machinery as a [`Combobox`](crate::Combobox) with one difference, and the
/// difference is the whole of what makes it a palette: its list is **not** a popup. It is on the
/// surface it was written on — usually a [`CommandDialog`] — so choosing something must not try to
/// close a surface that is not there, and the arrow keys must not try to re-open one.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::UnsyncCallback;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Everything this application can do, in one list.
/// #[component]
/// fn Palette() -> impl IntoView {
///     view! {
///         Command {
///             CommandInput(placeholder = "Type a command…", label = "Command")
///             CommandList {
///                 CommandGroup(label = "Invoices") {
///                     CommandItem(
///                         value = "invoice.new",
///                         text = "New invoice",
///                         on_select = UnsyncCallback::new(|()| println!("new invoice"))
///                     ) {
///                         "New invoice"
///                     }
///                     CommandItem(value = "invoice.export", text = "Export invoices") {
///                         "Export invoices"
///                     }
///                 }
///                 CommandEmpty {"Nothing by that name."}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// Typing narrows. <kbd>↓</kbd> and <kbd>↑</kbd> walk what is left and wrap; <kbd>Home</kbd> and
/// <kbd>End</kbd> jump; <kbd>Enter</kbd> runs whatever is highlighted. The caret never leaves the
/// field, so the highlighted command is named to a reader by `active_descendant`.
#[component]
pub fn Command(
    /// Which command is highlighted as chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which one starts chosen.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told whenever one is chosen, whichever way it was chosen.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The field and the list.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CommandStyle::CSS);
    // Always on the surface it was written on. Nothing opens it and nothing closes it, which is
    // exactly what `inline` says to the keyboard model.
    let surface = OverlayState::uncontrolled(true, None).provide();
    Listbox::new(surface, value, default_value, on_change)
        .inline()
        .provide();

    let own = Attrs::new()
        .class_toggle(ClassName::new(CommandStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-command"), true);

    view! {
        column({..own}, {..attrs}, class = class) {{children.into_view_once()}}
    }
}
