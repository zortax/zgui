//! Where a palette's commands are shown, and the dialog one usually lives in.

use zgui::prelude::*;
use zgui::view::ClassName;
use zgui::{component, view};

use crate::command::CommandProps;
use crate::command::SHEET;
use crate::command::style::CommandStyle;
use crate::dialog::{DialogContentProps, DialogTitleProps};
use crate::menu::MenuLabelProps;
use crate::overlay::{OverlayState, SurfaceLabels};
use zgui_ui_primitives::Binding;

/// The scrolling list a [`Command`](crate::Command) shows its results in.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Command {
///         CommandInput()
///         CommandList {CommandItem(value = "new") {"New"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn CommandList(
    /// What the list is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The groups and the commands.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CommandStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(true, None));
    let mut semantics = A11yBinding::new(Role::ListBox);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-command__list"), true)
        .a11y_from(semantics);

    view! {
        scroll(node_ref = {state.content()}, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A run of commands under a heading.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Command {CommandList {
///         CommandGroup(label = "Invoices") {CommandItem(value = "new") {"New"}}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn CommandGroup(
    /// What the group is called, shown as a heading and announced as the group's name.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The commands.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CommandStyle::CSS);
    let heading = label.clone();
    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(ClassName::new("zui-command__group"), true)
        .a11y_from(semantics);

    view! {
        column({..own}, {..attrs}, class = class) {
            {heading
                .map(|text| AnyView::new(view! { MenuLabel {{text}} }))
                .unwrap_or_else(|| AnyView::new(()))}
            {children.into_view_once()}
        }
    }
}

/// A [`Command`](crate::Command) inside a dialog, which is where a palette usually lives.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The palette, opened from somewhere else in the application.
/// #[component]
/// fn Palette() -> impl IntoView {
///     let open = RwSignal::new_local(false);
///     view! {
///         box {
///             Button(on:click = move |_| open.set(true)) {"Search…"}
///             CommandDialog(
///                 open = open,
///                 on_open_change = zgui::reactive::UnsyncCallback::new(move |next: bool| open.set(next)),
///                 title = "Commands"
///             ) {
///                 CommandInput(placeholder = "Type a command…")
///                 CommandList {CommandItem(value = "new") {"New invoice"}}
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn CommandDialog(
    /// Whether it is open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes.
    #[prop(optional)]
    on_open_change: Option<zgui::reactive::UnsyncCallback<bool>>,
    /// What the dialog is called, for a reader.
    ///
    /// A palette has no visible heading — its field is its heading — so this is written rather
    /// than shown, and a dialog with neither would be announced as an unlabelled dialog.
    #[prop(into, default = String::from("Commands"))]
    title: String,
    /// Whether the dialog draws a dismiss control in its own corner.
    #[prop(default = true)]
    dismiss_control: bool,
    /// Classes merged after the palette's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The field and the list.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, CommandStyle::CSS);
    // What `Dialog` does, written out: its open state and its labels, published here so the
    // content below finds them. Calling the component instead would mean handing an `Option`
    // through a setter that takes the value itself.
    OverlayState::new(open, default_open, on_open_change).provide();
    SurfaceLabels::provide();
    // Held rather than moved: the dialog's content is rebuilt every time it re-opens.
    let title = zgui::reactive::StoredValue::new_local(title);
    let class = zgui::reactive::StoredValue::new_local(class);
    let attrs = zgui::reactive::StoredValue::new_local(attrs);
    let children = zgui::reactive::StoredValue::new_local(children);

    view! {
        DialogContent(dismiss_control = dismiss_control, class = "zui-command__dialog") {
            DialogTitle(a11y:hidden = true) {{title.get_value()}}
            Command({..attrs.get_value()}, class = {class.get_value()}) {{children.get_value().view()}}
        }
    }
}
