//! Text that names a control, and hands presses to it.

mod style;

pub use crate::label::style::LabelStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the label's rules are installed under.
const SHEET: &str = "zui-label";

/// Text naming a control, which focuses that control when it is pressed.
///
/// A label is only a label if something points at it, and the pointing is done from the control's
/// side: an accessibility tree relates a control to its name, not a name to its control. So a
/// label is given a [`NodeRef`] like any other element, and the control names it:
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A checkbox with a name a reader can announce and a person can click.
/// #[component]
/// fn Terms() -> impl IntoView {
///     let name = NodeRef::new();
///     let box_ = NodeRef::new();
///     let accepted = RwSignal::new_local(Checked::No);
///     view! {
///         row {
///             Checkbox(node_ref = box_, checked = accepted, labelled_by = name)
///             Label(node_ref = name, control = box_) {"I accept the terms"}
///         }
///     }
/// }
/// ```
///
/// # Pressing it
///
/// A press on a label moves focus to the control it names, which is the whole reason a label is
/// worth clicking. Nothing else happens: a label does not activate its control, because a label
/// on a text field would then be a field that selects itself when its name is touched.
#[component]
pub fn Label(
    /// The control this names. A press here focuses it.
    #[prop(optional)]
    control: Option<NodeRef>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the label's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it says.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, LabelStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-label"), true)
        .a11y_from(A11yBinding::new(Role::Label));

    view! {
        label(
            node_ref = element,
            class = LabelStyle::CLASS,
            on:click = move |_| {
                if let Some(control) = control {
                    control.focus();
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
