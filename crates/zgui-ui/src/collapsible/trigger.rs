//! The control that opens and closes a disclosure.

use zgui::prelude::*;
use zgui::vocab::UiState;
use zgui::{component, view};

use crate::collapsible::style::CollapsibleStyle;
use crate::collapsible::{CollapsibleContext, SHEET};

/// The control that shows and hides a [`Collapsible`](crate::Collapsible).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Collapsible {
///         CollapsibleTrigger {"Delivery details"}
///         CollapsibleContent {text {"Arrives Thursday."}}
///     }
/// }
/// # }
/// ```
///
/// # What a reader is told
///
/// That it is a button, whether the thing it controls is expanded, and *which* thing that is —
/// the last of those as a relation to the content's own element rather than as a repeated label,
/// so the two cannot come apart.
#[component]
pub fn CollapsibleTrigger(
    /// Classes merged after the trigger's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the trigger shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CollapsibleStyle::CSS);
    let context = CollapsibleContext::current();
    let open = move || context.is_some_and(|context| context.is_open());
    let is_out = move || context.is_some_and(|context| context.is_disabled());

    let mut semantics = A11yBinding::new(Role::Button)
        .expanded(open)
        .disabled(is_out);
    if let Some(context) = context {
        semantics = semantics.controls(context.content());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .state(UiState::DISABLED, is_out)
        .a11y_from(semantics);

    view! {
        control(
            node_ref = context.map(CollapsibleContext::trigger).unwrap_or_default(),
            class = "zui-collapsible__trigger",
            tabindex = {Focus::Sequential},
            on:click = move |_| { if let Some(context) = context { context.toggle() } },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
