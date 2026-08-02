//! What opens an alert dialog, and the two ways out of one.

use zgui::prelude::*;
use zgui::vocab::HasPopup;
use zgui::{component, view};

use crate::alert_dialog::SHEET;
use crate::alert_dialog::style::AlertDialogStyle;
use crate::button::{ButtonProps, ButtonSize, ButtonVariant};
use crate::overlay::OverlayState;

/// The picture above an alert dialog's question.
///
/// A tinted square with a symbol in it, sized so the symbol reads at a glance: a warning triangle
/// over "Delete this project?", a key over "Your session has expired". It is decoration and is
/// declared so — everything it conveys is written underneath it in words, and a reader that
/// announced it would announce the same thing twice.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::alert_dialog::{AlertDialogMedia, AlertDialogMediaProps, AlertDialogSize};
/// # use zgui_ui_icons::prelude::*;
/// # use zgui_ui_icons::set::mark::CROSS;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AlertDialog {AlertDialogContent(size = AlertDialogSize::Sm) {
///         AlertDialogHeader {
///             AlertDialogMedia {Icon(icon = CROSS)}
///             AlertDialogTitle {"Delete this project?"}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn AlertDialogMedia(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The symbol.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AlertDialogStyle::CSS);
    view! {
        box(class = "zui-alert-dialog__media", a11y:hidden = true, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// The control that opens the enclosing [`AlertDialog`](crate::AlertDialog).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AlertDialog {
///         AlertDialogTrigger(variant = ButtonVariant::Destructive) {"Delete"}
///         AlertDialogContent {AlertDialogTitle {"Sure?"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn AlertDialogTrigger(
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
    let own = state.map_or_else(Attrs::new, |state| state.trigger_attrs(HasPopup::Dialog));
    let node = state.map_or_else(NodeRef::new, |state| state.trigger());

    view! {
        Button(
            node_ref = node,
            variant = variant,
            size = size,
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

/// The answer that goes ahead, and closes the alert dialog.
///
/// Whatever it is that is going ahead is the caller's `on:click`; this closes the surface. Both
/// run, the caller's first, because a component's own listener and a caller's accumulate rather
/// than replacing each other.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AlertDialog {AlertDialogContent {AlertDialogFooter {
///         AlertDialogAction(variant = ButtonVariant::Destructive) {"Delete"}
///     }}}
/// }
/// # }
/// ```
#[component]
pub fn AlertDialogAction(
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
    view! {
        Button(
            variant = variant,
            size = size,
            on:click = move |_| {
                if let Some(state) = state {
                    state.close();
                }
            },
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The answer that changes nothing, and closes the alert dialog.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AlertDialog {AlertDialogContent {AlertDialogFooter {
///         AlertDialogCancel {"Keep it"}
///     }}}
/// }
/// # }
/// ```
#[component]
pub fn AlertDialogCancel(
    /// How it looks.
    #[prop(default = ButtonVariant::Outline)]
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
    view! {
        Button(
            variant = variant,
            size = size,
            on:click = move |_| {
                if let Some(state) = state {
                    state.close();
                }
            },
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
