//! The heading of an accordion section, and the button inside it.

use zgui::prelude::*;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;
use zgui_ui_primitives::use_roving_item;

use crate::accordion::SHEET;
use crate::accordion::style::AccordionStyle;
use crate::collapsible::CollapsibleContext;

/// The heading of an [`AccordionItem`](crate::AccordionItem), which opens and closes it.
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
///             AccordionTrigger(level = 2) {"Can I send it back?"}
///             AccordionContent {text {"For thirty days."}}
///         }
///     }
/// }
/// # }
/// ```
///
/// # Two elements, and why
///
/// A heading with a button inside it, which is what the authoring practices ask for and what makes
/// an accordion navigable: a reader jumping by heading meets every section's title, and the button
/// inside each one is what opens it. One element could be a heading or a button and not both.
///
/// # The chevron
///
/// Drawn by the trigger and turned over as the section opens, hidden from a reader and deaf to the
/// pointer: it says the same thing the expanded state already says, and a reader that met both
/// would say it twice.
///
/// # Keyboard
///
/// The arrows, <kbd>Home</kbd> and <kbd>End</kbd> move between the headings without opening
/// anything; <kbd>Enter</kbd> and <kbd>Space</kbd> open whichever one they landed on. The first
/// two are the enclosing accordion's; the last is the framework's activation of what has focus.
#[component]
pub fn AccordionTrigger(
    /// What heading level this section's title is, for a reader jumping by heading.
    #[prop(default = 3)]
    level: usize,
    /// Classes merged after the trigger's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The title of the section.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AccordionStyle::CSS);
    let disclosure = CollapsibleContext::current();
    let node = disclosure
        .map(CollapsibleContext::trigger)
        .unwrap_or_default();
    let item = use_roving_item(node);

    let open = move || disclosure.is_some_and(CollapsibleContext::is_open);
    let is_out = move || disclosure.is_some_and(CollapsibleContext::is_disabled);

    let mut semantics = A11yBinding::new(Role::Button)
        .expanded(open)
        .disabled(is_out);
    if let Some(disclosure) = disclosure {
        semantics = semantics.controls(disclosure.content());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if open() { "open" } else { "closed" }.to_owned())
        })
        .state(UiState::DISABLED, is_out)
        .a11y_from(semantics);

    let heading = A11yBinding::new(Role::Heading).level(level);

    view! {
        box(class = "zui-accordion__header", {..Attrs::new().a11y_from(heading)}) {
            control(
                class = "zui-accordion__trigger",
                node_ref = node,
                tabindex = move || item.map_or(Focus::Sequential, |item| item.tabindex().get()),
                on:focus_in = move |_| { if let Some(item) = item { item.activate() } },
                on:click = move |_| { if let Some(disclosure) = disclosure { disclosure.toggle() } },
                {..own},
                {..attrs},
                class = class
            ) {
                {children.into_view_once()}
                Icon(icon = CHEVRON_DOWN, class = "zui-accordion__chevron")
            }
        }
    }
}
