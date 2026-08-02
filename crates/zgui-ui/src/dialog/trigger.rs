//! What opens a dialog, and what closes it.

use zgui::prelude::*;
use zgui::vocab::HasPopup;
use zgui::{component, view};

use crate::button::{ButtonProps, ButtonSize, ButtonVariant};
use crate::overlay::OverlayState;

/// The control that opens the enclosing [`Dialog`](crate::Dialog).
///
/// It is a [`Button`](crate::Button) and takes the same variant and size, because that is what it is: the only
/// thing this adds is the relation to the surface it opens and the state a style sheet turns a
/// chevron over on.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {
///         DialogTrigger(variant = ButtonVariant::Outline) {"Rename…"}
///         DialogContent {DialogTitle {"Rename"}}
///     }
/// }
/// # }
/// ```
///
/// Outside a dialog it is an ordinary button that opens nothing, which is what a trigger with
/// nothing to trigger amounts to.
#[component]
pub fn DialogTrigger(
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

/// A control that closes the enclosing [`Dialog`](crate::Dialog).
///
/// The reason it exists rather than a callback the caller writes: a dialog's "Cancel" sits in the
/// footer, three components below the root that owns whether the dialog is open, and threading a
/// setter down to it is a prop on every component in between.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Dialog {
///         DialogContent {
///             DialogFooter {
///                 DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
///             }
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn DialogClose(
    /// How it looks.
    #[prop(default = ButtonVariant::Ghost)]
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
