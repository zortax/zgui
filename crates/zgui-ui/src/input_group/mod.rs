//! A text field with marks, words or controls attached to it.

mod parts;
pub(crate) mod style;

pub use crate::input_group::parts::{
    InputGroupAddon, InputGroupAddonAlign, InputGroupAddonProps, InputGroupAddonVariants,
    InputGroupButton, InputGroupButtonProps, InputGroupButtonSize, InputGroupInput,
    InputGroupInputProps, InputGroupText, InputGroupTextProps, InputGroupTextarea,
    InputGroupTextareaProps,
};
pub use crate::input_group::style::{InputGroupPartStyle, InputGroupStyle};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

/// What the group's own rules are installed under.
pub(crate) const SHEET: &str = "zui-input-group";

/// What the rules for the pieces attached to it are installed under.
pub(crate) const PARTS_SHEET: &str = "zui-input-group-parts";

/// A field and whatever is fixed to it, framed as one control.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Somewhere to search, with the mark that says so.
/// #[component]
/// fn Search() -> impl IntoView {
///     view! {
///         InputGroup {
///             InputGroupAddon {"🔍"}
///             InputGroupInput(placeholder = "Search")
///             InputGroupAddon(align = InputGroupAddonAlign::InlineEnd) {KbdGroup {Kbd {"/"}}}
///         }
///     }
/// }
/// ```
///
/// # Why the field gives up its frame
///
/// The border, the rounding and the focus ring are the group's. A field that kept its own would
/// draw a second box inside the first and the two would disagree the moment either was focused —
/// so [`InputGroupInput`] and [`InputGroupTextarea`] are stripped versions of
/// [`Input`](crate::Input) and [`Textarea`](crate::Textarea) that expect to be framed by something
/// else.
///
/// # What a reader is told
///
/// Whatever the field says. The group is a frame, and a mark attached to it is decoration unless
/// it is a control — which is what [`InputGroupButton`] is for, and why that one takes a name.
#[component]
pub fn InputGroup(
    /// Whether the whole thing can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether what the field holds is wrong, which reddens the frame around the lot.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The field, and what is attached to it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, InputGroupStyle::CSS);
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-input-group"), true)
        .attribute(zgui::view::AttrName::new("data-disabled"), move || {
            Some(if disabled.get() { "true" } else { "false" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-invalid"), move || {
            Some(if invalid.get() { "true" } else { "false" }.to_owned())
        })
        .a11y_from(A11yBinding::new(Role::Group));

    view! {
        box(class = InputGroupStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
