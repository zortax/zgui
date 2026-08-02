//! What an [`InputGroup`](crate::InputGroup) holds.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, variants, view};
use zgui_ui_primitives::Binding;

use crate::button::{ButtonProps, ButtonSize, ButtonVariant};
use crate::input::InputProps;
use crate::input_group::PARTS_SHEET;
use crate::input_group::style::InputGroupPartStyle;
use crate::support::variant_attrs;
use crate::textarea::TextareaProps;

variants! {
    /// The axes an [`InputGroupAddon`] varies along.
    pub InputGroupAddonVariants {
        base: "zui-input-group__addon",
        align: {
            InlineStart => "",
            InlineEnd => "zui-input-group__addon--inline-end",
            BlockStart => "zui-input-group__addon--block-start",
            BlockEnd => "zui-input-group__addon--block-end",
        } = InlineStart,
    }
}

/// How large a control attached to an [`InputGroup`](crate::InputGroup) is.
///
/// Its own scale rather than a [`Button`](crate::Button)'s, and smaller at every rung: a control
/// fixed to a field has to sit inside the field's own height with air around it, so the largest of
/// these is the size an ordinary small button is. The two icon sizes are square, for a control that
/// is a mark and nothing else.
///
/// ```
/// use zgui_ui::input_group::InputGroupButtonSize;
///
/// assert_eq!(InputGroupButtonSize::default(), InputGroupButtonSize::Xs);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum InputGroupButtonSize {
    /// A word, at the smallest height a control here comes in.
    #[default]
    Xs,
    /// A word, at the height of a small button.
    Sm,
    /// A mark alone, square, at the smallest height.
    IconXs,
    /// A mark alone, square, at the height of a small button.
    IconSm,
}

impl InputGroupButtonSize {
    /// Which of a [`Button`](crate::Button)'s own sizes this one is drawn as.
    ///
    /// The button writes the size it was given onto its element, so a group's sheet has something
    /// to select on — which is why this is a mapping onto that scale rather than a scale of its
    /// own that nothing would report.
    const fn button(self) -> ButtonSize {
        match self {
            Self::Xs => ButtonSize::Xs,
            Self::Sm => ButtonSize::Sm,
            Self::IconXs => ButtonSize::IconXs,
            Self::IconSm => ButtonSize::IconSm,
        }
    }
}

/// Something fixed to one edge of an [`InputGroup`](crate::InputGroup).
///
/// The two inline alignments put it beside the field on the same line; the two block ones give it
/// a row of its own above or below, which is what a toolbar under a message box wants.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { InputGroup {InputGroupAddon {"🔍"} InputGroupInput()} }
/// # }
/// ```
#[component]
pub fn InputGroupAddon(
    /// Which edge it is fixed to.
    #[prop(default = InputGroupAddonAlign::InlineStart)]
    align: InputGroupAddonAlign,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The mark, words or controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    let variants = InputGroupAddonVariants { align };
    let own = variant_attrs(variants.classes(), variants.data_attributes());

    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// Words attached to a field: a unit, a prefix, a count of what is left.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     InputGroup {
///         InputGroupInput()
///         InputGroupAddon(align = InputGroupAddonAlign::InlineEnd) {InputGroupText {"kg"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn InputGroupText(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    view! {
        box(class = "zui-input-group__text", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A control fixed to a field: clear it, reveal a password, send what was typed.
///
/// It is a real [`Button`](crate::Button), so it needs a name of its own — the field beside it
/// says what the field is for, not what pressing this does. It is quiet and small by default,
/// because it sits inside a frame that is already drawn and must not compete with the words being
/// typed next to it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::input_group::InputGroupButtonSize;
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     InputGroup {
///         InputGroupInput()
///         InputGroupAddon(align = InputGroupAddonAlign::InlineEnd) {
///             InputGroupButton(size = InputGroupButtonSize::IconXs, a11y:label = "Clear") {"×"}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn InputGroupButton(
    /// How it looks.
    #[prop(default = ButtonVariant::Ghost)]
    variant: ButtonVariant,
    /// How big it is.
    #[prop(default = InputGroupButtonSize::Xs)]
    size: InputGroupButtonSize,
    /// Whether it can be pressed.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    view! {
        Button(
            variant = variant,
            size = {size.button()},
            disabled = disabled,
            class = "zui-input-group__button",
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// The field an [`InputGroup`](crate::InputGroup) is built around.
///
/// An [`Input`](crate::Input) with its own border, rounding, shadow and ring taken away, because
/// the group draws those once around everything.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { InputGroup {InputGroupInput(placeholder = "Search")} }
/// # }
/// ```
#[component]
pub fn InputGroupInput(
    /// What it holds, when the caller holds it.
    #[prop(into, optional)]
    value: Binding<String>,
    /// What it starts as, when the field owns it itself.
    #[prop(into, default = String::new())]
    default_value: String,
    /// Told on every change, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What to show while it is empty.
    #[prop(into, default = String::new())]
    placeholder: String,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether what it holds is wrong, which a reader is told even though the frame around it is
    /// what shows it.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// Where to record the field's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    view! {
        Input(
            value = value,
            default_value = default_value,
            on_change = on_change,
            placeholder = placeholder,
            disabled = disabled,
            invalid = invalid,
            node_ref = node_ref,
            class = "zui-input-group__field",
            {..attrs},
            class = class
        )
    }
}

/// The many-line field an [`InputGroup`](crate::InputGroup) is built around.
///
/// A [`Textarea`](crate::Textarea) stripped the same way as [`InputGroupInput`], for a group whose
/// controls sit under the writing rather than beside it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     InputGroup {
///         InputGroupTextarea(placeholder = "Say something")
///         InputGroupAddon(align = InputGroupAddonAlign::BlockEnd) {
///             InputGroupButton {"Send"}
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn InputGroupTextarea(
    /// What it holds, when the caller holds it.
    #[prop(into, optional)]
    value: Binding<String>,
    /// What it starts as, when the field owns it itself.
    #[prop(into, default = String::new())]
    default_value: String,
    /// Told on every change, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What to show while it is empty.
    #[prop(into, default = String::new())]
    placeholder: String,
    /// Whether it can be typed into.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether what it holds is wrong, which a reader is told even though the frame around it is
    /// what shows it.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// Where to record the field's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(PARTS_SHEET, InputGroupPartStyle::CSS);
    view! {
        Textarea(
            value = value,
            default_value = default_value,
            on_change = on_change,
            placeholder = placeholder,
            disabled = disabled,
            invalid = invalid,
            node_ref = node_ref,
            class = "zui-input-group__area",
            {..attrs},
            class = class
        )
    }
}
