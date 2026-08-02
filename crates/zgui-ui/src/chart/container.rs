//! The box a chart is given to fill.

use zgui::prelude::*;
use zgui::{component, view};

use crate::chart::SHEET;
use crate::chart::style::ChartStyle;

/// The box a chart is drawn in.
///
/// A chart has no size of its own to speak of — it is as large as it is given room to be — and this
/// is what gives it that room: a wide box of a fixed shape, with whatever is inside it centred. It
/// also sets the small type every label, tick and key inherits, so no part of a chart has to name a
/// size for itself.
#[component]
pub fn ChartContainer(
    /// Classes merged after the container's own.
    #[prop(into, optional)]
    class: Classes,
    /// Where to record the container's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The chart.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ChartStyle::CSS);
    let element = node_ref.unwrap_or_default();
    view! {
        box(node_ref = element, class = "zui-chart__container", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
