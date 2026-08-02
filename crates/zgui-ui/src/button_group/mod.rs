//! Buttons that belong together, drawn as one strip.

mod parts;
mod style;

pub use crate::button_group::parts::{
    ButtonGroupSeparator, ButtonGroupSeparatorProps, ButtonGroupText, ButtonGroupTextProps,
};
pub use crate::button_group::style::ButtonGroupStyle;

use zgui::prelude::*;
use zgui::{component, view};

use crate::support::variant_attrs;
use zgui::variants;

/// What the group's rules are installed under.
pub(crate) const SHEET: &str = "zui-button-group";

variants! {
    /// The axes a [`ButtonGroup`] varies along.
    pub ButtonGroupVariants {
        base: "zui-button-group",
        orientation: {
            Horizontal => "",
            Vertical => "zui-button-group--vertical",
        } = Horizontal,
    }
}

/// Several controls joined along one seam.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A split control: the thing to do, and the other things to do.
/// #[component]
/// fn Actions() -> impl IntoView {
///     view! {
///         ButtonGroup {
///             Button(variant = ButtonVariant::Outline) {"Merge"}
///             Button(variant = ButtonVariant::Outline) {"▾"}
///         }
///     }
/// }
/// ```
///
/// # A group and a toolbar
///
/// A group is a *shape*: its members keep their own tab stops and are reached with
/// <kbd>Tab</kbd>, exactly as if they were written apart. A
/// [`ToggleGroup`](crate::ToggleGroup) is the other thing — one tab stop and the arrow keys —
/// because its members are alternatives rather than separate actions.
///
/// # What a reader is told
///
/// That the controls form a group, so their relationship survives for somebody who cannot see the
/// seam. Name it with `a11y:label` when the group is doing something the individual buttons do not
/// say on their own.
#[component]
pub fn ButtonGroup(
    /// Which way the strip runs.
    #[prop(default = ButtonGroupOrientation::Horizontal)]
    orientation: ButtonGroupOrientation,
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The controls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ButtonGroupStyle::CSS);
    let variants = ButtonGroupVariants { orientation };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .a11y_from(A11yBinding::new(Role::Group));

    view! {
        box(class = ButtonGroupStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
