//! One section of an accordion, and the disclosure it publishes.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::accordion::style::AccordionStyle;
use crate::accordion::{AccordionContext, SHEET};
use crate::collapsible::CollapsibleContext;

/// One section of an [`Accordion`](crate::Accordion).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Accordion {
///         AccordionItem(value = "shipping") {
///             AccordionTrigger {"When does it ship?"}
///             AccordionContent {text {"Within two working days."}}
///         }
///     }
/// }
/// # }
/// ```
///
/// # Who owns whether it is open
///
/// The accordion does, and the section is told. That is not an implementation detail: a
/// single-selection accordion has to close one section to open another, and a section that held
/// its own answer could only find out afterwards — which is one frame with two sections open.
///
/// So the section publishes a [`CollapsibleContext`] whose value is *controlled by the accordion*:
/// reading it asks the accordion, and writing it tells the accordion, which decides. Every part
/// below is then the ordinary disclosure part, with no idea it is in a group.
#[component]
pub fn AccordionItem(
    /// What this section is called, which is what the accordion reports as open.
    #[prop(into)]
    value: String,
    /// Whether this one section can be operated, whatever the accordion says.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the section's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The heading and the content.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AccordionStyle::CSS);
    let accordion = AccordionContext::current();
    let name = std::rc::Rc::new(value);

    // The group owns which sections are open, so the section reads it and asks it — it may
    // refuse, which is how a single-selection accordion keeps one section open.
    let open = {
        let read = {
            let name = std::rc::Rc::clone(&name);
            Signal::derive_local(move || accordion.is_some_and(|group| group.is_open(&name)))
        };
        let name = std::rc::Rc::clone(&name);
        Binding::controlled(read, move |open: bool| {
            if let Some(group) = accordion {
                group.set_open(&name, open);
            }
        })
    };
    let out = Signal::derive_local(move || {
        disabled.get() || accordion.is_some_and(AccordionContext::is_disabled)
    });
    let context = CollapsibleContext::new(Controllable::new(open, false, None), out).provide();

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(context.state_name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-value"), {
            let name = std::rc::Rc::clone(&name);
            move || Some(name.to_string())
        });

    view! {
        column(class = "zui-accordion__item", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
