//! One panel of a resizable group.

use zgui::prelude::*;
use zgui::{component, view};

use crate::resizable::layout::PanelBound;
use crate::resizable::style::ResizableStyle;
use crate::resizable::{ResizableContext, SHEET};

/// One panel of a [`ResizablePanelGroup`](crate::ResizablePanelGroup).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ResizablePanelGroup {
///         ResizablePanel(default_size = 30.0, min_size = 15.0, label = "Inbox") {
///             text {"Inbox"}
///         }
///         ResizableHandle()
///         ResizablePanel(default_size = 70.0) {text {"The message"}}
///     }
/// }
/// # }
/// ```
///
/// Sizes are percentages of the group, and the group shares out whatever the declared numbers do
/// not add up to. A panel's own share reaches the layout as `--zui-panel-size`, so a caller who
/// wants a different flex rule writes one in CSS without any of this changing.
#[component]
pub fn ResizablePanel(
    /// What share of the group this panel starts with, as a percentage.
    #[prop(default = 0.0)]
    default_size: f64,
    /// The smallest share it may be squeezed to.
    #[prop(default = 0.0)]
    min_size: f64,
    /// The largest share it may be given.
    #[prop(default = 100.0)]
    max_size: f64,
    /// What the panel is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the panel holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ResizableStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = ResizableContext::current();
    let bound = PanelBound::new(min_size.clamp(0.0, 100.0), max_size.clamp(0.0, 100.0));
    let id = context.map(|group| group.register_panel(bound, default_size.clamp(0.0, 100.0)));

    let share = move || {
        let (Some(context), Some(id)) = (context, id) else {
            return 0.0;
        };
        context.size_of(id)
    };

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-panel-size"),
            move || Some(format!("{:.4}%", share())),
        )
        .a11y_from(semantics);

    view! {
        box(node_ref = element, class = "zui-resizable__panel", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
