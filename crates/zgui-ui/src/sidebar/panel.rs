//! The panel: the room it takes on the page, the surface inside that room, and its rail.

use zgui::prelude::*;
use zgui::{component, view};

use crate::sidebar::context::SidebarContext;
use crate::sidebar::shape::{SidebarCollapse, SidebarSide, SidebarVariant};
use crate::sidebar::style;

/// The panel itself.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {
///         Sidebar(label = "Project", variant = SidebarVariant::Inset) {
///             SidebarContent {text {"Files"}}
///             SidebarRail()
///         }
///         SidebarInset {text {"The document"}}
///     }
/// }
/// # }
/// ```
///
/// Announced as a complementary region, which is what a navigation panel beside a document is: a
/// reader can jump to it and jump past it, and it is not part of the main content. A panel folded
/// to icons is still that region — its entries are still there and still reachable — so only a
/// panel folded clean off the page is taken out of the tree.
///
/// # Two elements, not one
///
/// The room the panel takes on the page and the surface drawn in that room are separate: the room
/// is a plain flow item whose width the fold animates, and the surface is placed inside it. That is
/// what lets a panel slide off the edge rather than being squeezed flat against it, and it is what
/// gives the [`SidebarRail`](crate::SidebarRail) something to hang off that is not clipped.
///
/// # The three frames
///
/// [`SidebarVariant::Sidebar`] is flush against the window and ruled off from the page.
/// [`SidebarVariant::Floating`] holds the surface off the edge and rounds it into a card of its own.
/// [`SidebarVariant::Inset`] holds nothing off — it turns the *page* into the card, and tints what
/// is behind everything to the sidebar's own colour so the card reads as lifted off it.
#[component]
pub fn Sidebar(
    /// What the panel is called, for a reader.
    #[prop(into, default = String::from("Sidebar"))]
    label: String,
    /// Which side it is on, when the panel rather than the frame says.
    #[prop(optional)]
    side: Option<SidebarSide>,
    /// What folding it away leaves behind, when the panel rather than the frame says.
    #[prop(optional)]
    collapsible: Option<SidebarCollapse>,
    /// What frame the surface sits in, when the panel rather than the frame says.
    #[prop(optional)]
    variant: Option<SidebarVariant>,
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The bands inside it, when it has any yet.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    style::install();
    let context = SidebarContext::current();
    if let Some(context) = context {
        if let Some(side) = side {
            context.take_side(side);
        }
        if let Some(collapsible) = collapsible {
            context.take_collapse(collapsible);
        }
        if let Some(variant) = variant {
            context.take_variant(variant);
        }
    }

    let gone = move || {
        context.is_some_and(|context| {
            !context.is_open() && context.collapse() == SidebarCollapse::Offcanvas
        })
    };
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(
                context
                    .map_or("expanded", SidebarContext::state_name)
                    .to_owned(),
            )
        })
        .a11y_from(
            A11yBinding::new(Role::Complementary)
                .label(label)
                .hidden(gone),
        );

    view! {
        box(
            class = "zui-sidebar",
            node_ref = context.map(SidebarContext::panel).unwrap_or_default(),
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-sidebar__container") {
                column(class = "zui-sidebar__inner") {
                    {children.map(Children::into_view_once)}
                }
            }
        }
    }
}
