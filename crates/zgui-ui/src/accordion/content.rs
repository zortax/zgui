//! The body of an accordion section.

use zgui::prelude::*;
use zgui::{component, view};

use crate::accordion::SHEET;
use crate::accordion::style::AccordionStyle;
use crate::collapsible::{CollapsibleContentProps, CollapsibleContext};

/// The body of an [`AccordionItem`](crate::AccordionItem), shown while its heading is expanded.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Accordion {
///         AccordionItem(value = "returns") {
///             AccordionTrigger {"Can I send it back?"}
///             AccordionContent {text {"For thirty days, unopened."}}
///         }
///     }
/// }
/// # }
/// ```
///
/// This *is* [`CollapsibleContent`](crate::CollapsibleContent), with a region role and a relation
/// back to the heading that opened it. The sliding, the self-measurement and the
/// `--zui-collapsible-height` it animates to are the disclosure's, written once.
#[component]
pub fn AccordionContent(
    /// Classes merged after the body's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the section holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AccordionStyle::CSS);
    let disclosure = CollapsibleContext::current();
    let mut semantics = A11yBinding::new(Role::Region);
    if let Some(disclosure) = disclosure {
        semantics = semantics.labelled_by(disclosure.trigger());
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-accordion__content"), true)
        .a11y_from(semantics);

    view! {
        CollapsibleContent({..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
