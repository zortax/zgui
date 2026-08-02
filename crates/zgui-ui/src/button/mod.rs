//! A control that does something when it is pressed.

mod style;

pub use crate::button::style::ButtonStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::UiState;
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What the button's rules are installed under.
const SHEET: &str = "zui-button";

variants! {
    /// The axes a [`Button`] varies along.
    ///
    /// ```
    /// use zgui_ui::{ButtonSize, ButtonVariant, ButtonVariants};
    ///
    /// let quiet = ButtonVariants { variant: ButtonVariant::Ghost, size: ButtonSize::Sm };
    /// assert_eq!(quiet.class_list(), "zui-button zui-button--ghost zui-button--sm");
    /// assert_eq!(
    ///     quiet.data_attributes(),
    ///     [("data-variant", "ghost"), ("data-size", "sm")]
    /// );
    /// ```
    ///
    /// The four `Icon…` sizes are square: they have no side padding, because the thing inside them
    /// is a mark rather than a word and a square is what a mark wants around it.
    ///
    /// ```
    /// use zgui_ui::{ButtonSize, ButtonVariant, ButtonVariants};
    ///
    /// let close = ButtonVariants { variant: ButtonVariant::Ghost, size: ButtonSize::IconSm };
    /// assert_eq!(close.data_attributes()[1], ("data-size", "icon-sm"));
    /// ```
    pub ButtonVariants {
        base: "zui-button",
        variant: {
            Default => "zui-button--default",
            Secondary => "zui-button--secondary",
            Destructive => "zui-button--destructive",
            Outline => "zui-button--outline",
            Ghost => "zui-button--ghost",
            Link => "zui-button--link",
        } = Default,
        size: {
            Xs => "zui-button--xs",
            Sm => "zui-button--sm",
            Md => "",
            Lg => "zui-button--lg",
            Icon => "zui-button--icon",
            IconXs => "zui-button--icon-xs",
            IconSm => "zui-button--icon-sm",
            IconLg => "zui-button--icon-lg",
        } = Md,
    }
}

/// A button.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A count, and a button that raises it.
/// #[component]
/// fn Counter() -> impl IntoView {
///     let count = RwSignal::new_local(0);
///     view! {
///         row {
///             text {{move || count.get().to_string()}}
///             Button(
///                 variant = ButtonVariant::Outline,
///                 on:click = move |_| count.update(|count| *count += 1)
///             ) {
///                 "Add one"
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// <kbd>Enter</kbd> and <kbd>Space</kbd> press it, and neither is handled here. Activating what
/// has focus is the framework's own behaviour for those two keys, dispatched as an ordinary click
/// through the ordinary path — so a button has exactly one thing to listen for however it was
/// pressed, and a `on:click` handler is reached by the pointer, by the keyboard and by an
/// accessibility action alike.
///
/// # Disabled
///
/// `disabled` sets the interaction state `:disabled` matches, which is also what takes the button
/// out of the focus order and out of what a pointer can reach, and what a reader is told. There is
/// no second copy of the answer anywhere: the appearance comes from the same state.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::reactive::RwSignal;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Form() -> impl IntoView {
/// let saving = RwSignal::new_local(false);
/// view! { Button(disabled = saving) {"Save"} }
/// # }
/// ```
#[component]
pub fn Button(
    /// How it looks.
    #[prop(default = ButtonVariant::Default)]
    variant: ButtonVariant,
    /// How big it is.
    #[prop(default = ButtonSize::Md)]
    size: ButtonSize,
    /// Whether it can be pressed.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the button's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ButtonStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let variants = ButtonVariants { variant, size };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(A11yBinding::new(Role::Button).disabled(move || disabled.get()));

    view! {
        control(class = ButtonStyle::CLASS, node_ref = element, tabindex = Focus::Sequential, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
