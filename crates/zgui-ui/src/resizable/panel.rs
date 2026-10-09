//! One panel of a resizable group.

use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui::{component, view};
use zgui_ui_primitives::Orientation;

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
/// not add up to. A panel's own share reaches the layout as its `flex-basis`.
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
    /// The smallest the panel may be squeezed to, in CSS pixels.
    ///
    /// Read as a share of the group again whenever the group's box changes, so the floor stays the
    /// same number of pixels however the window is resized. It holds against `min_size`, and zero
    /// leaves `min_size` as the only floor.
    #[prop(default = 0.0)]
    min_pixels: f64,
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

    // A floor in pixels is a share that changes with the group, so it is watched rather than
    // worked out once.
    let floor = match (context, id) {
        (Some(context), Some(id)) if min_pixels > 0.0 => {
            let group = context.group();
            let watching = group.observe_border_size();
            let vertical = matches!(context.direction(), Orientation::Vertical);
            Some(RenderEffect::new(move |_| {
                let Some(measured) = watching.get() else {
                    return;
                };
                let length = if vertical {
                    measured.height.0
                } else {
                    measured.width.0
                };
                if length <= 0.0 {
                    return;
                }
                let scale = if group.scale() > 0.0 {
                    group.scale()
                } else {
                    1.0
                };
                let share = f64::from(scale) * min_pixels / f64::from(length) * 100.0;
                let min = bound.min.max(share).clamp(0.0, bound.max);
                context.set_bound(id, PanelBound::new(min, bound.max));
            }))
        }
        _ => None,
    };
    on_cleanup_local(move || drop(floor));

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
    // A basis on the panel itself rather than an inherited custom property: a step of a drag then
    // restyles the panel, and nothing inside it inherits a new value.
    let own = Attrs::new()
        .style_property("flex-basis", move || Some(format!("{:.4}%", share())))
        .a11y_from(semantics);

    view! {
        box(node_ref = element, class = "zui-resizable__panel", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
