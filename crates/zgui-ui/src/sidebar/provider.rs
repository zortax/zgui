//! The frame a sidebar and the page beside it live in.

use core::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, UnsyncCallback};
use zgui::vocab::Key;
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::sidebar::context::SidebarContext;
use crate::sidebar::shape::{SidebarCollapse, SidebarSide, SidebarVariant};
use crate::sidebar::style::{self, SidebarStyle};

/// The frame a [`Sidebar`](crate::Sidebar) and the page beside it live in.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A window with a navigation panel down the side.
/// #[component]
/// fn Workspace() -> impl IntoView {
///     view! {
///         SidebarProvider {
///             Sidebar(label = "Project") {
///                 SidebarHeader {text {"zgui"}}
///                 SidebarContent {
///                     SidebarGroup {
///                         SidebarGroupLabel {"Views"}
///                         SidebarGroupContent {
///                             SidebarMenu {
///                                 SidebarMenuItem {
///                                     SidebarMenuButton(active = true) {"Files"}
///                                 }
///                                 SidebarMenuItem {
///                                     SidebarMenuButton {"Search"}
///                                 }
///                             }
///                         }
///                     }
///                 }
///                 SidebarFooter {text {"Signed in"}}
///                 SidebarRail()
///             }
///             SidebarInset {
///                 SidebarTrigger()
///                 text {"The document"}
///             }
///         }
///     }
/// }
/// ```
///
/// # The width is one property
///
/// `--zui-sidebar-width` and `--zui-sidebar-width-icon` are what everything is laid out from, so an
/// application that wants a wider panel writes one declaration rather than overriding a rule per
/// part. Folding it away is a change to which of the two is in force, which is a transition a style
/// sheet can run.
///
/// # The keyboard shortcut
///
/// <kbd>Ctrl</kbd>+<kbd>B</kbd> folds the panel away and brings it back, from anywhere in the
/// window. The listener goes on the window's own root rather than on anything this component
/// renders, because a shortcut that only worked while the sidebar had focus would be a shortcut
/// nobody could use to get *to* the sidebar.
///
/// # Where the shape is written
///
/// Here, or on the panel. The frame's rules reach every part of the sidebar — the labels that fade
/// out, the entries that square up, the page that is pushed over — so the side, the collapsed form
/// and the variant are attributes of the *frame*. A [`Sidebar`](crate::Sidebar) that names any of
/// them tells the frame, which is why the panel may be written the way it reads:
/// `Sidebar(variant = SidebarVariant::Inset)`.
#[component]
pub fn SidebarProvider(
    /// Whether the panel is open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts open, when the frame owns that itself.
    #[prop(default = true)]
    default_open: bool,
    /// Told whenever the panel folds or unfolds, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// Which side the panel is on, unless the panel says otherwise.
    #[prop(default = SidebarSide::Left)]
    side: SidebarSide,
    /// What folding it away leaves behind, unless the panel says otherwise.
    #[prop(default = SidebarCollapse::Icon)]
    collapse: SidebarCollapse,
    /// What frame the panel's surface sits in, unless the panel says otherwise.
    #[prop(default = SidebarVariant::Sidebar)]
    variant: SidebarVariant,
    /// Whether <kbd>Ctrl</kbd>+<kbd>B</kbd> folds the panel from anywhere in the window.
    #[prop(default = true)]
    shortcut: bool,
    /// Classes merged after the frame's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The panel and the page beside it.
    children: Children,
) -> impl IntoView {
    style::install();
    let element = NodeRef::new();
    let context = SidebarContext::new(
        Controllable::new(open, default_open, on_open_change),
        side,
        collapse,
        variant,
    );
    provide_local_context(context);

    // The listener lives on the window's root and is held for as long as this component is. It is
    // attached from an effect because the handle it is reached through only binds once this
    // component's own element has been built.
    let held: Rc<RefCell<Option<ListenerGuard>>> = Rc::new(RefCell::new(None));
    let watching = {
        let held = Rc::clone(&held);
        RenderEffect::new(move |_| {
            if !shortcut || element.get().is_none() || held.borrow().is_some() {
                return;
            }
            let Some(root) = element.window_root() else {
                return;
            };
            *held.borrow_mut() = root.listen(
                events::KEY_DOWN,
                zgui::vocab::ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::KeyDown>| {
                    let asked = ev.modifiers.control()
                        && matches!(&ev.key, Key::Character(text) if text.eq_ignore_ascii_case("b"));
                    if asked {
                        context.toggle();
                        ev.prevent_default();
                        ev.stop_propagation();
                    }
                },
            );
        })
    };
    on_cleanup_local(move || drop(watching));

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-sidebar-provider"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(context.state_name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-side"), move || {
            Some(context.side().name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-collapsible"), move || {
            Some(context.collapse().name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-variant"), move || {
            Some(context.variant().name().to_owned())
        });

    view! {
        box(node_ref = element, class = SidebarStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
